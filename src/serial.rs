use color_eyre::eyre::{bail, WrapErr};
use libftd2xx::{BitsPerWord, Ftdi, FtdiCommon, Parity, StopBits};
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::thread::{sleep, spawn};
use std::time::Duration;


use crate::crc::calc_crc16_default;
use crate::SERIAL_TIMEOUT;
use color_eyre::Result;

const DFU_INIT_PACKET: u32 = 1;
const DFU_START_PACKET: u32 = 3;
const DFU_DATA_PACKET: u32 = 4;
const DFU_STOP_DATA_PACKET: u32 = 5;
const DFU_MAX_PACKET_SIZE: usize = 512;
const SEND_START_DFU_WAIT_TIME: Duration = Duration::from_secs(2);
const SEND_INIT_PACKET_WAIT_TIME: Duration = Duration::from_secs(1);

pub struct Serial {
    port: Ftdi,
    pub(crate) path: PathBuf,
    sequence_number: u8,
}

impl Serial {
    pub fn open(path: PathBuf) -> Result<Self> {
        // let mut port = SerialPort::open(&path, |mut s: Settings| {
        //     s.set_raw();
        //     s.set_baud_rate(921_600)?;
        //     s.set_flow_control(FlowControl::RtsCts);
        //     Ok(s)
        // })
        // .wrap_err("failed to open serial port")?;
        //
        // port.set_read_timeout(SERIAL_TIMEOUT).wrap_err("failed to set read timeout")?;
        // port.set_write_timeout(SERIAL_TIMEOUT)
        //     .wrap_err("failed to set write timeout")?;
        //
        // port.discard_buffers().wrap_err("flush")?;

        let mut port = Ftdi::new()?;
        port.set_data_characteristics(BitsPerWord::Bits8, StopBits::Bits1, Parity::No)?;
        port.set_baud_rate(921_600)?;
        port.set_flow_control_rts_cts()?;
        port.set_timeouts(SERIAL_TIMEOUT, SERIAL_TIMEOUT)?;
        port.purge_all()?;

         Ok(Self {
            port,
            path,
            sequence_number: 0,
        })
    }

    fn next_sequence_number(&mut self) -> u8 {
        self.sequence_number = (self.sequence_number + 1) % 8;
        self.sequence_number
    }

    /// For a description of the SLIP header go to:
    /// http://developer.nordicsemi.com/nRF51_SDK/doc/7.2.0/s110/html/a00093.html
    fn create_slip_header(&mut self, pkt_len: usize) -> ([u8; 4], u8) {
        assert!(pkt_len < 0x1000);

        // sequence number
        let seq = self.next_sequence_number();
        // data integrity check (yes we always have a CRC)
        let dip = true as u8;
        // reliable packet (yes, our (USB) connection is reliable)
        let rp = true as u8;

        // we always send HCI packet, pkt type 14.
        let pkt_type = 14;

        let b1 = seq | (((seq + 1) % 8) << 3) | (dip << 6) | (rp << 7);
        let b2 = pkt_type | ((pkt_len & 0x00f) << 4) as u8;
        let b3 = ((pkt_len & 0xff0) >> 4) as u8;

        (
            [
                b1,
                b2,
                b3,
                (!b1.wrapping_add(b2).wrapping_add(b3)).wrapping_add(1),
            ],
            seq,
        )
    }

    fn encode_int(i: u32) -> [u8; 4] {
        i.to_le_bytes()
    }

    fn create_packet(&mut self, data: &[u8]) -> (Vec<u8>, u8) {
        let mut temp_res = Vec::new();

        let (bytes, seq_nr) = self.create_slip_header(data.len());
        // create header
        temp_res.extend_from_slice(&bytes);
        // add data
        temp_res.extend_from_slice(data);
        // add crc
        temp_res.extend_from_slice(&calc_crc16_default(&temp_res).to_le_bytes());

        (Self::escape(&temp_res), seq_nr)
    }

    fn escape(unescaped: &[u8]) -> Vec<u8> {
        let mut res = vec![0xc0];
        for &i in unescaped {
            match i {
                0xc0 => res.extend_from_slice(&[0xdb, 0xdc]),
                0xdb => res.extend_from_slice(&[0xdb, 0xdd]),
                a => res.push(a),
            }
        }
        res.push(0xc0);
        res
    }

    fn unescape(unescaped: &[u8]) -> Result<Vec<u8>> {
        let mut res = vec![];

        let mut iter = unescaped.iter();
        while let Some(&byte) = iter.next() {
            res.push(match byte {
                0xdb => match iter.next() {
                    Some(0xdc) => 0xc0,
                    Some(0xdd) => 0xdb,
                    i => bail!("encountered invalid byte '{i:?}' after escape character"),
                },
                i => i,
            });
        }

        Ok(res)
    }

    pub fn send_data(&mut self, data: &[u8]) -> Result<()> {
        let (packet, seq_nr) = self.create_packet(data);

        // println!("send: {:?}", packet.iter().map(|i| format!("{:02x}", i).chars().collect::<Vec<_>>()).flatten().collect::<String>());

        self.port
            .write_all(&packet)
            .wrap_err("failed to write to serial port")?;
        sleep(Duration::from_millis(40));

        let res = self.wait_for_ack()
            .wrap_err("waiting for message acknowledgement. If this is due to a timeout, try resetting your board, or turning it off and on again")?;

        if res != (seq_nr + 1) % 8 {
            bail!("received invalid sequence number, retry transmission")
        }

        Ok(())
    }

    pub fn wait_for_ack(&mut self) -> Result<u8> {
        let (tx, rx) = channel();

        spawn(move || {
            if rx.recv_timeout(SERIAL_TIMEOUT).is_err() {
                println!("Your read operation seems to be timing out. Make sure you reset your board before uploading a program");
                println!("and try turning it off and on again. We'll keep trying to send data, but most likely the upload has failed now.");
            }
        });

        let mut response = Vec::new();

        while response.iter().filter(|&&i| i == 0xc0).count() < 2 {
            let mut temp = [0u8; 6];
            self.port
                .read_all(&mut temp)
                .wrap_err("failed to read from serial port")?;
            response.extend_from_slice(&temp);
        }

        // ignore error, if the thread died then that's too bad.
        let _ = tx.send(());

        let unescaped = Self::unescape(&response)?;

        // remove 0xc0 at the start and end
        let message = &unescaped[1..unescaped.len() - 1];

        Ok(message[0] >> 3 & 0x07)
    }

    pub fn send_start_dfu(&mut self, file_size: u32) -> Result<()> {
        let mut res = Vec::new();

        res.extend_from_slice(&Self::encode_int(DFU_START_PACKET));
        res.extend_from_slice(&Self::encode_int(4));
        res.extend_from_slice(&Self::encode_int(0));
        res.extend_from_slice(&Self::encode_int(0));
        res.extend_from_slice(&Self::encode_int(file_size));

        self.send_data(&res)?;

        Ok(())
    }

    pub fn send_init_packet(&mut self, file: &[u8]) -> Result<()> {
        let mut res = vec![];

        res.extend_from_slice(&Self::encode_int(DFU_INIT_PACKET));
        res.extend_from_slice(&[
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 0x00, 0xfe, 0xff,
        ]);
        res.extend_from_slice(&calc_crc16_default(file).to_le_bytes());
        // padding required as per the python reference implementation. No further docs found on this
        res.extend_from_slice(&[0, 0]);

        self.send_data(&res)?;

        Ok(())
    }

    pub fn send_stop_packet(&mut self) -> Result<()> {
        let mut res = vec![];

        res.extend_from_slice(&Self::encode_int(DFU_STOP_DATA_PACKET));
        self.send_data(&res)?;

        Ok(())
    }

    pub fn send_data_packet(&mut self, data: &[u8]) -> Result<()> {
        let mut res = vec![];

        res.extend_from_slice(&Self::encode_int(DFU_DATA_PACKET));
        res.extend_from_slice(data);

        self.send_data(&res)?;

        Ok(())
    }

    pub fn try_do_upload(&mut self, file: &[u8]) -> Result<()> {
        println!("starting connection...");
        self.send_start_dfu(file.len() as u32)?;
        // wait before we actually send data to the board after
        // we send the start_dfu message
        sleep(SEND_START_DFU_WAIT_TIME);

        println!("initializing upload...");
        self.send_init_packet(file)?;

        // wait before we actually send data to the board after
        // we send the init_packet message
        sleep(SEND_INIT_PACKET_WAIT_TIME);

        let total_chunks = (file.len() + DFU_MAX_PACKET_SIZE - 1) / DFU_MAX_PACKET_SIZE;

        println!(
            "uploading in {total_chunks} chunks ({}kb)...",
            file.len() as f64 / 1024.0
        );
        for (index, i) in file.chunks(DFU_MAX_PACKET_SIZE).enumerate() {
            if let Err(e) = self.send_data_packet(i) {
                println!();
                return Err(e);
            }
            print!(
                "\rframes uploaded: {}/{total_chunks} = {:.1}%",
                index + 1,
                ((index + 1) as f64 / total_chunks as f64) * 100.0
            );
            stdout().flush().unwrap();
        }
        println!();

        println!("finalizing upload...");
        self.send_stop_packet()?;

        println!("done");
        Ok(())
    }
}

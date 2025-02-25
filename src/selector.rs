use std::io::{stdin, stdout, Write};

use color_eyre::{eyre::eyre, Help, Result};
use crossterm::{
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use serial_enumerator::{get_serial_list, SerialInfo};

#[derive(Default)]
pub enum PortSelector<'a> {
    /// Automatically upload based on the USB Product ID and Vendor ID of the serial chip that is on
    /// the drone boards used in the Embedded Systems Lab
    #[default]
    AutoManufacturer,

    /// Upload to the first port that is found
    SearchFirst,

    /// Try all serial ports that can be found, and run after
    /// the first upload was successful.
    SearchAll,

    /// Interactively choose which serial port you want to upload to
    ChooseInteractive,

    /// Choose to a specific, named serial port
    /// Note that a conversion from strings exists for this
    /// variant, so you can just write `upload("/dev/ttyUSB0", ...)` for example.
    Named(&'a str),
}

impl<'a, T: AsRef<str>> From<&'a T> for PortSelector<'a> {
    fn from(s: &'a T) -> Self {
        Self::Named(s.as_ref())
    }
}

pub fn all_serial_ports() -> impl Iterator<Item = String> {
    get_serial_list()
        .into_iter()
        .filter(|i| i.usb_info.is_some())
        .map(|i| i.name)
}

pub fn choose_interactive() -> Result<String> {
    internal_choose_interactive(get_serial_list())
}

pub fn find_available_serial_port_by_id() -> Result<String> {
    let mut ports: Vec<_> = get_serial_list()
        .into_iter()
        .filter(|a| {
            if let Some(usb_info) = &a.usb_info {
                (usb_info.vid == "403" || usb_info.vid == "0403") && usb_info.pid == "6015"
            } else {
                false
            }
        })
        .collect();

    if ports.is_empty() {
        Err(eyre!("No serial port to choose from").suggestion("Make sure the usb is plugged in"))
    } else if ports.len() > 1 {
        internal_choose_interactive(ports)
    } else {
        ports
            .pop()
            .ok_or_else(|| eyre!("Error getting serial port"))
            .map(|info| info.name)
    }
}

fn internal_choose_interactive(mut ports: Vec<SerialInfo>) -> Result<String> {
    if ports.is_empty() {
        return Err(
            eyre!("No serial port to choose from").suggestion("Make sure the usb is plugged in")
        );
    }

    execute!(stdout(), EnterAlternateScreen, Clear(ClearType::All))?;
    let index = loop {
        println!("Please choose a Serial Device (by number):\n");
        for (index, port) in ports.iter().enumerate() {
            print!("\t{index}: {}", port.name);
            if let Some(product) = &port.product {
                print!(", {product}");
            }
            if let Some(usb_info) = &port.usb_info {
                print!(", pid: {}, vid: {}", usb_info.pid, usb_info.vid);
            }
            println!();
        }

        print!("\n >>> ");

        stdout().flush()?;
        let mut buf = String::new();
        stdin().read_line(&mut buf)?;

        if let Ok(i) = buf.trim().parse::<usize>() {
            if i < ports.len() {
                break i;
            }
            execute!(
                stdout(),
                Clear(ClearType::All),
                SetForegroundColor(Color::Red),
                Print("Index out of range".to_owned()),
                ResetColor
            )?;
        } else {
            execute!(
                stdout(),
                Clear(ClearType::All),
                SetForegroundColor(Color::Red),
                Print("Please enter a valid number".to_owned()),
                ResetColor
            )?;
        }

        println!();
    };

    execute!(stdout(), LeaveAlternateScreen)?;
    // swap_remove is safe because we checked i < ports.len() earlier
    // and i != 0 at the start of this function
    Ok(ports.swap_remove(index).name)
}

#[cfg(test)]
mod tests {
    use crate::selector::choose_interactive;

    use super::{find_available_serial_port_by_id, internal_choose_interactive};

    #[test]
    fn test_no_ports() {
        assert!(internal_choose_interactive(Vec::new()).is_err());
    }

    #[test]
    #[ignore]
    fn test_find_serial_port_by_manufacturer() {
        assert_eq!(find_available_serial_port_by_id().unwrap(), "/dev/ttyUSB0");
    }

    #[test]
    #[ignore]
    fn test_choose_interactive() {
        // To run this test, please do:
        // cargo test --package tudelft-serial-upload --lib -- selector::tests::test_choose_interactive --exact --nocapture --ignored
        assert_eq!(choose_interactive().unwrap(), "/dev/ttyUSB0");
    }
}

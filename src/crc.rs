/// This implements the CRC like the original python implementation.
/// It's hard to say which specific CRC it is, otherwise I'd have used a library.
/// ChatGPT says it's CCITT, but there are two variants and none look like this one.
pub fn calc_crc16(data: &[u8], start: Option<u16>) -> u16 {
    let mut crc = start.unwrap_or(0xffff);
    for &b in data {
        crc = (crc >> 8 & 0x00FF) | (crc << 8 & 0xFF00);
        crc ^= b as u16;
        crc ^= (crc & 0x00FF) >> 4;
        crc ^= (crc << 8) << 4;
        crc ^= ((crc & 0x00FF) << 4) << 1;
    }

    crc
}

pub fn calc_crc16_default(data: &[u8]) -> u16 {
    calc_crc16(data, None)
}

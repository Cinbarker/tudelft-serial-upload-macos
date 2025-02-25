extern crate core;

mod crc;
mod selector;
mod serial;
mod upload;

use std::time::Duration;

pub use color_eyre;
pub use selector::PortSelector;
pub use upload::{upload, upload_file, upload_file_or_stop, upload_or_stop};
pub use serial2;

const SERIAL_TIMEOUT: Duration = Duration::from_secs(5);

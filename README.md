# TU Delft Serial Upload

Serial upload library for the quadrupel drone project
of the Embedded Systems Lab at the TU Delft (`CESE4030`)

## Mac Usage

Since the default drivers for the FT23x serial bridge in macOS don't support hardware flow control (RTS/CTS), you need to follow do the following:

1. Install the FTDI d2xx drivers from the FTDI website: https://ftdichip.com/drivers/d2xx-drivers/. Follow the instructions in the driver's README file to install.
2. Use `tudelft-serial-upload = { git = "https://github.com/cinbarker/tudelft-serial-upload-macos.git" }` in the Cargo.toml of the runner to specify using this patched version.
3. Use `PortSelector::AutoManufacturer` and select the tty port when prompted.

**NOTE:** The other change you must make to get the project working on macOS is to specify a target of `aarch64-apple-darwin` instead of `x86_64-unknown-linux-gnu` in the `.cargo/config.toml`. 

# Changes

- Use `libftd2xx` instead of `serial2` in serial.rs and Cargo.toml 
- Fix PortSelector::AutoManufacturer in selector.rs to match a VID of "403" and not just "0403"

# Disclaimer

This code was uploaded with the permission of the CESE4030 course staff for use by other students facing the same issue. This patch was developed and tested on a M3 Pro Macbook Pro running macOS 14.6.1 Sonoma.

use crate::serial::Serial;
use crate::{selector, PortSelector};
use color_eyre::eyre::{bail, eyre, Context};
use color_eyre::Result;
use std::fs::read;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

fn copy_object(source: &Path, target: &Path) -> Result<()> {
    if Command::new("rust-objcopy").output().is_err() {
        bail!(
            "rust-objcopy not found, try installing cargo-binutils or refer to the course website"
        );
    }

    let op = Command::new("rust-objcopy")
        .arg("-O")
        .arg("binary")
        .arg(source)
        .arg(target)
        .output()
        .wrap_err("failed to run rust-objcopy")?;

    println!("creating binary file at {target:?}");

    if !op.status.success() {
        bail!(
            "running rust-objcopy failed: {}",
            String::from_utf8_lossy(&op.stderr)
        );
    }

    Ok(())
}

fn read_file(file: &Path) -> Result<Vec<u8>> {
    let mut target = file.to_path_buf();
    target.set_extension("bin");

    println!("converting elf file to bin file");
    copy_object(file, &target)?;

    println!("reading binary file");
    read(target).wrap_err("failed to read converted binary file to send to board")
}

/// Upload a file to a connected board. Select which serial port the board is on with the [`PortSelector`].
/// The file is expected to be the compiled `.elf` file created by cargo/rustc
/// Exit with an exit code of 1 when the upload fails.
///
/// Returns a path to a serial port over which uploading happened. This path can be used to communicate with the board.
pub fn upload_file_or_stop(port: PortSelector, file: Option<impl AsRef<Path>>) -> PathBuf {
    if let Some(file) = file {
        match read_file(file.as_ref())
            .wrap_err_with(|| format!("failed to read from file {:?}", file.as_ref()))
        {
            Ok(i) => upload_or_stop(port, i, false),
            Err(e) => {
                eprintln!("{e:?}");
                exit(1);
            }
        }
    } else {
        upload_or_stop(port, [], true)
    }
}

/// Upload a file to a connected board. Select which serial port the board is on with the [`PortSelector`]
/// The file is expected to be the compiled `.elf` file created by cargo/rustc
/// Returns an error when the upload fails.
///
/// Returns a path to a serial port over which uploading happened. This path can be used to communicate with the board.
pub fn upload_file(port: PortSelector, file: Option<impl AsRef<Path>>) -> Result<PathBuf> {
    upload(
        port,
        file.as_ref()
            .map(|f| {
                read_file(f.as_ref())
                    .wrap_err_with(|| format!("failed to read from file {:?}", f.as_ref()))
            })
            .transpose()?
            .unwrap_or_default(),
        file.is_none(),
    )
}

/// Upload (already read) bytes to a connected board. Select which serial port the board is on with the [`PortSelector`]
/// The bytes are the exact bytes that are uploaded to the board. That means it should be a binary file, and *not* contain
/// ELF headers or similar
/// Exit with an exit code of 1 when the upload fails.
///
/// Returns a path to a serial port over which uploading happened. This path can be used to communicate with the board.
pub fn upload_or_stop(port: PortSelector, file: impl AsRef<[u8]>, dry_run: bool) -> PathBuf {
    match upload(port, file.as_ref(), dry_run) {
        Err(e) => {
            eprintln!("{e:?}");
            exit(1);
        }
        Ok(i) => i,
    }
}

/// Upload (already read) bytes to a connected board. Select which serial port the board is on with the [`PortSelector`]
/// The bytes are the exact bytes that are uploaded to the board. That means it should be a binary file, and *not* contain
/// ELF headers or similar
/// Returns an error when the upload fails.
///
/// Returns a path to a serial port over which uploading happened. This path can be used to communicate with the board.
pub fn upload(port: PortSelector, file: impl AsRef<[u8]>, dry_run: bool) -> Result<PathBuf> {
    upload_internal(port, file.as_ref(), dry_run)
}

fn upload_internal(port: PortSelector<'_>, file: &[u8], dry_run: bool) -> Result<PathBuf> {
    if dry_run && matches!(port, PortSelector::SearchAll) {
        bail!("can't use dry_run in SearchAll mode");
    }

    let (ports_to_try, stop_after_first_error): (Vec<Result<Serial>>, bool) = match port {
        PortSelector::SearchFirst => (
            selector::all_serial_ports()
                .map(PathBuf::from)
                .map(Serial::open)
                .collect(),
            true,
        ),
        PortSelector::SearchAll => (
            selector::all_serial_ports()
                .map(PathBuf::from)
                .map(Serial::open)
                .collect(),
            false,
        ),
        PortSelector::ChooseInteractive => (
            vec![Serial::open(PathBuf::from(selector::choose_interactive()?))],
            true,
        ),
        PortSelector::Named(n) => (vec![Serial::open(Path::new(n).to_path_buf())], false),
        PortSelector::AutoManufacturer => (
            vec![Serial::open(PathBuf::from(
                selector::find_available_serial_port_by_id()?,
            ))],
            true,
        ),
    };

    let mut errors = Vec::new();
    let num_ports = ports_to_try.len();

    for i in ports_to_try {
        let mut port = match i {
            Ok(i) => i,
            Err(e) => {
                if stop_after_first_error || num_ports == 1 {
                    return Err(e);
                }
                eprintln!("WARNING: {e}");
                errors.push(e);
                continue;
            }
        };

        if dry_run {
            return Ok(port.path);
        }

        if let Err(e) = port
            .try_do_upload(file)
            .wrap_err_with(|| format!("failed to upload to port {:?}", port.path))
        {
            if stop_after_first_error || num_ports == 1 {
                return Err(e);
            }
            eprintln!("WARNING: {e}");
            errors.push(e);
            continue;
        }
        return Ok(port.path);
    }

    Err(eyre!(
        "uploading failed because none of the ports tried worked (see previous warnings)"
    ))
}

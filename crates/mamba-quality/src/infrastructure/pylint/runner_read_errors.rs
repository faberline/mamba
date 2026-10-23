//! A confirmed permission failure preserves the original source-load message.
use std::{fs, io::ErrorKind, path::Path};

pub(super) fn message(path: &Path) -> Option<String> {
    let error = fs::File::open(path).err()?;
    if error.kind() != ErrorKind::PermissionDenied {
        return None;
    }
    let errno = error.raw_os_error()?;
    let name = path.to_string_lossy();
    let quoted = super::string_format::python_string_repr(&name);
    Some(format!(
        "error while code parsing: Unable to load file {name}:\n[Errno {errno}] Permission denied: {quoted}"
    ))
}

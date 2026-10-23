//! Native effective-ID checks. This does not create a project-side probe.
use super::Creation;
use std::{io, path::Path};

// Darwin's public extended access modes select distinct directory ACL rights.
// sys/unistd.h defines these in the vnode-rights namespace shifted left eight.
#[cfg(target_os = "macos")]
pub(super) fn check(path: &Path, creation: Creation) -> io::Result<()> {
    use rustix::fs::{accessat, Access, AtFlags, CWD};
    use std::os::raw::c_int;

    const DARWIN_WRITE_OK: c_int = 1 << 10;
    const DARWIN_APPEND_OK: c_int = 1 << 13;
    const DARWIN_EXECUTE_OK: c_int = 1 << 11;
    let add = match creation {
        Creation::File => DARWIN_WRITE_OK,
        Creation::Directory => DARWIN_APPEND_OK,
    };
    accessat(
        CWD,
        path,
        Access::from_bits_retain(add | DARWIN_EXECUTE_OK),
        AtFlags::EACCESS,
    )
    .map_err(io::Error::from)
}

#[cfg(target_os = "linux")]
pub(super) fn check(path: &Path, _creation: Creation) -> io::Result<()> {
    use rustix::fs::{accessat, Access, AtFlags, CWD};
    accessat(
        CWD,
        path,
        Access::WRITE_OK | Access::EXEC_OK,
        AtFlags::EACCESS,
    )
    .map_err(io::Error::from)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn check(_path: &Path, _creation: Creation) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "No effective-access check is defined for this platform",
    ))
}

#[cfg(test)]
mod tests;

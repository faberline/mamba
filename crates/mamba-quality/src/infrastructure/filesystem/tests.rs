use super::*;
use crate::domain::ports::ProjectStore;

#[test]
fn refuses_a_source_changed_after_the_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.py");
    fs::write(&path, b"value = 1\n").unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let source = &workspace.sources[0];
    fs::write(&path, b"value = 2\n").unwrap();
    let error = Filesystem.publish(source, b"value = 3\n").unwrap_err();
    assert_eq!(error.issue.code, "source-changed");
    assert_eq!(fs::read(path).unwrap(), b"value = 2\n");
}

#[test]
fn publishes_atomically_and_keeps_the_original_encoding_and_mode() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.py");
    fs::write(&path, b"# coding: latin-1\r\nlabel='caf\xe9'\r\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
    }
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    assert!(workspace.issues.is_empty(), "{:?}", workspace.issues);
    let source = &workspace.sources[0];
    let bytes = Filesystem
        .candidate_bytes(source, "# coding: latin-1\r\nlabel = \"café\"\r\n")
        .unwrap();
    Filesystem.publish(source, &bytes).unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"# coding: latin-1\r\nlabel = \"caf\xe9\"\r\n"
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o751
        );
    }
}

#[test]
fn concurrent_writer_keeps_source_and_releases_its_lock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.py");
    fs::write(&path, b"VALUE=1\n").unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let source = &workspace.sources[0];
    let writer = fs::File::open(&path).unwrap();
    writer.try_lock_exclusive().unwrap();
    let error = Filesystem.publish(source, b"VALUE = 1\n").unwrap_err();
    assert_eq!(error.issue.code, "source-busy");
    assert_eq!(fs::read(&path).unwrap(), b"VALUE=1\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    drop(writer);
    Filesystem.publish(source, b"VALUE = 1\n").unwrap();
    assert_eq!(fs::read(path).unwrap(), b"VALUE = 1\n");
}

#[test]
fn replacement_after_snapshot_keeps_the_external_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.py");
    let replacement = directory.path().join("replacement.py");
    fs::write(&path, b"VALUE=1\n").unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    fs::write(&replacement, b"VALUE=2\n").unwrap();
    fs::rename(replacement, &path).unwrap();
    let error = Filesystem
        .publish(&workspace.sources[0], b"VALUE = 1\n")
        .unwrap_err();
    assert_eq!(error.issue.code, "source-changed");
    assert_eq!(fs::read(path).unwrap(), b"VALUE=2\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn bom_and_crlf_survive_publication_and_unencodable_candidate_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.py");
    fs::write(&path, b"\xef\xbb\xbfVALUE=1\r\n").unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let source = &workspace.sources[0];
    let bytes = Filesystem.candidate_bytes(source, "VALUE = 1\r\n").unwrap();
    Filesystem.publish(source, &bytes).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"\xef\xbb\xbfVALUE = 1\r\n");
    let original = b"# coding: ascii\nVALUE='one'\n";
    fs::write(&path, original).unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let error = Filesystem
        .candidate_bytes(&workspace.sources[0], "# coding: ascii\nVALUE = 'café'\n")
        .unwrap_err();
    assert_eq!(error.code, "source-encoding");
    assert_eq!(fs::read(path).unwrap(), original);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn read_only_source_is_never_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.py");
    fs::write(&path, b"VALUE=1\n").unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    let outcome = Filesystem.publish(&workspace.sources[0], b"VALUE = 1\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(outcome.unwrap_err().issue.code, "source-readonly");
    assert_eq!(fs::read(path).unwrap(), b"VALUE=1\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[cfg(unix)]
mod publication;

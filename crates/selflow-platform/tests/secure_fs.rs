use std::fs;
use std::os::unix::fs::PermissionsExt;

use selflow_platform::secure_fs::{private_dir, private_file};

fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn a_private_dir_is_created_and_an_existing_one_tightened() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("a/b");
    private_dir(&nested).unwrap();
    assert_eq!(mode(&nested), 0o700);

    let open = root.path().join("open");
    fs::create_dir(&open).unwrap();
    fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
    private_dir(&open).unwrap();
    assert_eq!(mode(&open), 0o700);
}

#[test]
fn a_private_file_keeps_its_contents_and_gets_mode_0600() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("f");
    fs::write(&file, "x").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    private_file(&file).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "x");
    assert_eq!(mode(&file), 0o600);
}

use selflow_client::client::ClientError;
use selflow_ui::lock::lock_exit_code;

#[test]
fn only_a_version_mismatch_ends_the_lock_client() {
    let mismatch = ClientError::VersionMismatch {
        running: Some(999),
        installed: Some(999),
    };
    assert_eq!(lock_exit_code(&mismatch), Some(3));
    assert_eq!(lock_exit_code(&ClientError::Unreachable), None);
}

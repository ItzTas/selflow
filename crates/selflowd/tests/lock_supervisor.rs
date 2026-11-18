mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use selflowd::enforcer::{LockSupervisor, PowerMode, lock_program_from, power_mode_from};
use support::{sample_view, wait_for};

#[tokio::test]
async fn restarts_the_lock_client_when_it_dies() {
    let mut supervisor = LockSupervisor::new("sleep".into(), vec!["30".into()]);
    supervisor.show(sample_view());
    let first = supervisor.pid().expect("the client started");
    std::process::Command::new("kill")
        .arg(first.to_string())
        .status()
        .unwrap();
    assert!(
        wait_for(Duration::from_secs(2), || supervisor
            .pid()
            .is_some_and(|pid| pid != first))
        .await
    );
    supervisor.hide();
}

#[tokio::test]
async fn hiding_publishes_the_end_and_stops_the_client() {
    let mut supervisor = LockSupervisor::new("sleep".into(), vec!["30".into()]);
    let views = supervisor.views();
    supervisor.show(sample_view());
    supervisor.hide();
    assert_eq!(*views.borrow(), None);
    assert!(wait_for(Duration::from_secs(7), || supervisor.pid().is_none()).await);
}

#[test]
fn finds_the_lock_program_next_to_the_daemon() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("selflow"), "").unwrap();
    assert_eq!(
        lock_program_from(&dir.path().join("selflowd")),
        dir.path().join("selflow")
    );
    assert_eq!(
        lock_program_from(Path::new("/nowhere/selflowd")),
        PathBuf::from("selflow")
    );
}

#[test]
fn log_mode_never_powers_off() {
    assert_eq!(power_mode_from(Some("log".as_ref())), PowerMode::LogOnly);
    assert_eq!(power_mode_from(None), PowerMode::Real);
}

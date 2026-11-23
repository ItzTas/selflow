mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use selflowd::enforcer::{
    LockSupervisor, PowerMode, ScreenControl, lock_program_from, power_mode_from,
};
use support::{sample_view, wait_for};

const STAYS: &str = "echo started >> \"$0\"; exec sleep 30";

fn counting(starts: &Path, script: &str) -> LockSupervisor {
    LockSupervisor::new(
        "sh".into(),
        vec!["-c".into(), script.into(), starts.display().to_string()],
    )
}

fn started(starts: &Path) -> usize {
    std::fs::read_to_string(starts).map_or(0, |text| text.lines().count())
}

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
async fn backs_off_when_the_client_keeps_exiting_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    let mut supervisor = LockSupervisor::new(
        "sh".into(),
        vec![
            "-c".into(),
            "echo started >> \"$0\"".into(),
            starts.display().to_string(),
        ],
    );
    supervisor.show(sample_view());
    tokio::time::sleep(Duration::from_millis(4500)).await;
    let count = std::fs::read_to_string(&starts).unwrap().lines().count();
    supervisor.hide();
    assert!((2..=3).contains(&count), "started {count} times");
}

#[tokio::test]
async fn a_client_that_ran_for_a_while_comes_back_at_once_and_resets_the_backoff() {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    let mut supervisor = LockSupervisor::new(
        "sh".into(),
        vec![
            "-c".into(),
            "echo started >> \"$0\"; case $(wc -l < \"$0\") in 3 | 5) exec sleep 30 ;; esac".into(),
            starts.display().to_string(),
        ],
    );
    supervisor.show(sample_view());
    let started = || std::fs::read_to_string(&starts).map_or(0, |text| text.lines().count());
    assert!(wait_for(Duration::from_secs(5), || started() == 3).await);
    tokio::time::sleep(Duration::from_millis(5500)).await;
    let steady = supervisor.pid().expect("the third client is running");
    std::process::Command::new("kill")
        .arg(steady.to_string())
        .status()
        .unwrap();
    assert!(
        wait_for(Duration::from_secs(3), || started() == 5
            && supervisor.pid().is_some())
        .await
    );
    supervisor.hide();
}

#[tokio::test]
async fn retries_a_client_that_could_not_start() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("sleep");
    let mut supervisor = LockSupervisor::new(program.clone(), vec!["30".into()]);
    supervisor.show(sample_view());
    assert_eq!(supervisor.pid(), None);
    std::os::unix::fs::symlink(sleep_binary(), &program).unwrap();
    assert!(wait_for(Duration::from_secs(3), || supervisor.pid().is_some()).await);
    supervisor.hide();
}

#[tokio::test]
async fn show_waits_out_the_backoff() {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    let mut supervisor = counting(&starts, "echo started >> \"$0\"");
    supervisor.show(sample_view());
    assert!(
        wait_for(Duration::from_secs(3), || started(&starts) == 2
            && supervisor.pid().is_none())
        .await
    );
    for _ in 0..10 {
        supervisor.show(sample_view());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(started(&starts), 2);
    supervisor.hide();
}

#[tokio::test]
async fn quick_exits_reset_after_a_steady_client_is_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    let mut supervisor = counting(
        &starts,
        "echo started >> \"$0\"; case $(wc -l < \"$0\") in 3) exec sleep 30 ;; esac",
    );
    supervisor.show(sample_view());
    assert!(wait_for(Duration::from_secs(5), || started(&starts) == 3).await);
    tokio::time::sleep(Duration::from_millis(5500)).await;
    supervisor.hide();
    supervisor.release();
    assert!(
        wait_for(Duration::from_secs(2), || started(&starts) == 4
            && supervisor.pid().is_none())
        .await
    );
    supervisor.show(sample_view());
    assert!(wait_for(Duration::from_millis(2500), || started(&starts) == 6).await);
    supervisor.hide();
}

#[tokio::test]
async fn hiding_during_the_backoff_starts_one_release_client() {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    let mut supervisor = counting(
        &starts,
        "echo started >> \"$0\"; case $(wc -l < \"$0\") in 1 | 2) exit ;; esac; exec sleep 30",
    );
    supervisor.show(sample_view());
    assert!(
        wait_for(Duration::from_secs(3), || started(&starts) == 2
            && supervisor.pid().is_none())
        .await
    );
    supervisor.hide();
    assert!(supervisor.pid().is_some());
    tokio::time::sleep(Duration::from_secs(7)).await;
    assert_eq!(started(&starts), 3);
    assert_eq!(supervisor.pid(), None);
    supervisor.hide();
    assert_eq!(supervisor.pid(), None);
}

#[tokio::test]
async fn a_release_starts_one_client_that_is_not_restarted() {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    let supervisor = counting(&starts, STAYS);
    supervisor.release();
    assert!(wait_for(Duration::from_secs(2), || started(&starts) == 1).await);
    let client = supervisor.pid().expect("the release client is running");
    std::process::Command::new("kill")
        .arg(client.to_string())
        .status()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(started(&starts), 1);
    assert_eq!(supervisor.pid(), None);
}

#[tokio::test]
async fn a_client_killed_after_the_lock_ends_is_followed_by_one_release_client() {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    let mut supervisor = counting(&starts, STAYS);
    let views = supervisor.views();
    supervisor.show(sample_view());
    supervisor.hide();
    assert_eq!(*views.borrow(), None);
    tokio::time::sleep(Duration::from_secs(11)).await;
    assert_eq!(started(&starts), 2);
    assert_eq!(supervisor.pid(), None);
}

#[test]
fn finds_the_lock_program_next_to_the_daemon() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("selflow-ui"), "").unwrap();
    assert_eq!(
        lock_program_from(&dir.path().join("selflowd")),
        dir.path().join("selflow-ui")
    );
    assert_eq!(
        lock_program_from(Path::new("/nowhere/selflowd")),
        PathBuf::from("selflow-ui")
    );
}

#[test]
fn log_mode_never_powers_off() {
    assert_eq!(power_mode_from(Some("log".as_ref())), PowerMode::LogOnly);
    assert_eq!(power_mode_from(None), PowerMode::Real);
}

fn sleep_binary() -> PathBuf {
    let output = std::process::Command::new("sh")
        .args(["-c", "command -v sleep"])
        .output()
        .unwrap();
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}

#[test]
fn power_modes_print_as_their_setting() {
    assert_eq!(PowerMode::LogOnly.to_string(), "log");
    assert_eq!(PowerMode::Real.to_string(), "real");
}

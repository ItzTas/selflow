use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CLAUDE_VERSION_DEADLINE: Duration = Duration::from_secs(3);
const EXIT_POLL: Duration = Duration::from_millis(10);

pub fn run_editor(path: &Path) -> io::Result<ExitStatus> {
    Command::new("sh")
        .args(["-c", r#"${VISUAL:-${EDITOR:-vi}} "$1""#, "sh"])
        .arg(path)
        .status()
}

pub fn claude_settings_path_from(
    config_dir: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Option<PathBuf> {
    let base = match config_dir.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(home.filter(|home| !home.is_empty())?).join(".claude"),
    };
    Some(base.join("settings.json"))
}

pub fn claude_settings_path() -> Option<PathBuf> {
    claude_settings_path_from(
        std::env::var_os("CLAUDE_CONFIG_DIR").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

pub fn claude_version_output() -> Option<String> {
    version_output_within(OsStr::new("claude"), CLAUDE_VERSION_DEADLINE)
}

pub fn version_output_within(program: &OsStr, limit: Duration) -> Option<String> {
    let mut child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(EXIT_POLL),
            _ => return stop(&mut child),
        }
    };
    if !status.success() {
        return None;
    }
    let mut output = String::new();
    child.stdout.take()?.read_to_string(&mut output).ok()?;
    Some(output)
}

fn stop(child: &mut Child) -> Option<String> {
    let _ = child.kill();
    let _ = child.wait();
    None
}

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

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
    let output = Command::new("claude")
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

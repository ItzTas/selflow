use std::io;
use std::path::Path;
use std::process::{Command, ExitStatus};

pub fn run_editor(path: &Path) -> io::Result<ExitStatus> {
    Command::new("sh")
        .args(["-c", r#"${VISUAL:-${EDITOR:-vi}} "$1""#, "sh"])
        .arg(path)
        .status()
}

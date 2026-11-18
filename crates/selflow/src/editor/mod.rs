use std::ffi::OsStr;
use std::fs::DirBuilder;
use std::io::{self, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::process::Command;

use selflow_protocol::ipc::{PositionedMessage, Response};

pub trait Editor {
    fn edit(&mut self, text: &str) -> io::Result<String>;
}

pub struct TempfileEditor {
    dir: Option<PathBuf>,
}

impl TempfileEditor {
    pub fn new() -> TempfileEditor {
        TempfileEditor::from_runtime_dir(std::env::var_os("XDG_RUNTIME_DIR").as_deref())
    }

    pub fn from_runtime_dir(runtime_dir: Option<&OsStr>) -> TempfileEditor {
        TempfileEditor {
            dir: runtime_dir.map(|dir| PathBuf::from(dir).join("selflow")),
        }
    }
}

impl Default for TempfileEditor {
    fn default() -> TempfileEditor {
        TempfileEditor::new()
    }
}

impl Editor for TempfileEditor {
    fn edit(&mut self, text: &str) -> io::Result<String> {
        let mut builder = tempfile::Builder::new();
        builder.suffix(".toml");
        let mut file = match &self.dir {
            Some(dir) => {
                DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
                builder.tempfile_in(dir)?
            }
            None => builder.tempfile()?,
        };
        file.write_all(text.as_bytes())?;
        file.flush()?;
        let status = Command::new("sh")
            .args(["-c", r#"${VISUAL:-${EDITOR:-vi}} "$1""#, "sh"])
            .arg(file.path())
            .status()?;
        if !status.success() {
            return Err(io::Error::other("the editor exited with an error"));
        }
        std::fs::read_to_string(file.path())
    }
}

#[derive(Debug)]
pub enum EditOutcome {
    Unchanged,
    Saved(Response),
    Abandoned,
    Failed(io::Error),
}

pub fn edit_loop(
    initial: &str,
    editor: &mut dyn Editor,
    save: &mut dyn FnMut(&str) -> Response,
    ask_again: &mut dyn FnMut(&[PositionedMessage]) -> bool,
) -> EditOutcome {
    let mut current = initial.to_owned();
    loop {
        let edited = match editor.edit(&current) {
            Ok(edited) => edited,
            Err(error) => return EditOutcome::Failed(error),
        };
        if edited == current {
            return match current == initial {
                true => EditOutcome::Unchanged,
                false => EditOutcome::Abandoned,
            };
        }
        let response = save(&edited);
        let Response::Invalid { errors } = response else {
            return EditOutcome::Saved(response);
        };
        current = edited;
        if !ask_again(&errors) {
            return EditOutcome::Abandoned;
        }
    }
}

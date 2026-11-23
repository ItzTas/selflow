use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::os::run_editor;
use selflow_platform::paths::{runtime_dir, runtime_dir_from};
use selflow_platform::secure_fs::private_dir;
use selflow_protocol::ipc::{PositionedMessage, Response};

pub trait Editor {
    fn edit(&mut self, text: &str) -> io::Result<String>;
}

pub struct TempfileEditor {
    dir: Option<PathBuf>,
}

impl TempfileEditor {
    pub fn new() -> TempfileEditor {
        TempfileEditor { dir: runtime_dir() }
    }

    pub fn from_runtime_dir(runtime_dir: Option<&OsStr>) -> TempfileEditor {
        TempfileEditor {
            dir: runtime_dir_from(runtime_dir),
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
                private_dir(dir)?;
                builder.tempfile_in(dir)?
            }
            None => builder.tempfile()?,
        };
        file.write_all(text.as_bytes())?;
        file.flush()?;
        let status = run_editor(file.path())?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstSave {
    Pending,
    Done,
}

pub fn edit_loop(
    initial: &str,
    first_save: FirstSave,
    editor: &mut dyn Editor,
    save: &mut dyn FnMut(&str) -> Response,
    ask_again: &mut dyn FnMut(&[PositionedMessage]) -> bool,
) -> EditOutcome {
    let mut current = initial.to_owned();
    let mut rejected = false;
    loop {
        let edited = match editor.edit(&current) {
            Ok(edited) => edited,
            Err(error) => return EditOutcome::Failed(error),
        };
        let pending = first_save == FirstSave::Pending && !rejected;
        if edited == current && !pending {
            return match rejected {
                true => EditOutcome::Abandoned,
                false => EditOutcome::Unchanged,
            };
        }
        let response = save(&edited);
        let Response::Invalid { errors } = response else {
            return EditOutcome::Saved(response);
        };
        current = edited;
        rejected = true;
        if !ask_again(&errors) {
            return EditOutcome::Abandoned;
        }
    }
}

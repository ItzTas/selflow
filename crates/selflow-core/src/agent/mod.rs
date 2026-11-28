mod constants;
mod types;

use std::path::{Path, PathBuf};

use crate::config::AgentStep;

use constants::{DENIED_ARGUMENT_PREFIXES, HOME_FOLDER, QUOTING, SHELL_OPERATORS, WORD_SEPARATORS};

pub use types::ActiveAgentStep;

pub fn covers(step: &AgentStep, cwd: &Path, home: Option<&Path>) -> bool {
    if step.folders.is_empty() {
        return true;
    }
    step.folders
        .iter()
        .filter_map(|folder| expanded(folder, home))
        .any(|folder| cwd.starts_with(folder))
}

fn expanded(folder: &str, home: Option<&Path>) -> Option<PathBuf> {
    let path = Path::new(folder);
    let Ok(rest) = path.strip_prefix(HOME_FOLDER) else {
        return Some(path.to_path_buf());
    };
    home.map(|home| home.join(rest))
}

pub fn governing<'s>(
    steps: &'s [ActiveAgentStep],
    cwd: &Path,
    home: Option<&Path>,
    attended: bool,
) -> Option<&'s ActiveAgentStep> {
    steps
        .iter()
        .filter(|active| attended || active.step.unattended)
        .filter(|active| covers(&active.step, cwd, home))
        .max_by_key(|active| (active.step.level, active.at))
}

pub fn command_allowed(command: &str, allowed: &[String]) -> bool {
    if SHELL_OPERATORS
        .iter()
        .any(|operator| command.contains(operator))
    {
        return false;
    }
    let given = words(command);
    if given.iter().any(|word| denied_argument(word)) {
        return false;
    }
    allowed.iter().any(|entry| {
        let entry = words(entry);
        !entry.is_empty() && given.starts_with(&entry)
    })
}

fn denied_argument(word: &str) -> bool {
    let unquoted: String = word.chars().filter(|c| !QUOTING.contains(c)).collect();
    DENIED_ARGUMENT_PREFIXES
        .iter()
        .any(|prefix| unquoted.starts_with(prefix))
}

fn words(command: &str) -> Vec<&str> {
    command
        .split(WORD_SEPARATORS)
        .filter(|word| !word.is_empty())
        .collect()
}

pub fn folders_cover(outer: &[String], inner: &[String]) -> bool {
    if outer.is_empty() {
        return true;
    }
    !inner.is_empty()
        && inner.iter().all(|folder| {
            outer
                .iter()
                .any(|cover| Path::new(folder).starts_with(cover))
        })
}

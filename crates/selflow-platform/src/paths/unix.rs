use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub fn store_path_from(state_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    let base = match state_home.filter(|value| !value.is_empty()) {
        Some(state) => PathBuf::from(state),
        None => Path::new(home?).join(".local/state"),
    };
    Some(base.join("selflow/selflow.db"))
}

pub fn store_path() -> Option<PathBuf> {
    store_path_from(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

pub fn runtime_dir_from(runtime_dir: Option<&OsStr>) -> Option<PathBuf> {
    runtime_dir.map(|dir| Path::new(dir).join("selflow"))
}

pub fn runtime_dir() -> Option<PathBuf> {
    runtime_dir_from(std::env::var_os("XDG_RUNTIME_DIR").as_deref())
}

pub fn socket_path_from(runtime_dir: Option<&OsStr>) -> Option<PathBuf> {
    runtime_dir_from(runtime_dir).map(|dir| dir.join("daemon.sock"))
}

pub fn socket_path() -> Option<PathBuf> {
    socket_path_from(std::env::var_os("XDG_RUNTIME_DIR").as_deref())
}

pub fn home_dir_from(home: Option<&OsStr>) -> Option<PathBuf> {
    home.filter(|value| !value.is_empty()).map(PathBuf::from)
}

pub fn home_dir() -> Option<PathBuf> {
    home_dir_from(std::env::var_os("HOME").as_deref())
}

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{
    home_dir, home_dir_from, runtime_dir, runtime_dir_from, socket_path, socket_path_from,
    store_path, store_path_from,
};

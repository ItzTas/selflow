#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{private_dir, private_file};

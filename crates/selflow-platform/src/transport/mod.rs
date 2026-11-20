mod types;
#[cfg(unix)]
mod unix;

pub use types::ListenError;
#[cfg(unix)]
pub use unix::{Listener, Stream, connect, listen};

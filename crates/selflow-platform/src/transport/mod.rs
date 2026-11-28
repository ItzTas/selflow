mod types;
#[cfg(unix)]
mod unix;

pub use types::{AcceptError, ListenError};
#[cfg(unix)]
pub use unix::{Listener, Stream, check_peer, connect, listen};

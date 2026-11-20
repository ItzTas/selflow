use std::io;
use std::process::{Command, Stdio};

use tokio::signal::unix::{Signal, SignalKind, signal};

pub fn power_off() -> io::Result<()> {
    Command::new("systemctl")
        .arg("poweroff")
        .stdin(Stdio::null())
        .spawn()
        .map(drop)
}

pub struct StopSignal {
    inner: Signal,
}

impl StopSignal {
    pub fn new() -> io::Result<StopSignal> {
        Ok(StopSignal {
            inner: signal(SignalKind::terminate())?,
        })
    }

    pub async fn recv(&mut self) {
        self.inner.recv().await;
    }
}

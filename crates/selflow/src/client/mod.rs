use std::path::Path;

use selflow_platform::transport::{Stream, connect};
use selflow_protocol::ipc::{Request, Response, read_message, write_message};
use tokio::io::BufReader;
use tokio::runtime::{Builder, Runtime};

use crate::cli::CliError;

pub struct Client {
    runtime: Runtime,
    stream: BufReader<Stream>,
}

impl Client {
    pub fn connect(path: &Path) -> Result<Client, CliError> {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(CliError::Protocol)?;
        let stream = runtime
            .block_on(connect(path))
            .map_err(|_| CliError::DaemonUnreachable)?;
        Ok(Client {
            runtime,
            stream: BufReader::new(stream),
        })
    }

    pub fn request(&mut self, request: Request) -> Result<Response, CliError> {
        let stream = &mut self.stream;
        self.runtime.block_on(async {
            write_message(stream.get_mut(), &request)
                .await
                .map_err(CliError::Protocol)?;
            read_message(stream)
                .await
                .map_err(CliError::Protocol)?
                .ok_or(CliError::DaemonUnreachable)
        })
    }
}

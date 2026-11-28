use std::io::IsTerminal;
use std::process::ExitCode;

use selflow_protocol::ipc::PROTOCOL_VERSION;
use selflowd::app::{self, AppError};
use selflowd::args::asks_for_protocol_version;
use tokio::runtime::Builder;

fn main() -> ExitCode {
    if asks_for_protocol_version(std::env::args().skip(1)) {
        println!("{PROTOCOL_VERSION}");
        return ExitCode::SUCCESS;
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_max_level(tracing::Level::INFO)
        .init();
    let outcome = Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(AppError::from)
        .and_then(|runtime| runtime.block_on(app::run()));
    let Err(error) = outcome else {
        return ExitCode::SUCCESS;
    };
    tracing::error!("{error}");
    ExitCode::from(1)
}

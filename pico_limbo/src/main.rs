#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
mod cli;
mod configuration;
mod forwarding;
mod handlers;
mod i18n;
mod kick_messages;
mod server;
mod server_brand;
mod server_state;
mod custom;

use crate::cli::Cli;
use clap::Parser;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let custom_options = cli.custom_options();

    server::start_server::start_server(
        cli.config_path,
        cli.verbose,
        None,
        custom_options,
    ).await
}
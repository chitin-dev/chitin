#![forbid(unsafe_code)]
//! Command-line access to Chitin database workflows.

mod cli;
mod download;
mod error;
mod structure;

use std::process::ExitCode;

use chitin_command::CommandReportStatus;
use clap::Parser;
use console::Style;

use crate::{cli::Cli, error::CliError};

/// Parses CLI input and executes the selected command.
#[tokio::main]
async fn main() -> ExitCode {
  let cli = Cli::parse();
  match cli::dispatch(cli.command).await {
    Ok(CommandReportStatus::Succeeded) => ExitCode::SUCCESS,
    Ok(CommandReportStatus::Failed) => ExitCode::FAILURE,
    Err(error) => report_error(error),
  }
}

/// Prints a user-facing error and returns the process failure code.
fn report_error(error: CliError) -> ExitCode {
  eprintln!("{} {error}", Style::new().red().bold().apply_to("error:"));
  ExitCode::FAILURE
}

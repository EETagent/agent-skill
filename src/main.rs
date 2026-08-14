use std::process::ExitCode;

use agent_skill::{
    cli::{self, Cli},
    terminal::sanitize_multiline,
};
use clap::Parser;

fn main() -> ExitCode {
    match cli::run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let message = sanitize_multiline(&format!("{error:#}"));
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

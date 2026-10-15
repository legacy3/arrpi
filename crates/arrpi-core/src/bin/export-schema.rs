//! Emit static public schemas for frontend code generation and drift checks.

use std::{io::Write as _, process::ExitCode};

fn main() -> ExitCode {
    match write() {
        Ok(()) => ExitCode::SUCCESS,

        Err(error) => {
            let _ = writeln!(std::io::stderr().lock(), "schema export: {error}");

            ExitCode::FAILURE
        }
    }
}

fn write() -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer_pretty(std::io::stdout().lock(), &arrpi_core::schema::document())?;
    writeln!(std::io::stdout().lock())?;
    Ok(())
}

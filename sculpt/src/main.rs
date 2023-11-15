use clap::{Parser, Subcommand};
use lalrpop_util::lalrpop_mod;

use std::fs::read_to_string;
use std::io::{self};
use std::path::PathBuf;

mod fmt;
mod report;
mod run;
mod syntax;

lalrpop_mod!(grammar);

use report::report_error;
use run::run;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run { file: PathBuf },
}

fn main() {
    let Args { command } = Args::parse();

    match command {
        Command::Run { file } => {
            let source_code = read_to_string(&file).unwrap();
            if let Err(error) = run(&source_code, io::stdout()) {
                let colored = true;
                report_error(&file, &source_code, error, colored, io::stderr());
            }
        }
    }
}

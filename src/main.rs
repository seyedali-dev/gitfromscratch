use clap::{Parser, Subcommand};
use std::fs;

/// Git directory.
const GIT_DIR: &str = ".customgit";

/// Application arguments.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

/// Git sub commands (init, add, commit, push, etc.)
#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize a new git repository.
    Init,
}

fn main() {
    let args = Args::parse();

    match args.command {
        Command::Init => {
            fs::create_dir(GIT_DIR).unwrap();
            fs::create_dir(format!("{GIT_DIR}/objects")).unwrap();
            fs::create_dir(format!("{GIT_DIR}/refs")).unwrap();
            fs::write(format!("{GIT_DIR}/HEAD"), "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory")
        }
        _ => {
            println!("Invalid command")
        }
    }
}

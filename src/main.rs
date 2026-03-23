//! imgcull — AI-powered image culling tool using vision LLMs.

mod cli;

use clap::Parser;
use cli::Cli;
use cli::Commands;

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Score(args) => {
            println!("Score: {:?}", args.paths);
        }
        Commands::Describe(args) => {
            println!("Describe: {:?}", args.paths);
        }
        Commands::Init => {
            println!("Init: would create config files");
        }
    }
}

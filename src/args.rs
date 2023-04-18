use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub struct Args {
    #[command(subcommand)]
    pub commands: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Configure {
        #[command(subcommand)]
        configure_commands: ConfigureCommands
    },
    Report {
        /// The index of the ICS file. Use `hour-calc configure ics list` to see available options
        #[clap(long, short)]
        ics_index: usize,
        /// The month to filter on. 1-12
        #[clap(long, short)]
        month: Option<u32>,
        /// The year to filter on
        #[clap(long, short)]
        year: Option<i32>,
    }
}

#[derive(Debug, Subcommand)]
pub enum ConfigureCommands {
    Ics {
        #[command(subcommand)]
        ics_commands: IcsCommands,
    }
}

#[derive(Debug, Subcommand)]
pub enum IcsCommands {
    List,
    Add {
        link: String,
    },
    Remove {
        index: usize,
    },
}
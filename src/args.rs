use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct Args {
    /// Path to your .ics file
    #[clap(long, short)]
    pub ics: PathBuf,
    /// The month to filter on. 1-12
    #[clap(long, short)]
    pub month: Option<u32>,
    /// The year to filter on
    #[clap(long, short)]
    pub year: Option<i32>,
}

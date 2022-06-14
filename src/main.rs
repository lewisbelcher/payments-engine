//! Program entrypoint and argument parsing.

use std::env;
use std::fs::File;
use std::io;

use anyhow::{anyhow, Result};

pub mod process;
pub mod types;

static ARG_MSG: &str = "Expected one positional argument (path to CSV file to process)";

/// Parse Arg
///
/// Parse a single positional argument, returning an error if anything other than that is present.
/// (Skipping a dependency on `Clap` or equivalent given how simple this is).
fn parse_arg() -> Result<String> {
	let mut args = env::args();
	if args.len() > 2 {
		return Err(anyhow!(ARG_MSG)); // Reject any unexpected args, just to be sure
	}
	args.nth(1).ok_or(anyhow!(ARG_MSG))
}

fn main() -> Result<()> {
	env_logger::init();
	let filepath = parse_arg()?;
	let mut input = File::open(filepath)?;
	let mut output = io::stdout();
	process::run(&mut input, &mut output)
}

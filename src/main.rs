use anyhow::Result;
use clap::Parser;
use jj_status_parser::Status;
use log::debug;
use std::io;
use std::io::BufRead;
use std::str::FromStr;

#[derive(Parser)]
struct Cli {
    /// Operate on the parent commit instead of the working copy
    #[clap(short, long)]
    parent: bool,

    /// Show the change id
    #[clap(long)]
    #[arg(group = "output")]
    change_id: bool,

    /// Output json
    #[clap(short, long, group = "output")]
    json: bool,

    /// Show the commit id
    #[clap(long)]
    #[arg(group = "output")]
    commit_id: bool,

    /// Show the bookmark (if exists)
    #[clap(short, long)]
    #[arg(group = "output")]
    bookmark: bool,

    /// Show the description
    #[clap(short, long)]
    #[arg(group = "output")]
    description: bool,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Cli::parse();

    let stdin: Vec<String> = io::stdin()
        .lock()
        .lines()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let stdin = stdin.join("\n");

    let status = Status::from_str(&stdin)?;
    debug!("{status}");

    let change = if args.parent {
        &status.parent_commit()
    } else {
        &status.working_copy()
    };

    let display = if args.json {
        &serde_json::to_string(&change)?
    } else if args.change_id {
        change.change_id()
    } else if args.commit_id {
        change.commit_id()
    } else if args.bookmark {
        match &change.bookmark() {
            Some(bookmark) => bookmark,
            None => "",
        }
    } else if args.description {
        change.description()
    } else {
        &change.to_string()
    };

    println!("{display}");

    Ok(())
}

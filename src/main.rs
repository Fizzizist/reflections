mod calendar;
mod cli;
mod db;
mod models;
mod repositories;
mod schema;
mod services;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use std::path::PathBuf;
use tui::run as tui_run;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Timeline(args)) => cli::run_timeline(args).await,
        Some(Command::Summary(args)) => match args.command {
            cli::SummaryCommand::Create(create_args) => cli::run_summary_create(create_args).await,
        },
        Some(Command::Meeting(args)) => match args.command {
            cli::MeetingCommand::Sync(sync_args) => cli::run_meeting_sync(sync_args).await,
        },
        None => {
            let db_path = PathBuf::from(crate::db::DEFAULT_DB_PATH);
            let root_dir = std::env::current_dir()?;
            tui_run(db_path, root_dir).await
        }
    }
}

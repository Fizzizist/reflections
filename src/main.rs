mod cli;
mod models;
mod repositories;
mod schema;
mod services;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use tui::run as tui_run;
use turso::Builder;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Timeline(args)) => cli::run_timeline(args).await,
        Some(Command::Summary(args)) => match args.command {
            cli::SummaryCommand::Create(create_args) => cli::run_summary_create(create_args).await,
        },
        None => {
            let db = Builder::new_local("reflections.db")
                .experimental_custom_types(true)
                .build()
                .await?;
            let conn = db.connect()?;
            schema::init_schema(&conn).await?;
            let root_dir = std::env::current_dir()?;
            tui_run(conn, root_dir).await
        }
    }
}

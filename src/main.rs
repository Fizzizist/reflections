mod models;
mod repositories;
mod schema;
mod services;
mod tui;

use anyhow::Result;
use tui::run as tui_run;
use turso::Builder;

#[tokio::main]
async fn main() -> Result<()> {
    let db = Builder::new_local("reflections.db")
        .experimental_custom_types(true)
        .build()
        .await?;
    let conn = db.connect()?;

    schema::init_schema(&conn).await?;
    schema::run_migrations(&conn).await?;

    let root_dir = std::env::current_dir()?;
    tui_run(conn, root_dir).await
}

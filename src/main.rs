mod models;
mod repositories;
mod schema;
mod services;
mod tui;

use anyhow::Result;
use services::meeting::MeetingService;
use services::todo::TodoService;
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

    let meeting_conn = db.connect()?;
    tui_run(TodoService::new(conn), MeetingService::new(meeting_conn)).await
}

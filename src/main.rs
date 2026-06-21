mod services;
mod tui;
use anyhow::Result;
use services::todo::TodoService;
use tui::run as tui_run;
use turso::Builder;

#[tokio::main]
async fn main() -> Result<()> {
    let db = Builder::new_local("reflections.db").build().await?;
    let conn = db.connect()?;

    tui_run(TodoService::new(conn)).await
}

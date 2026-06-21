mod tui;
use anyhow::Result;
use tui::run;

#[tokio::main]
async fn main() -> Result<()> {
    run().await
}

use crate::config::Config;
use crate::mcp::McpServer;
use std::path::Path;
use tracing::info;

pub async fn execute<P: AsRef<Path>>(
    config_path: Option<P>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = if let Some(path) = config_path {
        Config::load_from_file(path.as_ref())?
    } else {
        Config::default()
    };

    info!("Iniciando servidor MCP de PeakGuard (stdio)...");
    let server = McpServer::new(cfg);
    server.run_stdio().await?;
    Ok(())
}

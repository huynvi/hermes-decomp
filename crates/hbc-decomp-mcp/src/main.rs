use rmcp::ServiceExt;
use tokio::io::{stdin, stdout};

mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = server::HermesService::new();
    let server = service.serve((stdin(), stdout())).await?;
    server.waiting().await?;
    Ok(())
}

use dispatch_support_worker::{Result, config::Config, dispatch::run};
#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    run(config).await
}

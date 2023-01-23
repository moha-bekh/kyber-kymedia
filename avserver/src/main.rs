use log::LevelFilter;

use avservice::Result;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    let worker = libkypc::process::connect_to_commander().await?;
    avservice::run(worker).await
}

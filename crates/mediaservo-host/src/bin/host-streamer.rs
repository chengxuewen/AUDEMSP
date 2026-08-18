//! host-streamer: 推流进程（Phase A 占位）。

use mediaservo_host::{init_logging, run_placeholder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging("streamer");
    run_placeholder("streamer").await
}

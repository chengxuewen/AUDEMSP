//! host-recorder: 录制进程（Phase A 占位）。

use mediaservo_host::{init_logging, run_placeholder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging("recorder");
    run_placeholder("recorder").await
}

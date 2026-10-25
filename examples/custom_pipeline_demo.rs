//! custom_pipeline_demo（渲染架构批次 C/D）：自建 WGSL 管线 +
//! raw_render 直绘会话全链路验证。
//!
//! 运行：`cargo run -p pygame-rs --example custom_pipeline_demo`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("custom_pipeline_demo.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "cp_demo", false).await
}

starfish_std::app_entry!(run());

//! Surface 链路最小复现探针：四象限（纯 fill / 2D 形状 / 深度面 /
//! 1px 物理调试线）逐项验证 blit 上屏——任一象限空白即定位链路断点。
//!
//! 运行：`cargo run -p pygame-rs --example surf_probe`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("surf_probe.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "surf_probe", false).await
}

starfish_std::app_entry!(run());

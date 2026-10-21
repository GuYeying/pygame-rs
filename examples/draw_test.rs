//! pygame.draw 模块专项测试（逐模块独立案例 2/N）。
//!
//! 共享宿主壳（`rpy::testhost::run`）；脚本覆盖六形状 × 填充/描边 ×
//! 颜色形态 × 半透明叠加 × gfxdraw 兼容混绘（[draw-test] 行逐项断言 +
//! 全形状样张上屏）。
//!
//! 运行：`cargo run --release -p pygame-rs --example draw_test`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("draw_test.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "draw_test", false).await
}

starfish_std::app_entry!(run());

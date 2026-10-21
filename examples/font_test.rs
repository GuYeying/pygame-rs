//! pygame.font 模块专项测试（逐模块独立案例 1/N）。
//!
//! 共享宿主壳（`rpy::testhost::run`）固化装配/字体预载/事件泵/帧循环；
//! 本案例只提供 Python 脚本（`font_test.py`：构造族/度量/render 一等化/
//! CJK 链路逐项断言 + 字体样张上屏）。
//!
//! 运行：`cargo run --release -p pygame-rs --example font_test`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("font_test.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "font_test", false).await
}

starfish_std::app_entry!(run());

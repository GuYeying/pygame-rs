//! pygame_gui 对接专项测试（渲染对接 v1，2026-10-24）。
//!
//! 覆盖：init 幂等 / frame(callback) 即时模式 UI / 叠加通道 draw /
//! wants_pointer 查询 / shutdown。[egui-test] 行逐项断言 + HUD 样张上屏
//! （视觉验收：egui 面板画在游戏画面之上、中文正常）。
//!
//! 运行：`cargo run --release -p pygame-rs --example egui_test`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("egui_test.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "egui_test", false).await
}

starfish_std::app_entry!(run());



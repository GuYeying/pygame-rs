//! pygame_physics 对接专项测试（渲染对接 v1，2026-10-24）。
//!
//! 覆盖：PhysicsWorld 构造/insert/step_frame/查询 + collect 线段流 +
//! DebugLinesRenderer 叠加通道上屏（渲染对接核心验收——调试线画到
//! pygame 屏幕上）。[rapier-test] 行逐项断言 + 落体场景样张上屏。
//!
//! 运行：`cargo run --release -p pygame-rs --example rapier_test`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("rapier_test.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "rapier_test", false).await
}

starfish_std::app_entry!(run());

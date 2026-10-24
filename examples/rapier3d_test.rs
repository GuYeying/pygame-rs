//! pygame_physics3d 对接专项测试（3D 调试渲染 v1，2026-10-24）。
//!
//! 覆盖：PhysicsWorld3D 构造/insert/step_frame/位姿查询 + collect 线段流
//! + DebugLinesRenderer3D（look-at 透视相机）叠加通道上屏（**深度通道**
//! ——3D 线管线硬契约）。[rapier3d-test] 行逐项断言 + 落体场景样张。
//!
//! 运行：`cargo run --features physics3d -p pygame-rs --example rapier3d_test`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("rapier3d_test.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "rapier3d_test", false).await
}

starfish_std::app_entry!(run());

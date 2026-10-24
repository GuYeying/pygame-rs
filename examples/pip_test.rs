//! 画中画 / 游戏内面板验证（批次 48：Surface 会话客人叠加通道）。
//!
//! 验证三件事：①3D 物理调试动画画进**离屏 Surface**（深度通道 +
//! look-at 相机）→ blit 上屏（画中画视口）；②egui 面板画进离屏 Surface
//! （游戏内面板）；③主屏画面与两块画中画同帧合成。[pip-test] 逐项断言
//! + 保持窗形态（关窗/ESC 收尾）。
//!
//! 运行：`cargo run --features physics3d -p pygame-rs --example pip_test`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("pip_test.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "pip_test", false).await
}

starfish_std::app_entry!(run());

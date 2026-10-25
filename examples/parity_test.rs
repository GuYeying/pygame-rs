//! 绑定收官 parity 验证（批次 49 新接口：颜色表/色彩空间/mixer
//! Channel+Effect/深度会话）。
//!
//! [parity-test] 逐项断言 + 色板/深度会话样张 + mixer 音频（Web 需手势
//! 解锁）。保持窗形态（关窗/ESC 收尾）。
//!
//! 运行：`cargo run -p pygame-rs --example parity_test`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("parity_test.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "parity_test", false).await
}

starfish_std::app_entry!(run());

//! pygame 透明窗口专项验证（逐模块独立案例；桌宠形态基座）。
//!
//! 共享宿主壳 `transparent=true`：TRANSPARENT 创建旗标 + 透明合成 +
//! 跳过铺底。脚本只画主体（无不透明底色）——桌面透出 + AA 边缘 +
//! set_opacity 呼吸。验收以肉眼为准（透明质量），控制台为程序化断言。
//!
//! 运行：`cargo run --release -p pygame-rs --example transparent_test`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("transparent_test.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "transparent_test", true).await
}

starfish_std::app_entry!(run());

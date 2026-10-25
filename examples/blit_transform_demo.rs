//! blit_transform 演示（渲染架构批次五十二）：单接口收拢——Mat4
//! 承载一切变换（上半 2D 仿射三联 + 烘焙对照；下半透视相机 + 模型
//! 矩阵空间放置，近大远小 + 深度遮挡）。
//!
//! 运行：`cargo run -p pygame-rs --example blit_transform_demo`

use pygame_rs::rpy;

const SCRIPT: &str = include_str!("blit_transform_demo.py");

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    rpy::testhost::run(SCRIPT, "blit_demo", false).await
}

starfish_std::app_entry!(run());

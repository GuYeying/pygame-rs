//! web_repro：Web 端"多 blit + draw 混合内容丢失"最小复现（纯 starfish API）。
//!
//! 画面（400×300，深红底）：
//!   绿方块 (20,20) ← blit    蓝矩形 (200,20) ← draw.rect
//!   绿方块 (20,100) ← blit   白矩形 (280,100) ← draw.rect
//! 预期四元素同屏稳定；Web 疑似只剩最后的 draw（中间 blit 被清）。
//!
//! 运行：桌面 cargo run --release --example web_repro
//! Web：python scripts/build_web.py web_repro → http.server 查看

use starfish::base::window::next_frame;
use starfish::pygame::{display, draw, render::Surface};
use starfish::pygame::Rect;

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    display::set_mode((400, 300)).await?;

    let green = Surface::new((40, 40));
    green.fill([0, 200, 70, 255]);

    let mut frame = 0u32;
    loop {
        let screen = display::get_screen();
        screen.fill([120, 20, 30, 255]); // 深红底
        screen.blit(&green, (20, 20)); // blit 1
        draw::rect(
            screen,
            [40, 80, 220, 255],
            Rect::new(200, 20, 60, 60),
            0,
        ); // draw 1
        screen.blit(&green, (20, 100)); // blit 2
        draw::rect(
            screen,
            [240, 240, 240, 255],
            Rect::new(280, 100, 60, 60),
            0,
        ); // draw 2
        display::flip();

        frame += 1;
        if frame % 60 == 0 {
            starfish::base::debug::console_log(&format!("web_repro frame {frame}"));
        }
        next_frame().await;
    }
}

starfish::app_entry!(run());

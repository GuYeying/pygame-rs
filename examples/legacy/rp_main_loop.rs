//! rp_main_loop：**主循环归 rpy**——Python 生成器门驱动 starfish 窗口。
//!
//! Python 帧逻辑在生成器 `game()` 里，Rust 每帧调 `__next__` 步进。
//! yield 是帧边界标记（pygbag 同款模式），Web 端 rAF 自然节流。
//!
//! 运行：`cargo run --release -p pygame-rs --example rp_main_loop`

use pygame_rs::rpy;
use starfish::base::window::next_frame;
use starfish::pygame::display;
use starfish::pygame::Event;
use std::cell::RefCell;
use std::io::Write;

/// 诊断日志写入文件（卓易通等 logcat 不可用时的备用）
const LOG_PATH: &str = "/sdcard/Download/rp_log.txt";

/// Android 专用诊断：TCP 直传 PC 收集器（192.168.124.13:9000），失败静默
#[cfg(target_os = "android")]
fn net_log(msg: &str) {
    use std::io::Write;
    if let Ok(mut s) = std::net::TcpStream::connect(("192.168.124.13", 9000)) {
        let req = format!(
            "POST /log HTTP/1.1\r\nHost: 192.168.124.13:9000\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            msg.len(),
            msg
        );
        let _ = s.write_all(req.as_bytes());
        let _ = s.flush();
    }
}

fn dlog(msg: &str) {
    starfish::base::debug::console_log(msg);
    #[cfg(target_os = "android")]
    net_log(msg);
}

/// Python 帧逻辑脚本（生成器门：yield = 帧边界）
const SCRIPT: &str = r#"
def game():
    x = 370
    while True:
        if quit_requested():
            return
        fill(30, 30, 46, 255)
        rect(x, 270, 60, 60, 0, 200, 90, 255)
        x = 0 if x >= 730 else x + 4
        flip()
        yield

# 创建生成器（不启动——由 Rust __next__ 驱动）
game = game()
"#;

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    dlog("=== rp_main_loop 启动 ===");
    dlog("set_mode 开始");

    let interp = rpy::new_interpreter();
    dlog("RP genesis 完成");

    let gen_cell: RefCell<Option<rustpython_vm::PyObjectRef>> = RefCell::new(None);

    // 装配阶段：注入 sf 原生面 → 执行脚本（定义 game 生成器）→ 创建生成器
    interp.enter(|vm| {
        dlog("[enter] scope 创建");
        let scope = vm.new_scope_with_builtins();
        rpy::bridge::register(vm, &scope);
        dlog("[enter] sf natives 注入完成");
        let code = vm
            .compile(
                SCRIPT,
                rustpython_vm::compiler::Mode::Exec,
                "main.py".to_owned(),
            )
            .map_err(|e| e.to_string())?;
        dlog("[enter] 脚本编译完成");
        vm.run_code_obj(code, scope.clone())
            .map_err(|e| format!("{e:?}"))?;
        dlog("[enter] run_code_obj 完成");
        let game = scope
            .globals
            .get_item("game", vm)
            .map_err(|e| format!("{e:?}"))?;
        dlog("[enter] game 获取完成");
        // game 全局已是生成器对象（脚本 game = game() 创建），直接使用
        *gen_cell.borrow_mut() = Some(game);
        dlog("[enter] 生成器获取完成");
        Ok::<(), String>(())
    })?;

    // 主循环（调用方持）：事件泵 → Python 帧步进 → 上屏 → 帧拍
    dlog("进入帧循环");
    loop {
        if rpy::bridge::pump() {
            dlog("[rp_main_loop] 退出请求，结束");
            break;
        }

        let mut done = false;
        interp.enter(|vm| {
            let borrowed = gen_cell.borrow();
            let Some(genobj) = borrowed.as_ref() else {
                done = true;
                return;
            };
            match vm.call_method(genobj, "__next__", ()) {
                Ok(_) => {} // yield：本帧逻辑完成
                Err(e) => {
                    let obj: rustpython_vm::PyObjectRef = e.clone().into();
                    let msg = obj
                        .repr(vm)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| format!("{e:?}"));
                    println!("[rp_main_loop] 脚本异常: {msg}");
                    done = true;
                }
            }
        });
        if done {
            break;
        }

        next_frame().await;
    }
    // 先关窗再卸载（避免 RP 卸载期间窗口 ghost 无响应）
    display::quit();
    dlog("[rp_main_loop] Python 主循环结束，进程退出");
    println!("[rp_main_loop] Python 主循环结束，进程退出");
    Ok(())
}

starfish::app_entry!(run());

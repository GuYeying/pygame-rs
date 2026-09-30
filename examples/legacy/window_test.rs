//! rpy 主循环窗口测试（M1 先行切片）：**Python 脚本驱动 starfish 窗口**。
//!
//! 职责划分（生成器门）：
//! - Rust：窗口装配 + 事件泵 + 生成器门（每帧 `__next__` 步进 Python）+ 帧拍；
//! - Python：`game()` 生成器 = 帧逻辑（填充/画方块/翻转，方块移动）。
//!
//! 诊断日志写入 `/sdcard/Download/wt_log.txt`（卓易通等 logcat 不可用时）。
//!
//! 运行：`cargo run -p pygame-rs --example window_test`
//! 预期：深蓝底上绿色方块从左向右移动（循环），窗口 ×/ESC 退出。

use pygame_rs::rpy;
use starfish::base::window::next_frame;
use starfish::pygame::display;
use std::cell::RefCell;
use std::io::Write;

const LOG_PATH: &str = "/sdcard/Download/wt_log.txt";
/// 兜底路径：/sdcard/Download 不可写时尝试 /sdcard 根目录
const LOG_PATH_FALLBACK: &str = "/sdcard/wt_log_fb.txt";
/// 最终兜底：TCP 直传 PC（卓易通沙盒内所有文件路径都可能不可写/不可见）
const LOG_PC: Option<(&str, u16)> = Some(("192.168.124.13", 9000));

/// Android 专用：单行 HTTP POST 到 PC 收集器（连接失败静默——诊断通道不干扰主流程）
#[cfg(target_os = "android")]
fn net_log(msg: &str) {
    use std::io::Write;
    if let Some((host, port)) = LOG_PC {
        if let Ok(mut s) = std::net::TcpStream::connect((host, port)) {
            let req = format!(
                "POST /log HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                msg.len(),
                msg
            );
            let _ = s.write_all(req.as_bytes());
            let _ = s.flush();
        }
    }
}

fn dlog(msg: &str) {
    println!("{}", msg);
    starfish::base::debug::console_log(msg);
    for path in [LOG_PATH, LOG_PATH_FALLBACK] {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{}", msg);
        }
    }
    #[cfg(target_os = "android")]
    net_log(msg);
}

/// Python 帧逻辑脚本（生成器门：yield = 帧边界）
/// 原生名对齐 sf.rs register 注入的扁平名（fill/rect/flip/quit_requested，
/// 批次十七起去 sf_ 前缀——旧 sf_* 名会 NameError 即退）。
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
"#;

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    dlog("[window_test] starting");
    // 立即 set_mode（不在启动早期睡眠/阻塞——横屏旋转的 TerminateWindow/
    // InitWindow 会重建 ANativeWindow，迟到的 set_mode 会拿旧 window 配
    // surface 报 Invalid surface；旋转由帧循环的 Resized→resize 正常消化）
    match display::set_mode((800, 600)).await {
        Ok(_) => dlog("[window_test] set_mode OK"),
        Err(e) => {
            dlog(&format!("[window_test] set_mode ERR: {e}"));
            return Err(e.into());
        }
    }

    let interp = rpy::new_interpreter();
    dlog("[window_test] RP genesis 完成");
    let gen_cell: RefCell<Option<rustpython_vm::PyObjectRef>> = RefCell::new(None);

    // 装配阶段：注入 sf 原生面 → 执行脚本（定义 game 生成器）→ 创建生成器
    dlog("[window_test] executing script");
    interp.enter(|vm| {
        let scope = vm.new_scope_with_builtins();
        rpy::bridge::register(vm, &scope);
        let code = vm
            .compile(
                SCRIPT,
                rustpython_vm::compiler::Mode::Exec,
                "window_test.py".to_owned(),
            )
            .map_err(|e| e.to_string())?;
        vm.run_code_obj(code, scope.clone())
            .map_err(|e| format!("{e:?}"))?;
        let game = scope
            .globals
            .get_item("game", vm)
            .map_err(|e| format!("{e:?}"))?;
        let genobj = game.call((), vm).map_err(|e| format!("{e:?}"))?;
        *gen_cell.borrow_mut() = Some(genobj);
        dlog("[window_test] generator created");
        Ok::<(), String>(())
    })?;

    // 主循环（调用方持）：事件泵 → Python 帧步进 → 上屏 → 帧拍
    dlog("[window_test] entering frame loop");
    loop {
        if rpy::bridge::pump() {
            dlog("[window_test] 退出请求，结束");
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
                Err(exc) => {
                    let obj: rustpython_vm::PyObjectRef = exc.clone().into();
                    let msg = obj
                        .repr(vm)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| format!("{exc:?}"));
                    dlog(&format!("[window_test] 脚本异常: {msg}"));
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
    dlog("[window_test] main loop ended");
    Ok(())
}

starfish::app_entry!(run());

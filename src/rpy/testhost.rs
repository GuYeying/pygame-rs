//! 逐模块独立测试案例的共享宿主壳（binding_probe 同构固化）。
//!
//! 模块专项案例（examples/<模块>_test.rs）只提供 Python 脚本，装配/
//! 字体预载/事件泵/帧循环/收口全部由 [`run`] 固化——案例间零重复。
//! 收口顺序与坑位（关窗先于卸载、mixer 显式收口）与 binding_probe
//! 保持一字不差。
//!
//! 用法（案例侧）：
//! ```rust, ignore
//! const SCRIPT: &str = include_str!("font_test.py");
//! async fn run() -> Result<(), Box<dyn std::error::Error>> {
//!     pygame_rs::rpy::testhost::run(SCRIPT, "font_test").await
//! }
//! starfish::app_entry!(run());
//! ```

use crate::rpy;
use starfish::base::window::next_frame;
use starfish::pygame::display;
use std::cell::RefCell;
use std::io::Write;

/// 诊断日志（println + console_log + 安卓 TCP 收集器 + 文件兜底；
/// 与 binding_probe 的 dlog 同构）
fn dlog(tag: &str, msg: &str) {
    let line = format!("[{tag}] {msg}");
    println!("{line}");
    starfish::base::debug::console_log(&line);
    #[cfg(target_os = "android")]
    net_log(&line);
    #[cfg(target_os = "android")]
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/sdcard/Download/bp_log.txt")
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Android 专用诊断：单行 HTTP POST 到 PC 收集器（300ms 超时 + 一次失败
/// 拉黑——无超时阻塞 connect 在主线程 = ANR）
#[cfg(target_os = "android")]
fn net_log(msg: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static TCP_DEAD: AtomicBool = AtomicBool::new(false);
    if TCP_DEAD.load(Ordering::Relaxed) {
        return;
    }
    let addr: std::net::SocketAddr = "192.168.124.13:9000".parse().unwrap();
    match std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300)) {
        Ok(mut s) => {
            let req = format!(
                "POST /log HTTP/1.1\r\nHost: 192.168.124.13:9000\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                msg.len(),
                msg
            );
            let _ = s.write_all(req.as_bytes());
            let _ = s.flush();
        }
        Err(_) => TCP_DEAD.store(true, Ordering::Relaxed),
    }
}

/// CJK 字体预载链（三平台；批次 G + Android 系统候选链——与
/// binding_probe 保持一字不差，命中即 log）
async fn preload_cjk(tag: &str) {
    #[cfg(windows)]
    match std::fs::read("C:/Windows/Fonts/msyh.ttc") {
        Ok(b) => {
            starfish::pygame::font::preload("cjk", b);
            dlog(tag, "CJK 字体已预载（系统 msyh.ttc）");
        }
        Err(_) => dlog(tag, "CJK 预载失败（msyh.ttc 不存在）"),
    }
    match starfish::base::io::read("fonts/cjk.ttf").await {
        Ok(b) => {
            starfish::pygame::font::preload("cjk", b);
            dlog(tag, "CJK 字体已预载（fonts/cjk.ttf）");
        }
        Err(_) => dlog(tag, "CJK 字体未找到（fonts/cjk.ttf）"),
    }
    #[cfg(target_os = "android")]
    {
        const CJK_CANDIDATES: [&str; 7] = [
            "/system/fonts/NotoSansCJK-Regular.ttc",
            "/system/fonts/NotoSansCJKjp-Regular.otf",
            "/system/fonts/NotoSansSC-Regular.otf",
            "/system/fonts/NotoSerifCJK-Regular.ttc",
            "/system/fonts/HarmonyOS_Sans_SC.ttf",
            "/system/fonts/miui.ttf",
            "/system/fonts/DroidSansFallback.ttf",
        ];
        let mut hit = false;
        for p in CJK_CANDIDATES {
            match starfish::base::io::read(p).await {
                Ok(b) => {
                    starfish::pygame::font::preload("cjk", b);
                    dlog(tag, &format!("CJK 字体已预载（系统 {p}）"));
                    hit = true;
                    break;
                }
                Err(_) => continue,
            }
        }
        if !hit {
            dlog(tag, "CJK 系统字体候选全 miss（中文将缺字）");
        }
    }
}

/// 运行一个模块专项测试脚本（生成器门：脚本定义 `game()`，宿主持循环）
///
/// - `script`：include 静态嵌入的 Python 脚本全文；
/// - `tag`：日志前缀（如 "font_test"）；
/// - `transparent`：透明窗口创建（TRANSPARENT 创建旗标 + 引擎透明底
///   清屏——桌宠/悬浮件形态。透明时跳过铺底，脚本内勿填不透明底色）。
pub async fn run(script: &str, tag: &str, transparent: bool) -> Result<(), Box<dyn std::error::Error>> {
    use starfish::pygame::display as dflags;
    dlog(tag, "starting");
    let flags = if transparent { dflags::TRANSPARENT } else { 0 };
    match display::set_mode_ex((800, 600), flags).await {
        Ok(_) => {
            dlog(tag, "set_mode OK");
        }
        Err(e) => {
            dlog(tag, &format!("set_mode ERR: {e}"));
            return Err(e.into());
        }
    }

    preload_cjk(tag).await;

    let interp = rpy::new_interpreter();
    dlog(tag, "RP genesis 完成");

    let gen_cell: RefCell<Option<rustpython_vm::PyObjectRef>> = RefCell::new(None);

    // 装配：原生 pygame 模块注入 → 用户脚本 → 生成器（enter 内、
    // essential init 完成后——init_hook 时点会爆栈）
    let install_err: RefCell<Option<String>> = RefCell::new(None);
    interp.enter(|vm| {
        let run = || -> Result<(), String> {
            let scope = vm.new_scope_with_builtins();
            rpy::install_pygame(vm);
            dlog(tag, "pygame 原生模块已注入");
            let code = vm
                .compile(
                    script,
                    rustpython_vm::compiler::Mode::Exec,
                    format!("{tag}.py"),
                )
                .map_err(|e| e.to_string())?;
            if let Err(e) = vm.run_code_obj(code, scope.clone()) {
                let obj: rustpython_vm::PyObjectRef = e.clone().into();
                let msg = obj
                    .repr(vm)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| format!("{e:?}"));
                return Err(format!("脚本执行异常: {msg}"));
            }
            let game = scope
                .globals
                .get_item("game", vm)
                .map_err(|e| format!("{e:?}"))?;
            let genobj = match game.call((), vm) {
                Ok(g) => g,
                Err(e) => {
                    let obj: rustpython_vm::PyObjectRef = e.clone().into();
                    let msg = obj
                        .repr(vm)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| format!("{e:?}"));
                    return Err(format!("生成器创建异常: {msg}"));
                }
            };
            *gen_cell.borrow_mut() = Some(genobj);
            dlog(tag, "generator created");
            Ok(())
        };
        if let Err(msg) = run() {
            *install_err.borrow_mut() = Some(msg);
        }
    });
    if let Some(msg) = install_err.into_inner() {
        dlog(tag, &msg);
        display::quit();
        return Err(msg.into());
    }

    // 主循环（调用方持）：事件泵 → Python 帧步进 → 上屏 → 帧拍
    dlog(tag, "entering frame loop");
    loop {
        if starfish::pygame::event::pump() {
            dlog(tag, "退出请求，结束");
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
                    dlog(tag, &format!("脚本异常: {msg}"));
                    done = true;
                }
            }
        });
        if done {
            break;
        }

        starfish::pygame::mixer::pump(); // Web 流式解码驱动（桌面 no-op）
        next_frame().await;
    }
    // 先关窗再卸载（RP 卸载期间窗口 ghost）；mixer 显式收口（TLS 析构期）
    display::quit();
    starfish::pygame::mixer::quit();
    dlog(tag, "main loop ended");
    Ok(())
}

//! binding_probe：M1 绑定面验收（标准 pygame 写法）。
//!
//! 与 window_test 同壳（生成器门），差异：
//! - 装配阶段多一步 `rpy::install_pygame`（原生 pygame 模块注入 sys.modules，
//!   批次 B 起 `pygame_shim.py` 已删）；
//! - 用户脚本 `binding_probe.py` = 标准 `import pygame` 写法（include 静态
//!   嵌入，三平台无文件系统依赖）。
//!
//! 渲染走 ADR-5 with 语法：脚本 `with screen.render():` 内 draw_*。
//!
//! 运行：`cargo run --release -p pygame-rs --example binding_probe`

use pygame_rs::rpy;
use starfish::base::window::next_frame;
use starfish::pygame::display;
use std::cell::RefCell;
use std::io::Write;

/// 诊断日志（卓易通等 logcat 不可用时的文件兜底 + TCP devlog 通道）
const LOG_PATH: &str = "/sdcard/Download/bp_log.txt";

fn dlog(msg: &str) {
    println!("{}", msg);
    starfish::base::debug::console_log(msg);
    #[cfg(target_os = "android")]
    net_log(msg);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_PATH)
    {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Android 专用诊断：单行 HTTP POST 到 PC 收集器（300ms 超时 + 一次失败
/// 拉黑——无超时阻塞 connect 在主线程 = ANR，2026-09-28 实测坑）
#[cfg(target_os = "android")]
fn net_log(msg: &str) {
    use std::io::Write;
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

/// 用户脚本（标准 pygame 写法；include 静态嵌入）
const SCRIPT: &str = include_str!("binding_probe.py");

/// 加载进度帧（genesis/字体预载阻塞主线程——静态帧给用户"活着"的反馈）。
/// pct: 0..=100。固定 800×600 坐标（壳自控 set_mode 尺寸）。
fn loading_frame(pct: u32) {
    let screen = starfish::pygame::display::get_screen();
    //screen.fill([30, 30, 46, 255]);
    // 底部进度条（背景条 + 前景条；quad 直推——Screen 无 &T DrawTarget 转发）
    let pct = pct.min(100) as f32;
    screen.with_batch(|b| {

        b.push_quad(200.0, 520.0, 400.0, 18.0, [60.0, 60.0, 84.0, 255.0], None);
        let w = 400.0 * pct / 100.0;
        if w >= 1.0 {
            b.push_quad(200.0, 520.0, w, 18.0, [120.0, 200.0, 255.0, 255.0], None);
        }
    });
    display::flip();
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // 时基 = base::time 跨平台时钟（std Instant 在 wasm32-unknown-unknown
    // 上 panic——unsupported.rs，批次二十四 wasm 启动 panic 根因）
    let t0 = starfish::base::time::since_start_ms();
    let stage = |name: &str| {
        dlog(&format!(
            "[t] +{}ms {name}",
            starfish::base::time::since_start_ms() - t0
        ));
    };

    dlog("[binding_probe] starting");

    // CJK 字节后台预读（I/O 与 GPU 初始化/genesis 重叠；cache set 在主线程）
    #[cfg(target_os = "android")]
    const CJK_CANDIDATES: [&str; 7] = [
        "/system/fonts/NotoSansCJK-Regular.ttc", // 原生 Android / CN ROM
        "/system/fonts/NotoSansCJKjp-Regular.otf",
        "/system/fonts/NotoSansSC-Regular.otf",
        "/system/fonts/NotoSerifCJK-Regular.ttc",
        "/system/fonts/HarmonyOS_Sans_SC.ttf", // 鸿蒙 NEXT
        "/system/fonts/miui.ttf",              // 小米系
        "/system/fonts/DroidSansFallback.ttf", // 老安卓兜底
    ];
    #[cfg(target_os = "android")]
    let cjk_thread = std::thread::spawn(|| {
        for p in CJK_CANDIDATES {
            if let Ok(b) = std::fs::read(p) {
                return Some((p.to_string(), b));
            }
        }
        None
    });

    // 立即 set_mode（启动路径禁止延迟——横屏旋转 × 迟到装配 = Invalid surface）
    match display::set_mode((800, 600)).await {
        Ok(_) => {
            stage("set_mode OK");

            // 铺底 + 进度条 15%（覆盖重建表面）。
            // 批次二十四定约：权限申请不在此处调用——本 demo 只播放不录音
            // （播放无需 RECORD_AUDIO），启动时阻塞申请曾致 15s 空等
            // （ok=false，卓易通容器内轮询未获授权状态，见权限备忘）。
            // 录音功能立项时由 audio/record.rs 的隐式约定申请。
            loading_frame(15);
        }
        Err(e) => {
            dlog(&format!("[binding_probe] set_mode ERR: {e}"));
            return Err(e.into());
        }
    }

    // CJK 字体预加载（批次 G）：io 三后端（桌面 fs / Web fetch / Android fs）
    // Windows 优先系统微软雅黑（TTC index0 = 常规体，机器必有）；未预载时
    // Font("cjk") 构造报错 → demo 回退 Antonio 拉丁（中文缺字静默跳过）
    // 通用字体加载演示：仓库根的装饰字体（风格由字体文件决定）
    #[cfg(windows)]
    match std::fs::read("Canterbury.ttf") {
        Ok(b) => {
            starfish::pygame::font::preload("canterbury", b);
            dlog("[binding_probe] Canterbury 已预载（哥特风演示）");
        }
        Err(_) => dlog("[binding_probe] Canterbury.ttf 未找到（跳过）"),
    }
    #[cfg(windows)]
    match std::fs::read("C:/Windows/Fonts/msyh.ttc") {
        Ok(b) => {
            starfish::pygame::font::preload("cjk", b);
            dlog("[binding_probe] CJK 字体已预载（系统 msyh.ttc）");
        }
        Err(_) => dlog("[binding_probe] CJK 预载失败（msyh.ttc 不存在）"),
    }
    match starfish::base::io::read("fonts/cjk.ttf").await {
        Ok(b) => {
            starfish::pygame::font::preload("cjk", b);
            dlog("[binding_probe] CJK 字体已预载（fonts/cjk.ttf）");
        }
        Err(_) => dlog("[binding_probe] CJK 字体未找到（fonts/cjk.ttf，中文将缺字）"),
    }

    // Android：字体字节由开局的 cjk_thread 后台预读（与 set_mode/genesis
    // 重叠）——此处 join 并入缓存（preload 在主线程，缓存非并发）。
    #[cfg(target_os = "android")]
    if let Ok(joined) = cjk_thread.join() {
        match joined {
            Some((p, b)) => {
                starfish::pygame::font::preload("cjk", b);
                stage(&format!("CJK 字体已预载（系统 {p}）"));
            }
            None => dlog("[binding_probe] CJK 系统字体候选全 miss（中文将缺字）"),
        }
    }
    #[cfg(target_os = "android")]
    loading_frame(45);

    let interp = rpy::new_interpreter();
    stage("RP genesis 完成");
    #[cfg(target_os = "android")]
    loading_frame(75);

    let gen_cell: RefCell<Option<rustpython_vm::PyObjectRef>> = RefCell::new(None);

    // 装配：sf 原生面 → 原生 pygame 模块注入 → 用户脚本 → 生成器
    // （install 必须在 enter 内、essential init 完成后——init_hook 时点会爆栈）
    let install_err: RefCell<Option<String>> = RefCell::new(None);
    interp.enter(|vm| {
        let run = || -> Result<(), String> {
            let scope = vm.new_scope_with_builtins();
            rpy::install_pygame(vm);
            dlog("[binding_probe] pygame 原生模块已注入");
            stage("原生模块注入完成");
            let code = vm
                .compile(
                    SCRIPT,
                    rustpython_vm::compiler::Mode::Exec,
                    "binding_probe.py".to_owned(),
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
            dlog("[binding_probe] generator created");
            stage("generator created");
            Ok(())
        };
        if let Err(msg) = run() {
            *install_err.borrow_mut() = Some(msg);
        } else {
            #[cfg(target_os = "android")]
            loading_frame(100);
        }
    });
    if let Some(msg) = install_err.into_inner() {
        dlog(&format!("[binding_probe] {msg}"));
        display::quit();
        return Err(msg.into());
    }

    // 主循环（调用方持）：事件泵 → Python 帧步进 → 上屏 → 帧拍
    dlog("[binding_probe] entering frame loop");

    // Web：浏览器授权框异步触发（首次交互后最稳；授权流缓存待录音复用）
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    starfish::base::app::spawn_local(async {
        let ok = starfish::base::permission::ensure_async(
            starfish::base::permission::Permission::Microphone,
        )
        .await;
        starfish::base::debug::console_log(&format!(
            "[permission] Web 麦克风授权结果: {ok}"
        ));
    });
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    dlog("[permission] 桌面无运行时权限模型（直通）");

    // 首帧 present 打点 ×3（批次二十四：呈现链路时序定位）
    let mut frames: u32 = 0;

    loop {
        if starfish::pygame::event::pump() {
            dlog("[binding_probe] 退出请求，结束");
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
                    dlog(&format!("[binding_probe] 脚本异常: {msg}"));
                    done = true;
                }
            }
        });
        if done {
            break;
        }

        frames += 1;
        if frames <= 3 {
            dlog(&format!(
                "[t] +{}ms frame #{frames} presented",
                starfish::base::time::since_start_ms() - t0
            ));
        }

        starfish::pygame::mixer::pump();  // Web 流式解码驱动（桌面 no-op）
        next_frame().await;
    }
    // 先关窗再卸载（避免 RP 卸载期间窗口 ghost 无响应）；
    // 音频混音器显式收口（cpal 流不留到主线程 TLS 析构期）
    display::quit();
    starfish::pygame::mixer::quit();
    dlog("[binding_probe] main loop ended");
    Ok(())
}

starfish::app_entry!(run());

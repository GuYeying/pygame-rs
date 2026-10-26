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
use starfish_we::next_frame;
use starfish_pygame::display;
use std::cell::{Cell, RefCell};
use std::io::Write;

/// 诊断日志（console_log 单路输出：桌面 stdout / wasm 浏览器控制台 +
/// 安卓 TCP 收集器 + 文件兜底；与 binding_probe 的 dlog 同构）
fn dlog(tag: &str, msg: &str) {
    let line = format!("[{tag}] {msg}");
    starfish_std::debug::console_log(&line);
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
pub async fn preload_cjk(tag: &str) {
    #[cfg(windows)]
    match std::fs::read("C:/Windows/Fonts/msyh.ttc") {
        Ok(b) => {
            starfish_pygame::font::preload("cjk", b);
            dlog(tag, "CJK 字体已预载（系统 msyh.ttc）");
        }
        Err(_) => dlog(tag, "CJK 预载失败（msyh.ttc 不存在）"),
    }
    match starfish_io::read("fonts/cjk.ttf").await {
        Ok(b) => {
            starfish_pygame::font::preload("cjk", b);
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
            match starfish_io::read(p).await {
                Ok(b) => {
                    starfish_pygame::font::preload("cjk", b);
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

/// 经典 Python traceback 格式化（RustPython `vm.write_exception` 直通：
/// Traceback (most recent call last) → 逐帧 File/line/in func →
/// 异常类型: 消息；含 __cause__/__context__ 链与 SyntaxError 行列）。
/// 异常出口（脚本执行 / 生成器创建 / 帧循环）统一走这里。
fn format_traceback(
    vm: &rustpython_vm::VirtualMachine,
    exc: &rustpython_vm::builtins::PyBaseExceptionRef,
) -> String {
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    let _ = vm.write_exception(
        &mut rustpython_vm::py_io::IoWriter(&mut buf),
        exc,
    );
    String::from_utf8_lossy(buf.get_ref()).trim_end().to_string()
}

/// 运行一个模块专项测试脚本（生成器门：脚本定义 `main()`，宿主持循环）
///
/// - `script`：include 静态嵌入的 Python 脚本全文；
/// - `tag`：日志前缀（如 "font_test"）；
/// - `transparent`：透明窗口创建（TRANSPARENT 创建旗标 + 引擎透明底
///   清屏——桌宠/悬浮件形态。透明时跳过铺底，脚本内勿填不透明底色）。
pub async fn run(script: &str, tag: &str, transparent: bool) -> Result<(), Box<dyn std::error::Error>> {
    use starfish_pygame::display as dflags;
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
            rpy::install_modules(vm);
            dlog(tag, "pygame 原生模块已注入");
            let code = match vm.compile(
                script,
                rustpython_vm::compiler::Mode::Exec,
                format!("{tag}.py"),
            ) {
                Ok(c) => c,
                Err(e) => {
                    // 语法错 → SyntaxError 异常对象（带文件/行列）→
                    // 同一 traceback 格式化（CPython 一致的诊断输出）
                    let exc = e.into_pyexception(vm, Some(script));
                    return Err(format!(
                        "脚本编译异常:\n{}",
                        format_traceback(vm, &exc)
                    ));
                }
            };
            if let Err(e) = vm.run_code_obj(code, scope.clone()) {
                return Err(format!("脚本执行异常:\n{}", format_traceback(vm, &e)));
            }
            // ── 入口定约（批次 59 归一化定稿，用户定夺）：**唯一形态
            // `async def main():`**（帧体 `await pygame.next_frame()`）
            // ——async 形态向下兼容纯同步写法（帧逻辑照常顺序写），
            // 向上承接将来 `await` 网络 IO 不卡帧（net awaitable 接
            // 帧桥同款机制）；生成器形态（def main + yield）退役。
            let frame_bridge = r#"
class _FrameAwait:
    def __await__(self):
        yield "frame"
_next_frame_state = _FrameAwait()
def next_frame():
    return _next_frame_state
"#;
            let bridge_code = vm
                .compile(
                    frame_bridge,
                    rustpython_vm::compiler::Mode::Exec,
                    "<frame_bridge>".to_string(),
                )
                .map_err(|e| format!("帧桥编译失败: {e}"))?;
            if let Err(e) = vm.run_code_obj(bridge_code, scope.clone()) {
                return Err(format!("帧桥注入失败: {e:?}"));
            }
            // pygame.next_frame 挂载（帧桥函数 → pygame 模块属性）
            match (vm.import("pygame", 0), scope.globals.get_item("next_frame", vm)) {
                (Ok(pg), Ok(nf)) => {
                    let mounted = pg.set_attr("next_frame", nf, vm);
                    eprintln!("[bridge-run] next_frame 挂载: {mounted:?}");
                }
                (pg, nf) => {
                    eprintln!("[bridge-run] 挂载失败 pg={pg:?} nf={nf:?}");
                }
            }
            let entry = scope.globals.get_item("main", vm).map_err(|_| {
                "入口缺失：脚本须定义 `async def main():`（帧体 await \
                 pygame.next_frame()）".to_string()
            })?;
            let genobj = entry.call((), vm).map_err(|e| {
                format!("入口 main() 调用异常:\n{}", format_traceback(vm, &e))
            })?;
            // 归一化校验：必须是协程（async def）——生成器形态指路
            let cls = genobj.class().name().to_string();
            if cls != "coroutine" {
                return Err(
                    "入口形态错误：main 须为 `async def main():`（生成器形态 \
                     def main + yield 已退役——把 yield 改为 await \
                     pygame.next_frame()）"
                        .to_string(),
                );
            }
            *gen_cell.borrow_mut() = Some(genobj);
            dlog(tag, "async main() 协程已建");
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
        if starfish_pygame::event::pump() {
            dlog(tag, "退出请求，结束");
            break;
        }

        let mut done = false;
        interp.enter(|vm| {
            let borrowed = gen_cell.borrow();
            let Some(entry_obj) = borrowed.as_ref() else {
                done = true;
                return;
            };
            // 协程步进（归一化后唯一形态）：send(None) 恢复——
            // await 帧桥挂起 = 本帧完成
            let step = vm
                .call_method(entry_obj, "send", (vm.ctx.none.clone(),))
                .map(|_| ());
            match step {
                Ok(()) => {} // 本帧逻辑完成（yield / await 帧桥挂起）
                Err(exc) => {
                    // 生成器/协程耗尽（StopIteration）= 正常收尾，非异常
                    let obj: rustpython_vm::PyObjectRef = exc.clone().into();
                    let exc_cls = obj.class().name().to_string();
                    if exc_cls == "StopIteration" {
                        dlog(tag, "脚本完成（入口耗尽）");
                    } else {
                        dlog(
                            tag,
                            &format!("脚本异常:\n{}", format_traceback(vm, &exc)),
                        );
                    }
                    done = true;
                }
            }
        });
        if done {
            break;
        }

        starfish_pygame::mixer::pump(); // Web 流式解码驱动（桌面 no-op）
        next_frame().await;
    }
    // 先关窗再卸载（RP 卸载期间窗口 ghost）；mixer 显式收口（TLS 析构期）
    display::quit();
    starfish_pygame::mixer::quit();
    dlog(tag, "main loop ended");
    Ok(())
}

/// 运行任意 Python 脚本（**pypg 脚本直跑形态**——与 [`run`] 的游戏门
/// 同构的完整驱动）。自建解释器后转交 [`run_script_in`]。
pub async fn run_script(script: &str, source_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    run_script_in(crate::rpy::new_interpreter(), script, source_name).await
}

/// [`run_script`] 的**解释器外置版**（pypg 用）：解释器由调用方装配并
/// 持有——genesis / sys.argv 接线等启动段可先行于建窗完成（控制台
/// 阶段先行，窗口出现即进入脚本执行）。
///
/// 流程：帧桥注入 → 顶层执行 → 入口自动探测（`async def main():` 协程
/// → 帧循环驱动；无 main / 同步 main / 非函数 main = 顶层执行完即收
/// ——python 语义：定义 ≠ 调用）→ 关窗 → mixer 收口。
/// 编译错 = SyntaxError 经典 traceback（与 [`run`] 同款诊断）；
/// 运行期异常（帧循环内）= 打印 traceback 后返回 Err（调用方据此置
/// 退出码 1，python 同语义）。
/// 首帧打点（"装配段→首帧"耗时）常开——启动诊断仪表；可见性不做
/// 任何刻意安排（2026-10-26 用户定夺：白窗已短暂，HIDDEN+首帧后显示
/// 的机制不跨平台，退役）。
pub async fn run_script_in(
    interp: rustpython_vm::Interpreter,
    script: &str,
    source_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let t0 = std::time::Instant::now();
    let err: RefCell<Option<String>> = RefCell::new(None);
    let gen_cell: RefCell<Option<rustpython_vm::PyObjectRef>> = RefCell::new(None);
    let errored = Cell::new(false);
    interp.enter(|vm| {
        let run = || -> Result<(), String> {
            let scope = vm.new_scope_with_builtins();
            crate::rpy::install_modules(vm);
            // 帧桥注入（与游戏门同款——脚本直跑形态同样支持 async
            // def main + await pygame.next_frame() 的协程游戏）
            let bridge_code = vm
                .compile(
                    r#"
class _FrameAwait:
    def __await__(self):
        yield "frame"
_next_frame_state = _FrameAwait()
def next_frame():
    return _next_frame_state
"#,
                    rustpython_vm::compiler::Mode::Exec,
                    "<run_script frame_bridge>".to_string(),
                )
                .map_err(|e| format!("帧桥编译失败: {e}"))?;
            if let Err(e) = vm.run_code_obj(bridge_code, scope.clone()) {
                return Err(format!("帧桥注入失败: {e:?}"));
            }
            match (vm.import("pygame", 0), scope.globals.get_item("next_frame", vm)) {
                (Ok(pg), Ok(nf)) => {
                    let mounted = pg.set_attr("next_frame", nf, vm);
                    eprintln!("[bridge] next_frame 挂载: {mounted:?}");
                }
                (pg, nf) => {
                    eprintln!("[bridge] 挂载失败 pg={pg:?} nf={nf:?}");
                }
            }
            let code = match vm.compile(
                script,
                rustpython_vm::compiler::Mode::Exec,
                source_name.to_string(),
            ) {
                Ok(c) => c,
                Err(e) => {
                    // 语法错 → SyntaxError 异常对象 → 经典 traceback
                    //（与游戏门 run 同款诊断；第二参 = 源文本（脱字符
                    // 定位行显示用），非文件名）
                    let exc = e.into_pyexception(vm, Some(script));
                    return Err(format!("脚本编译异常:\n{}", format_traceback(vm, &exc)));
                }
            };
            if let Err(e) = vm.run_code_obj(code, scope.clone()) {
                return Err(format!("脚本执行异常:\n{}", format_traceback(vm, &e)));
            }
            dlog(
                source_name,
                &format!("顶层执行完成（{}ms）", t0.elapsed().as_millis()),
            );
            // ── 入口自动探测：`async def main()` 协程 → 记入 gen_cell
            // 交外层驱动循环。同步 main 不代调、非函数名 main 忽略
            //（python 语义：定义 ≠ 调用）。
            if let Ok(main_f) = scope.globals.get_item("main", vm) {
                let is_fn = main_f
                    .downcast_ref::<rustpython_vm::builtins::PyFunction>()
                    .is_some();
                if is_fn {
                    match main_f.call((), vm) {
                        Ok(co) => {
                            if co.class().name().to_string() == "coroutine" {
                                *gen_cell.borrow_mut() = Some(co);
                            }
                        }
                        Err(e) => {
                            return Err(format!(
                                "入口 main() 调用异常:\n{}",
                                format_traceback(vm, &e)
                            ));
                        }
                    }
                }
            }
            Ok(())
        };
        if let Err(msg) = run() {
            *err.borrow_mut() = Some(msg);
        }
    });
    if let Some(msg) = err.into_inner() {
        // 关窗先于收口（与 run 同约）；消息交调用方打印（单路输出）
        display::quit();
        starfish_pygame::mixer::quit();
        return Err(msg.into());
    }
    dlog(
        source_name,
        &format!("装配段完成（+{}ms）——进入帧循环", t0.elapsed().as_millis()),
    );

    // ── 帧驱动循环（与 testhost::run 同构）：事件泵 → 协程 send 步进
    // → StopIteration 正常收尾 → 帧拍。首帧打点 = 启动诊断仪表。
    let mut first_frame = true;
    loop {
        let seg_pump = std::time::Instant::now();
        if starfish_pygame::event::pump() {
            dlog("pypg", "退出请求，结束");
            break;
        }
        let ms_pump = seg_pump.elapsed().as_millis();
        let seg_send = std::time::Instant::now();
        let mut done = false;
        interp.enter(|vm| {
            let borrowed = gen_cell.borrow();
            let Some(genobj) = borrowed.as_ref() else {
                done = true;
                return;
            };
            match vm.call_method(genobj, "send", (vm.ctx.none.clone(),)) {
                Ok(_) => {} // 本帧逻辑完成（await 帧桥挂起）
                Err(exc) => {
                    let obj: rustpython_vm::PyObjectRef = exc.clone().into();
                    let cls = obj.class().name().to_string();
                    if cls == "StopIteration" {
                        dlog("pypg", "main 协程完成");
                    } else {
                        errored.set(true);
                        dlog(
                            "pypg",
                            &format!("脚本异常:\n{}", format_traceback(vm, &exc)),
                        );
                    }
                    done = true;
                }
            }
        });
        if done {
            break;
        }
        starfish_pygame::mixer::pump(); // Web 流式解码驱动
        next_frame().await;
        if first_frame {
            first_frame = false;
            dlog(
                "pypg",
                &format!(
                    "首帧分段: pump={}ms send+收尾={}ms（累计 +{}ms）",
                    ms_pump,
                    seg_send.elapsed().as_millis(),
                    t0.elapsed().as_millis()
                ),
            );
        }
    }
    // 无协程（纯脚本形态）：顶层执行完即收（python 语义）
    // 先关窗再收口（与 run 同约——RP 活动期间窗口 ghost 防御）
    display::quit();
    starfish_pygame::mixer::quit();
    if errored.get() {
        return Err("脚本异常（traceback 见上）".into());
    }
    Ok(())
}

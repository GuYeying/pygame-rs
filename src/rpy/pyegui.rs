//! pygame.egui —— egui-starfish-we 原生风格绑定面（v1，2026-10-24）。
//!
//! 接口风格定约：**第三方库等价物，不 pygame 化**——OOP 门面
//! `t.draw_ui(panel)`（t = `with screen.render()` 的会话对象），panel 为
//! `pygame.egui.UiBase` 子类实例、实现 `draw(self, ui)`；Ui 为 egui `Ui`
//! 的直绘包装（方法名 = egui 原生：heading/label/button/checkbox/
//! slider_f32/…）；嵌套布局双形态：`ui.horizontal(cb)`（回调）与
//! `with ui.horizontal():`（区域对象——可读性优先的 Python 惯用形态）。
//!
//! 帧序映射（对齐 egui-starfish-we example/render_test 的实测帧序，
//! `t.draw_ui(panel)` 单调用承载两段）：
//! - 构建（CPU 侧）：`take_egui_input_we` → `ctx.run_ui`（透明
//!   CentralPanel + panel.draw(ui)）→ `tessellate`；
//! - 排程：整条 0.37 管线（update_texture → update_buffers → render →
//!   free + clear → platform_output）打包成闭包，经引擎会话级泛型
//!   `RenderPass::defer_overlay` 排程，flip/present 时在本帧全部游戏
//!   内容 pack 之后统一落屏——恒在最顶层、恒单次落屏，egui 是 HUD 的
//!   层级语义由结构保证而非脚本纪律。
//!
//! 事件侧（2026-10-24 修订）：init 不再自动挂监听——`egui.capture()`
//! **显式**开启原始事件镜像（pygame 翻译有损：KeyDown 不带修饰键等，
//! egui 快捷键需要完整词汇），`capture(False)` 撤除；不 capture 则 egui
//! 收不到任何输入（指针/键盘全失效）——捕获行为在脚本层可见。
//!
//! 结构体声明 + `#[pyclass] impl` 块在 `mods/egui.rs` 的 pymodule 树内
//! （RP 宏要求 struct 与方法收集块同宿；批次 43 拆分定约），本文件 =
//! 会话实现本体（EguiRuntime + draw_ui 三段/DeferredFrame/discard_output）。

use std::cell::RefCell;
use std::rc::Rc;

use egui_starfish_we::egui;
use rustpython_derive::pyclass;
use rustpython_vm::builtins::{PyBaseExceptionRef, PyStrRef};
use rustpython_vm::function::{ArgIntoFloat, FuncArgs};
use rustpython_vm::{PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine};

use crate::rpy::bridge;
use crate::rpy::mods::egui::{PyUi, PyUiRegion};

/// egui 运行时（跨帧持有：输入桥 State + 绘制端 Renderer）。
/// 主线程单例（ADR-6——egui Context 非 Send）；Rc 内壳供延迟叠加闭包
/// 在 present 时段取用（不跨线程，Box<dyn FnOnce> 无 Send 约束）。
struct EguiRuntime {
    ctx: egui::Context,
    state: egui_starfish_we::State,
    renderer: egui_starfish_we::SfEguiRenderer,
}

thread_local! {
    static EGUI: RefCell<Option<Rc<RefCell<EguiRuntime>>>> = const { RefCell::new(None) };
    /// 回调栈内的活跃 Ui 指针（None = 回调外——PyUi 方法此时一律报错，
    /// 防 Python 侧把 Ui 对象跨帧持有后踩悬垂指针）
    static CURRENT_UI: RefCell<Option<*mut egui::Ui>> = const { RefCell::new(None) };
    /// with 区打开中的子 Ui 栈（horizontal() 区域形态；嵌套天然成栈）
    static UI_REGIONS: RefCell<Vec<Box<egui::Ui>>> = const { RefCell::new(Vec::new()) };
    /// 区域世代（frame 收尾 +1——跨帧持 PyUiRegion 调 __exit__ 时代码不匹配，
    /// 干净报错而非悬垂解引用）
    static REGION_GEN: RefCell<usize> = const { RefCell::new(0) };
    /// 回调内 Python 异常暂存（run_ui 闭包无法携带 PyResult 出来）
    static EXC: RefCell<Option<PyBaseExceptionRef>> = const { RefCell::new(None) };
}

/// SAFETY（CURRENT_UI 指针模式）：指针只在回调执行期间置位、回调返回
/// 立即清空——解引用窗口与 Ui 生命周期严格重合；回调外调用 PyUi 方法
/// 走 `CURRENT_UI = None` 分支干净报错，永不触达悬垂指针。
/// 借用纪律：指针**拷出即还锁**（Option<*mut> Copy）——嵌套场景
/// （horizontal 内再画控件）不得持有 CURRENT_UI 的借用。
fn with_ui<T>(vm: &VirtualMachine, f: impl FnOnce(&mut egui::Ui) -> T) -> PyResult<T> {
    let ptr = CURRENT_UI.with(|u| *u.borrow());
    match ptr {
        Some(ptr) => {
            // SAFETY：见上方不变式——仅在置位窗口内解引用
            Ok(f(unsafe { &mut *ptr }))
        }
        None => Err(vm.new_type_error(
            "Ui 仅在 draw_ui 的面板 draw 回调栈内有效（勿跨帧持有 Ui 对象）",
        )),
    }
}

fn stash_exc(e: PyBaseExceptionRef) {
    EXC.with(|x| {
        let mut x = x.borrow_mut();
        if x.is_none() {
            *x = Some(e);
        }
    });
}

/// CJK 字体注入（复用 pygame 宿主预载链字节；写法对齐 fork 的
/// `load_system_cjk_fonts`——Proportional 首选 / Monospace 兜底）
fn inject_cjk(ctx: &egui::Context, bytes: Vec<u8>) {
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "cjk".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("cjk".to_owned());
    ctx.set_fonts(fonts);
}

// ── 模块函数本体（mods/egui.rs 的 #[pymodule] 面委托目标）──────────

/// init()：建 Context/输入桥/绘制端（幂等；须在 set_mode 之后——Renderer
/// 创建期依赖 GPU 与交换链格式）。事件输入走**逐条显式喂**（`egui.event(e)`)，
/// init 不做任何隐式挂接。
pub(crate) fn init(vm: &VirtualMachine) -> PyResult<()> {
    if !starfish_pygame::display::get_init() {
        return Err(vm
            .new_runtime_error("pygame.egui.init() 须在 pygame.display.set_mode 之后调用"));
    }
    if EGUI.with(|c| c.borrow().is_some()) {
        return Ok(()); // 幂等
    }
    let ctx = egui::Context::default();
    // CJK：优先复用 pygame 宿主三平台预载链（Windows msyh / Web cjk.ttf /
    // Android 系统候选链——批次 G 资产），miss 回退 fork 的系统探测（桌面）
    let mut cjk = false;
    if let Some(bytes) = starfish_pygame::font::preloaded("cjk") {
        inject_cjk(&ctx, bytes);
        cjk = true;
    } else {
        cjk = egui_starfish_we::load_system_cjk_fonts(&ctx);
    }
    let state = starfish_pygame::display::with_window(|w| {
        egui_starfish_we::State::new_we(ctx.clone(), egui::ViewportId::ROOT, None, w)
    })
    .ok_or_else(|| vm.new_runtime_error("窗口未就绪（display 未初始化）"))?;
    let gpu = starfish_pygame::render::gpu_info()
        .ok_or_else(|| vm.new_runtime_error("GPU 未就绪（set_mode 之后才可 init）"))?;
    let screen = starfish_pygame::display::get_screen();
    // MSAA 必须与交换链匹配（wgpu 对管线采样数与附件做严格校验）
    let renderer = egui_starfish_we::SfEguiRenderer::new(egui_wgpu::Renderer::new(
        &gpu.device,
        screen.color_format(),
        egui_wgpu::RendererOptions {
            msaa_samples: screen.sample_count(),
            ..egui_wgpu::RendererOptions::PREDICTABLE
        },
    ));
    EGUI.with(|c| {
        *c.borrow_mut() = Some(Rc::new(RefCell::new(EguiRuntime {
            ctx,
            state,
            renderer,
        })))
    });
    bridge::devlog(&format!("[egui] init OK（CJK={cjk}，输入 = egui.event(e) 逐条喂）"));
    Ok(())
}

/// event(e)：**逐条显式喂事件**（`pygame.event.get()` 循环内调用；不喂
/// 则 egui 无输入——捕获行为在数据流层完全可见）。pygame 事件 → 原始
/// 引擎事件**全保真重建**（PyEvent 载荷持有引擎枚举；`e.mod` KMOD 位 →
/// 完整修饰键——egui 快捷键可用），经 `on_we_event` 进入输入桥。
/// egui 无关变体（quit/touch/user/suspend 等）为 no-op。
pub(crate) fn event(
    ev: &crate::rpy::mods::event::PyEvent,
    vm: &VirtualMachine,
) -> PyResult<()> {
    let rc = EGUI.with(|c| c.borrow().as_ref().cloned());
    let Some(rc) = rc else {
        return Err(vm.new_runtime_error("pygame.egui 未初始化（先 init()）"));
    };
    let Some(we) = pygame_to_we(&ev.ev) else {
        return Ok(()); // egui 无关事件：静默
    };
    let mut rt = rc.borrow_mut();
    starfish_pygame::display::with_window(|w| rt.state.on_we_event(w, &we));
    Ok(())
}

/// pygame 事件 → 原始引擎事件（全保真重建；None = egui 无关变体）
fn pygame_to_we(ev: &starfish_pygame::event::Event) -> Option<starfish_we::Event> {
    use starfish_pygame::key;
    use starfish_we::Event as We;
    use starfish_we::MouseButton as Mb;
    Some(match ev {
        starfish_pygame::event::Event::Resized { width, height } => We::Resized {
            width: *width,
            height: *height,
        },
        starfish_pygame::event::Event::FocusGained => We::FocusGained,
        starfish_pygame::event::Event::FocusLost => We::FocusLost,
        starfish_pygame::event::Event::KeyDown { key, modifiers, repeat } => We::KeyDown {
            key: *key,
            modifiers: key::bits_to_mods(*modifiers),
            repeat: *repeat,
        },
        starfish_pygame::event::Event::KeyUp { key, modifiers } => We::KeyUp {
            key: *key,
            modifiers: key::bits_to_mods(*modifiers),
        },
        starfish_pygame::event::Event::TextInput { text } => We::TextInput {
            text: text.clone(),
        },
        starfish_pygame::event::Event::MouseMotion { x, y } => We::MouseMoved {
            x: *x as f64,
            y: *y as f64,
        },
        starfish_pygame::event::Event::MouseButtonDown { button, .. } => We::MouseDown {
            button: match *button {
                1 => Mb::Left,
                2 => Mb::Middle,
                3 => Mb::Right,
                4 => Mb::Back,
                5 => Mb::Forward,
                other => Mb::Other(other as u8),
            },
        },
        starfish_pygame::event::Event::MouseButtonUp { button, .. } => We::MouseUp {
            button: match *button {
                1 => Mb::Left,
                2 => Mb::Middle,
                3 => Mb::Right,
                4 => Mb::Back,
                5 => Mb::Forward,
                other => Mb::Other(other as u8),
            },
        },
        starfish_pygame::event::Event::MouseWheel { x, y } => We::MouseWheel {
            delta_x: *x as f64,
            delta_y: *y as f64,
        },
        // Quit/Touch/Suspend/Resume/User/Other：egui 无关（Touch 的鼠标
        // 合成已在引擎层完成，回喂会双份）
        _ => return None,
    })
}

/// UiBase 基类对象（sys.modules["pygame.egui"].UiBase——module_exec 注入
/// 的 Python 层基类；frame 的 isinstance 校验用）
fn uibase_class(vm: &VirtualMachine) -> PyResult<PyObjectRef> {
    let sys_modules = vm.sys_module.get_attr("modules", vm)?;
    let module = sys_modules.get_item("pygame.egui", vm)?;
    module.get_attr("UiBase", vm)
}

/// 构建（输入 → UI → 细分，CPU 侧）：panel 须为 UiBase 子类实例且实现
/// draw(self, ui)（OOP 门面——isinstance 强制，基类 NotImplementedError
/// 兜底"必须实现"；校验失败不消耗输入）；draw 内 Python 异常弃置产物
/// （对账后）并上抛。
fn build_frame(
    rc: &Rc<RefCell<EguiRuntime>>,
    panel: &PyObjectRef,
    vm: &VirtualMachine,
) -> PyResult<(egui::FullOutput, Vec<egui::epaint::ClippedPrimitive>)> {
    // isinstance 强制：UiBase 子类（鸭子类型不放行——基类契约即文档）
    let uibase = uibase_class(vm)?;
    if !panel.is_instance(&uibase, vm)? {
        return Err(vm.new_type_error(
            "draw_ui 须传 pygame.egui.UiBase 的子类实例（class HUD(UiBase) 并实现 draw(ui)）",
        ));
    }
    let draw = panel.get_attr("draw", vm)?;
    let raw = {
        let mut rt = rc.borrow_mut();
        starfish_pygame::display::with_window(|w| rt.state.take_egui_input_we(w))
            .ok_or_else(|| vm.new_runtime_error("窗口未就绪（display 未初始化）"))?
    };
    let full = rc.borrow().ctx.run_ui(raw, |ui| {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(egui::Color32::TRANSPARENT))
            .show(ui, |ui| {
                CURRENT_UI.with(|u| *u.borrow_mut() = Some(ui as *mut egui::Ui));
                let ui_obj: PyObjectRef = PyUi.into_ref(&vm.ctx).into();
                if let Err(e) = draw.call((ui_obj,), vm) {
                    stash_exc(e);
                }
                // 区域兜底回收：漏配对的 with 区（horizontal() 未进 with）
                // 就地丢弃（子 Ui 不回填父光标——回调已结束，布局收尾无碍）
                UI_REGIONS.with(|r| r.borrow_mut().clear());
                REGION_GEN.with(|g| *g.borrow_mut() += 1);
                CURRENT_UI.with(|u| *u.borrow_mut() = None);
            });
    });
    if let Some(e) = EXC.with(|x| x.borrow_mut().take()) {
        // 弃置产物先与渲染器对账再清空（egui 端已记账——直接丢弃会让
        // 后续部分更新命中"未分配纹理"panic；drop-panic 双保险）
        let mut rt = rc.borrow_mut();
        discard_output(&mut rt, full);
        return Err(e); // 回调异常：产物弃置（输入已消耗，下帧自愈）
    }
    let egui::FullOutput { platform_output, textures_delta, shapes, pixels_per_point, viewport_output } = full;
    let jobs = rc.borrow().ctx.tessellate(shapes, pixels_per_point);
    let output = egui::FullOutput {
        // shapes 已细分进 jobs——重建时留存空表即可
        platform_output,
        textures_delta,
        shapes: Vec::new(),
        pixels_per_point,
        viewport_output,
    };
    Ok((output, jobs))
}

/// 弃置产物与渲染器对账：egui 端已记账的 set/free 增量喂给 Renderer
/// （丢弃会让后续部分更新命中"未分配纹理"panic），随后清空 delta
/// 保证 drop 安全。用于构建错误弃置路径。
fn discard_output(rt: &mut EguiRuntime, mut output: egui::FullOutput) {
    if let Some(gpu) = starfish_pygame::render::gpu_info() {
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                rt.renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
            }
        }
        for id in &output.textures_delta.free {
            rt.renderer.free_texture(id);
        }
    }
    output.textures_delta.clear();
}

/// 延迟叠加载荷（defer 闭包的全部持有物）：Drop 兜底清 delta——闭包若
/// 因退出/异常从未执行，带未应用条目的 TexturesDelta 直接 drop 会 panic
/// （epaint 0.37 契约），Drop 先 clear 保证任意路径 drop 安全。
struct DeferredFrame {
    rc: Rc<RefCell<EguiRuntime>>,
    output: Option<egui::FullOutput>,
    jobs: Vec<egui::epaint::ClippedPrimitive>,
    desc: egui_wgpu::ScreenDescriptor,
}

impl Drop for DeferredFrame {
    fn drop(&mut self) {
        if let Some(mut out) = self.output.take() {
            out.textures_delta.clear();
        }
    }
}

/// draw_ui(panel)（`t.draw_ui(panel)` 的本体，t = `with screen.render()`
/// 的会话对象）：①会话探针（会话外调用先报错——不消耗输入、不建 UI）
/// ②构建（输入→UI→细分，CPU 侧，OOP 门面校验）③经活跃会话排程延迟
/// 叠加（引擎泛型 `RenderPass::defer_overlay`——零 egui 感知）。
/// 每帧一次；恒在游戏内容之上、恒单次落屏。
pub(crate) fn draw_ui(vm: &VirtualMachine, panel: PyObjectRef) -> PyResult<()> {
    let rc = EGUI.with(|c| c.borrow().as_ref().cloned());
    let Some(rc) = rc else {
        return Err(vm.new_runtime_error("pygame.egui 未初始化（先 init()）"));
    };
    // 会话探针先行（探针到排程之间同一同步调用栈、单线程——会话必然
    // 仍存活；panel.draw 回调无法触达会话收口）
    starfish_pygame::render::session::with_screen(|_| {}).map_err(|_| {
        vm.new_type_error("draw_ui 须在 `with screen.render():` 会话块内调用（t = 会话对象）")
    })?;
    let (output, jobs) = build_frame(&rc, &panel, vm)?;
    let (w, h) = starfish_pygame::display::with_window(|win| win.size()).unwrap_or((0, 0));
    let frame = DeferredFrame {
        rc,
        desc: egui_wgpu::ScreenDescriptor {
            size_in_pixels: [w, h],
            pixels_per_point: output.pixels_per_point,
        },
        output: Some(output),
        jobs,
    };
    starfish_pygame::render::session::with_screen(|pass| {
        pass.defer_overlay(move |octx| run_deferred(octx, frame));
    })
    .expect("会话探针刚确认存在（同栈同步，无中途收口路径）");
    Ok(())
}

/// 延迟叠加执行体（present 时段运行）：0.37 硬序 update_texture →
/// update_buffers → render，帧尾 free + clear + platform_output。
fn run_deferred(octx: &mut starfish_pygame::render::OverlayCtx, mut frame: DeferredFrame) {
    let Some(mut output) = frame.output.take() else {
        return;
    };
    let mut rt = frame.rc.borrow_mut();
    // 0.37 契约：先 update_texture，再 update_buffers，最后 render
    for (id, deltas) in &output.textures_delta.set {
        for delta in deltas {
            rt.renderer.update_texture(octx.device(), octx.queue(), *id, delta);
        }
    }
    // 通道前命令（uniform/回调 prepare）先于绘制通道提交
    let mut pre = octx.create_command_encoder();
    let mut cmds =
        rt.renderer.update_buffers(octx.device(), octx.queue(), &mut pre, &frame.jobs, &frame.desc);
    cmds.push(pre.finish());
    octx.submit_before(cmds);
    octx.with_pass("pygame.egui", false, |pass| {
        rt.renderer.render(pass, &frame.jobs, &frame.desc);
    });
    // 纹理退场 + delta 清空（不 clear 会 drop panic——0.37 实测坑）
    for id in &output.textures_delta.free {
        rt.renderer.free_texture(id);
    }
    output.textures_delta.clear();
    let egui::FullOutput { platform_output, .. } = output; // delta 已空，余字段安全 drop
    // 帧尾：光标可见性 / 剪贴板 / 链接
    starfish_pygame::display::with_window(|win| {
        rt.state.handle_platform_output_we(win, platform_output)
    });
}

/// wants_pointer() -> bool：egui 是否想要指针输入（游戏让位判断）
pub(crate) fn wants_pointer() -> bool {
    EGUI.with(|c| {
        c.borrow()
            .as_ref()
            .is_some_and(|rc| rc.borrow().ctx.egui_wants_pointer_input())
    })
}

/// wants_keyboard() -> bool：egui 是否想要键盘输入
pub(crate) fn wants_keyboard() -> bool {
    EGUI.with(|c| {
        c.borrow()
            .as_ref()
            .is_some_and(|rc| rc.borrow().ctx.egui_wants_keyboard_input())
    })
}

/// shutdown()：卸运行时（幂等；未执行的延迟排程由引擎丢弃，其 Drop
/// 兜底清 delta）
pub(crate) fn shutdown() {
    EGUI.with(|c| {
        c.borrow_mut().take();
    });
}

// ── with 区域（horizontal() 的无回调形态）────────────────────────

/// 开一个水平布局区（`with ui.horizontal():` 的入口）：建子 Ui 压栈、
/// CURRENT_UI 换到子 Ui；`__exit__` 回填父光标。返回区域对象供 with。
fn region_begin(vm: &VirtualMachine) -> PyResult<PyRef<PyUiRegion>> {
    let parent_ptr = CURRENT_UI.with(|u| *u.borrow()).ok_or_else(|| {
        vm.new_type_error("Ui 仅在 draw_ui 的面板 draw 回调栈内有效")
    })?;
    // SAFETY：parent_ptr 在置位窗口内（回调栈中），Ui 存活
    let child: egui::Ui = unsafe {
        (*parent_ptr).new_child(egui::UiBuilder::new().layout(egui::Layout::left_to_right(
            egui::Align::Center,
        )))
    };
    let mut boxed = Box::new(child);
    let child_ptr: *mut egui::Ui = &mut *boxed; // Box 入栈后堆址稳定，指针跨 move 有效
    UI_REGIONS.with(|r| r.borrow_mut().push(boxed));
    CURRENT_UI.with(|u| *u.borrow_mut() = Some(child_ptr));
    let epoch = REGION_GEN.with(|g| *g.borrow());
    Ok(PyUiRegion {
        parent: parent_ptr,
        epoch,
    }
    .into_ref(&vm.ctx))
}

/// 关区域（`__exit__`）：弹栈取子 Ui → 取 min_rect → 回填父光标。
/// 世代不匹配（跨帧持区域）= 干净报错，永不悬垂解引用。
fn region_end(region: &PyUiRegion, vm: &VirtualMachine) -> PyResult<()> {
    let epoch = REGION_GEN.with(|g| *g.borrow());
    if region.epoch != epoch {
        return Err(vm.new_type_error("horizontal 区已跨帧失效（勿跨帧持有区域对象）"));
    }
    let child = UI_REGIONS.with(|r| r.borrow_mut().pop());
    let Some(child) = child else {
        return Err(vm.new_type_error("horizontal 区已结束（__exit__ 重复调用）"));
    };
    let min_rect = child.min_rect();
    drop(child);
    // SAFETY：parent 是同帧内更早入栈的 Ui（回调栈中或外层区域框），
    // 世代匹配保证其仍存活
    unsafe { (*region.parent).advance_cursor_after_rect(min_rect) };
    CURRENT_UI.with(|u| *u.borrow_mut() = Some(region.parent));
    Ok(())
}

// ── PyUiRegion：with 布局区（__exit__ 回填父光标）────────────────

#[pyclass]
impl PyUiRegion {
    #[pymethod]
    fn __enter__(zelf: PyRef<Self>) -> PyRef<Self> {
        zelf
    }

    /// 收口：回填父光标（返回 False = 不抑制 with 块内异常）
    #[pymethod]
    fn __exit__(&self, _exc_info: FuncArgs, vm: &VirtualMachine) -> PyResult<bool> {
        region_end(self, vm)?;
        Ok(false)
    }
}

// ── PyUi：egui Ui 的即时模式包装（方法名 = egui 原生）────────────

fn as_str<'a>(s: &'a PyStrRef, vm: &VirtualMachine) -> PyResult<&'a str> {
    s.to_str()
        .ok_or_else(|| vm.new_type_error("文本含非 UTF-8 字符"))
}

#[pyclass]
impl PyUi {
    /// heading(text)：一级标题
    #[pymethod]
    fn heading(&self, text: PyStrRef, vm: &VirtualMachine) -> PyResult<()> {
        let t = as_str(&text, vm)?;
        with_ui(vm, |ui| {
            ui.heading(t);
        })
    }

    /// label(text)：文本标签
    #[pymethod]
    fn label(&self, text: PyStrRef, vm: &VirtualMachine) -> PyResult<()> {
        let t = as_str(&text, vm)?;
        with_ui(vm, |ui| {
            ui.label(t);
        })
    }

    /// button(text) -> bool：按钮（本帧被点击 = True）
    #[pymethod]
    fn button(&self, text: PyStrRef, vm: &VirtualMachine) -> PyResult<bool> {
        let t = as_str(&text, vm)?;
        with_ui(vm, |ui| ui.button(t).clicked())
    }

    /// separator()：分隔线
    #[pymethod]
    fn separator(&self, vm: &VirtualMachine) -> PyResult<()> {
        with_ui(vm, |ui| {
            ui.separator();
        })
    }

    /// checkbox(text, checked) -> bool：复选框（返回新状态）
    #[pymethod]
    fn checkbox(&self, text: PyStrRef, checked: bool, vm: &VirtualMachine) -> PyResult<bool> {
        let t = as_str(&text, vm)?;
        with_ui(vm, |ui| {
            let mut v = checked;
            ui.checkbox(&mut v, t);
            v
        })
    }

    /// slider_f32(text, value, min, max) -> float：滑条（返回新值）
    #[pymethod]
    fn slider_f32(
        &self,
        text: PyStrRef,
        value: ArgIntoFloat,
        min: ArgIntoFloat,
        max: ArgIntoFloat,
        vm: &VirtualMachine,
    ) -> PyResult<f64> {
        let t = as_str(&text, vm)?;
        with_ui(vm, |ui| {
            let mut v = value.into_float() as f32;
            ui.add(
                egui::Slider::new(
                    &mut v,
                    (min.into_float() as f32)..=(max.into_float() as f32),
                )
                .text(t),
            );
            v as f64
        })
    }

    /// add_space(px)：留白
    #[pymethod]
    fn add_space(&self, px: ArgIntoFloat, vm: &VirtualMachine) -> PyResult<()> {
        let px = px.into_float() as f32;
        with_ui(vm, |ui| {
            ui.add_space(px);
        })
    }

    /// horizontal(callback=None)：水平布局，双形态——
    /// 传回调 = egui 原生 `ui.horizontal(|ui| ...)`（返回 None）；
    /// 无参 = 返回区域对象，配 `with ui.horizontal():` 使用（可读性优先）。
    #[pymethod]
    fn horizontal(
        &self,
        args: crate::rpy::args::HorizontalArgs,
        vm: &VirtualMachine,
    ) -> PyResult<PyObjectRef> {
        let Some(cb) = args.cb else {
            let region = region_begin(vm)?;
            return Ok(region.into());
        };
        with_ui(vm, |ui| {
            ui.horizontal(|ui| {
                // 换指针前存前置值（嵌套返回后恢复外层 Ui）
                let prev = CURRENT_UI.with(|u| u.borrow_mut().replace(ui as *mut egui::Ui));
                let inner: PyObjectRef = PyUi.into_ref(&vm.ctx).into();
                if let Err(e) = cb.call((inner,), vm) {
                    stash_exc(e);
                }
                CURRENT_UI.with(|u| *u.borrow_mut() = prev);
            });
        })?;
        Ok(vm.ctx.none())
    }
}

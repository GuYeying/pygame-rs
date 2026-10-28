//! pygame_gui —— egui-starfish-we 原生风格绑定面（v1，2026-10-24；
//! 2026-10-24 架构抽象升格独立顶层模块，原 pygame.egui）。
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
use std::collections::HashMap;
use std::rc::Rc;

use egui_starfish_we::egui;
use rustpython_derive::pyclass;
use rustpython_vm::builtins::{PyBaseExceptionRef, PyStrRef};
use rustpython_vm::function::{ArgIntoFloat, FuncArgs};
use rustpython_vm::{PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine};

use crate::rpy::bridge;
use super::{PyUi, PyUiRegion};

/// 渲染器槽键：(目标颜色格式, MSAA 样本数)。egui 管线按附件格式 +
/// 样本数分套是 wgpu 硬约束——主屏 = 交换链格式（随平台漂移：Web/macOS
/// Bgra8、Windows Rgba8），Surface 离屏 = 恒 Rgba8Unorm；跨目标复用管线
/// = 附件校验 panic（Web pip_test 实测）。"管线按 (color_format,
/// sample_count) 分套"多窗口定约的叠加目标版。
type RendererKey = (egui_wgpu::wgpu::TextureFormat, u32);

/// egui 运行时（跨帧持有：输入桥 State + 绘制端 Renderer 槽）。
/// 主线程单例（ADR-6——egui Context 非 Send）；Rc 内壳供延迟叠加闭包
/// 在 present 时段取用（不跨线程，Box<dyn FnOnce> 无 Send 约束）。
struct EguiRuntime {
    ctx: egui::Context,
    state: egui_starfish_we::State,
    /// 绘制端按 [`RendererKey`] 分槽：主屏槽 init 期建，离屏槽首用懒建。
    /// 字体图集纹理增量在**全部活跃槽**间广播（egui 纹理 id 全局命名，
    /// 各槽持有独立上传副本——图集 MB 级，内存换正确性）。
    renderers: HashMap<RendererKey, egui_starfish_we::SfEguiRenderer>,
    /// **中间面**（渲染架构批次 B 全局相机对齐）：egui_wgpu 自带管线
    /// 与引擎定约不同构（NDC 直写无矩阵）——客人先画进面，引擎 sprite
    /// 管线 `blit_fullscreen` 合成回目标（全局相机 MVP 自然生效）。
    /// 键 = (槽键, 尺寸)：同槽不同尺寸各一张（画中画/分屏），尺寸变化
    /// 即重建（churn 可接受——面板场景尺寸恒定）。Arc 持有——借用纪律
    /// （渲染器可变借用与中间面取用并存）。
    mids: HashMap<(RendererKey, (u32, u32)), std::sync::Arc<starfish_pygame::render::Surface>>,
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
    // 渲染器恒单槽（渲染架构批次 B 中间面方案）：egui 只画进 Rgba8/1x
    // 中间面（离屏画布定约），引擎管线合成回各目标——目标格式/MSAA
    // 由合成段承载，egui 管线不再按目标分套（原"按交换链格式分槽"
    // 的必要性随直绘取消而消失）。
    let renderer_key: RendererKey = (egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, 1);
    let mut renderers = HashMap::new();
    renderers.insert(
        renderer_key,
        egui_starfish_we::SfEguiRenderer::new(egui_wgpu::Renderer::new(
            &gpu.device,
            renderer_key.0,
            egui_wgpu::RendererOptions {
                msaa_samples: renderer_key.1,
                ..Default::default()
            },
        )),
    );
    EGUI.with(|c| {
        *c.borrow_mut() = Some(Rc::new(RefCell::new(EguiRuntime {
            ctx,
            state,
            renderers,
            mids: HashMap::new(),
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
    ev: &crate::rpy::pygame::event::PyEvent,
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

/// UiBase 基类对象（sys.modules["pygame_gui"].UiBase——module_exec 注入
/// 的 Python 层基类；frame 的 isinstance 校验用）
fn uibase_class(vm: &VirtualMachine) -> PyResult<PyObjectRef> {
    let sys_modules = vm.sys_module.get_attr("modules", vm)?;
    let module = sys_modules.get_item("pygame_gui", vm)?;
    module.get_attr("UiBase", vm)
}

/// 构建（输入 → UI → 细分，CPU 侧）：panel 须为 UiBase 子类实例且实现
/// draw(self, ui)（OOP 门面——isinstance 强制，基类 NotImplementedError
/// 兜底"必须实现"；校验失败不消耗输入）；draw 内 Python 异常弃置产物
/// 构建（输入 → UI → 细分，CPU 侧）：panel 须为 UiBase 子类实例且实现
/// draw(self, ui)（OOP 门面——isinstance 强制，基类 NotImplementedError
/// 兜底"必须实现"；校验失败不消耗输入）；draw 内 Python 异常弃置产物
/// （对账后）并上抛。指针事件语义由**变体**在 raw 上预处理（Screen 直
/// 通 / Surface 剥除——见 `strip_pointer`）。
fn build_frame(
    rc: &Rc<RefCell<EguiRuntime>>,
    panel: &PyObjectRef,
    raw: egui::RawInput,
    vm: &VirtualMachine,
) -> PyResult<(egui::FullOutput, Vec<egui::epaint::ClippedPrimitive>)> {
    // isinstance 强制：UiBase 子类（鸭子类型不放行——基类契约即文档）
    let uibase = uibase_class(vm)?;
    if !panel.is_instance(&uibase, vm)? {
        return Err(vm.new_type_error(
            "draw_ui 须传 pygame_gui.UiBase 的子类实例（class HUD(UiBase) 并实现 draw(ui)）",
        ));
    }
    let draw = panel.get_attr("draw", vm)?;
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

/// 延迟帧（构建产物暂存——defer_overlay 排程时装箱，present/flush 时段
/// 执行）：持有 rc 引用计数 + 渲染描述 + egui 产物；Drop 兜底清 delta
/// （排程未执行也 drop 安全——纹理 id 与渲染器对账后置空）。
struct DeferredFrame {
    rc: std::rc::Rc<std::cell::RefCell<EguiRuntime>>,
    /// 渲染目标槽键（格式+样本数——run_deferred 据此选/懒建 Renderer）
    key: RendererKey,
    desc: egui_wgpu::ScreenDescriptor,
    /// **合成 MVP 快照**（排程时点的 `camera_mvp_for(目标尺寸)` 产出）：
    /// 帧内多次 set_camera 时主屏收口（present 帧末）的全局相机可能已
    /// 非排程时的值——快照锁定"draw_ui 时刻的相机"，A 相机画主屏 /
    /// B 相机画离屏各得其所（批次 53 补）。
    compose_mvp: [[f32; 4]; 4],
    output: Option<egui::FullOutput>,
    jobs: Vec<egui::epaint::ClippedPrimitive>,
}

impl Drop for DeferredFrame {
    fn drop(&mut self) {
        if let Some(mut out) = self.output.take() {
            out.textures_delta.clear();
        }
    }
}

/// 弃置产物与渲染器对账：egui 端已记账的 set/free 增量喂给**全部槽**
/// 的 Renderer（丢弃会让后续部分更新命中"未分配纹理"panic；字体增量
/// 全槽同步——各槽纹理 id 命名空间独立），随后清空 delta 保证 drop
/// 安全。用于构建错误弃置路径。
fn discard_output(rt: &mut EguiRuntime, mut output: egui::FullOutput) {
    if let Some(gpu) = starfish_pygame::render::gpu_info() {
        for r in rt.renderers.values_mut() {
            for (id, deltas) in &output.textures_delta.set {
                for delta in deltas {
                    r.update_texture(&gpu.device, &gpu.queue, *id, delta);
                }
            }
            for id in &output.textures_delta.free {
                r.free_texture(id);
            }
        }
    }
    output.textures_delta.clear();
}

/// 取原始输入（Screen/Surface 变体共用；窗口指针事件随 take 带出——
/// 剥离语义归各变体自决）。
fn take_raw_input(
    rc: &Rc<RefCell<EguiRuntime>>,
    vm: &VirtualMachine,
) -> PyResult<egui::RawInput> {
    let mut rt = rc.borrow_mut();
    starfish_pygame::display::with_window(|w| rt.state.take_egui_input_we(w))
        .ok_or_else(|| vm.new_runtime_error("窗口未就绪（display 未初始化）"))
}

/// 纯展示形态（Surface 目标专用）：剥窗口系指针事件——窗口指针坐标对
/// 表面无意义，面板显示完全由 Python 侧变量驱动（交互在游戏侧判定、
/// 传值不回写——变量驱动定约）。已知纯视觉边缘：egui 指针态随 Context
/// 跨帧共享（Surface/Screen 帧同 Context），Screen 帧指针位残留可能造
/// 成面板 hover 残影——不推 PointerGone 是**有意的**（会打断主屏进行中
/// 的拖拽）；完整隔离 = per-surface Context（字体图集翻倍），按需重开。
fn strip_pointer(mut raw: egui::RawInput) -> egui::RawInput {
    raw.events
        .retain(|ev| {
            !matches!(
                ev,
                egui::Event::PointerMoved(_)
                    | egui::Event::PointerButton { .. }
                    | egui::Event::Touch { .. }
            )
        });
    raw
}

/// draw_ui(panel)（`t.draw_ui(panel)` 的本体，t = `with screen.render()`
/// 的会话对象）：①会话探针（会话外调用先报错——不消耗输入、不建 UI）
/// ②构建（输入→UI→细分，CPU 侧，OOP 门面校验）③经活跃会话排程延迟
/// 叠加（引擎泛型 `RenderPass::defer_overlay`——零 egui 感知）。
/// 每帧一次；恒在游戏内容之上、恒单次落屏。**指针交互主目标**（窗口
/// 指针直通）。
pub(crate) fn draw_ui(vm: &VirtualMachine, panel: PyObjectRef) -> PyResult<()> {
    let rc = EGUI.with(|c| c.borrow().as_ref().cloned());
    let Some(rc) = rc else {
        return Err(vm.new_runtime_error("pygame_gui 未初始化（先 init()）"));
    };
    // 会话探针先行（探针到排程之间同一同步调用栈、单线程——会话必然
    // 仍存活；panel.draw 回调无法触达会话收口）
    starfish_pygame::render::session::with_screen(|_| {}).map_err(|_| {
        vm.new_type_error("draw_ui 须在 `with screen.render():` 会话块内调用（t = 会话对象）")
    })?;
    let screen = starfish_pygame::display::get_screen();
    // screen_rect = 逻辑画布（set_mode 语义尺寸）——egui 与游戏内容同一
    // 坐标系（Android 嵌入下物理恒原生窗口，取窗口会让布局与命中分家）
    let (w, h) = screen.logical_size();
    let mut raw = take_raw_input(&rc, vm)?;
    raw.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(w as f32, h as f32),
    ));
    let key: RendererKey = (screen.color_format(), screen.sample_count());
    let compose_mvp = screen.present_mvp().to_cols_array_2d();
    draw_ui_generic(vm, rc, panel, raw, key, compose_mvp, |boxed| {
        starfish_pygame::render::session::with_screen(|pass| {
            pass.defer_overlay(boxed);
        })
        .expect("会话探针刚确认存在（同栈同步，无中途收口路径）");
    })
}

/// Surface 变体（`t.draw_ui(panel)`，t = `with surf.render()` 会话对象）
/// ——排程到本面 flush 收口（batch pack 之后；画中画 / 游戏内面板）。
/// **egui 渲染目标 = 表面尺寸**（screen_rect 与 ScreenDescriptor 均按
/// 表面取——尺寸不一致 = 布局按窗口铺开 + 剪裁越界 wgpu 校验 panic，
/// 批次 48 实测）。**纯展示形态**：窗口指针事件剥除（变量驱动交互——
/// 见 `strip_pointer`）。
pub(crate) fn draw_ui_on_surface(
    vm: &VirtualMachine,
    panel: PyObjectRef,
    surface: &starfish_pygame::render::Surface,
) -> PyResult<()> {
    let rc = EGUI.with(|c| c.borrow().as_ref().cloned());
    let Some(rc) = rc else {
        return Err(vm.new_runtime_error("pygame_gui 未初始化（先 init()）"));
    };
    let mut raw = strip_pointer(take_raw_input(&rc, vm)?);
    // 离屏面无 DPI 概念：像素即逻辑点——ppp 恒 1.0（窗口 DPI 渗入 =
    // 布局/剪裁/字形全错位：Windows 175% 缩放下 ctx ppp=1.75 而物理
    // 尺寸被当逻辑点 → desc 错位 → 面板内容被裁空，实测）
    if let Some(vi) = raw.viewports.get_mut(&raw.viewport_id) {
        vi.native_pixels_per_point = Some(1.0);
    }
    raw.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(
            starfish_pygame::render::DrawTarget::size(surface).0 as f32,
            starfish_pygame::render::DrawTarget::size(surface).1 as f32,
        ),
    ));
    // 离屏目标恒 Rgba8Unorm 无 MSAA（Surface/Texture 纹理定约）——与
    // 主屏交换链格式（随平台漂移）分槽持管线
    let key: RendererKey = (egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, 1);
    let compose_mvp = starfish_pygame::render::camera_mvp_for(
        starfish_pygame::render::DrawTarget::size(surface),
    )
    .to_cols_array_2d();
    draw_ui_generic(vm, rc, panel, raw, key, compose_mvp, |boxed| {
        surface.defer_overlay(boxed)
    })
}

/// draw_ui 本体（调度泛化，批次 48）：构建 0.37 全管线帧 → 经 schedule
/// 排程。Screen/Surface 两目标的差异（指针语义 / screen_rect / 收口点）
/// 全在变体层的 raw 预处理与 schedule——本体零目标感知。
fn draw_ui_generic(
    vm: &VirtualMachine,
    rc: std::rc::Rc<std::cell::RefCell<EguiRuntime>>,
    panel: PyObjectRef,
    raw: egui::RawInput,
    key: RendererKey,
    compose_mvp: [[f32; 4]; 4],
    schedule: impl FnOnce(
        Box<dyn FnOnce(&mut starfish_pygame::render::OverlayCtx) + 'static>,
    ),
) -> PyResult<()> {
    let (w, h) = raw
        .screen_rect
        .map(|r| (r.width() as u32, r.height() as u32))
        .unwrap_or((0, 0));
    let (output, jobs) = build_frame(&rc, &panel, raw, vm)?;
    let frame = DeferredFrame {
        rc,
        key,
        desc: egui_wgpu::ScreenDescriptor {
            // 渲染目标像素尺寸 = 变体按目标设置的 screen_rect（Screen =
            // 窗口 / Surface = 表面——尺寸不一致 = egui 剪裁越界 panic）
            size_in_pixels: [w, h],
            pixels_per_point: output.pixels_per_point,
        },
        // 排程时点快照全局相机（同帧多次 set_camera 的时序正确性）
        compose_mvp,
        output: Some(output),
        jobs,
    };
    schedule(Box::new(move |octx| run_deferred(octx, frame)));
    Ok(())
}

/// 延迟叠加执行体（present/flush 时段运行）：0.37 硬序 update_texture →
/// update_buffers → render，帧尾 free + clear + platform_output。
/// 纹理增量喂**全部槽**（字体图集全槽同步——各槽 id 空间独立）；绘制
/// 用目标槽（无则懒建：离屏槽首用即装配）。
fn run_deferred(octx: &mut starfish_pygame::render::OverlayCtx, mut frame: DeferredFrame) {
    let Some(mut output) = frame.output.take() else {
        return;
    };
    let mut rt = frame.rc.borrow_mut();
    // 渲染器恒单槽（中间面 Rgba8/1x——见 init 注释；懒建兜底保持）
    let renderer_key: RendererKey = (egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, 1);
    if !rt.renderers.contains_key(&renderer_key) {
        rt.renderers.insert(
            renderer_key,
            egui_starfish_we::SfEguiRenderer::new(egui_wgpu::Renderer::new(
                octx.device(),
                renderer_key.0,
                egui_wgpu::RendererOptions {
                    msaa_samples: renderer_key.1,
                    ..Default::default()
                },
            )),
        );
    }
    // 0.37 契约：先 update_texture（全槽广播），再 update_buffers（目标
    // 槽），最后 render（目标槽）
    for r in rt.renderers.values_mut() {
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                r.update_texture(octx.device(), octx.queue(), *id, delta);
            }
        }
    }
    // **中间面两段**（渲染架构批次 B——全局相机对齐，零改 egui-wgpu）：
    // ① egui 自带管线画进中间面（Rgba8 无深度、清透明——原 NDC 公式
    //    在面上 1:1 成立，绘制行为与直绘目标完全等价）
    // ② 引擎 sprite 管线 blit_fullscreen 合成回目标——MVP =
    //    camera_mvp_for(目标尺寸)：Auto2d = 像素级 1:1 全屏（零回归），
    //    set_camera 后 egui 跟随全局相机（一处设置、全体生效）。
    // （中间面 ensure 先于 renderer 借用——同 rt 的两字段不冲突借用）
    let mid_size = (frame.desc.size_in_pixels[0], frame.desc.size_in_pixels[1]);
    let mid_key = (frame.key, mid_size);
    if !rt.mids.contains_key(&mid_key) {
        rt.mids.insert(
            mid_key,
            std::sync::Arc::new(starfish_pygame::render::Surface::new(mid_size)),
        );
    }
    let mid = rt.mids.get(&mid_key).expect("中间面刚 ensure").clone();
    let renderer = rt
        .renderers
        .get_mut(&renderer_key)
        .expect("渲染器单槽刚 ensure（懒建或 init 已建）");
    // 通道前命令（uniform/回调 prepare）先于绘制通道提交
    let mut pre = octx.create_command_encoder();
    let mut cmds =
        renderer.update_buffers(octx.device(), octx.queue(), &mut pre, &frame.jobs, &frame.desc);
    cmds.push(pre.finish());
    octx.submit_before(cmds);
    octx.pass_on_surface("pygame.egui.mid", &mid, egui_wgpu::wgpu::Color::TRANSPARENT, |pass| {
        renderer.render(pass, &frame.jobs, &frame.desc);
    });
    // 合成 MVP = 排程时点快照（帧内多次 set_camera 时序正确）
    octx.blit_fullscreen_with(&mid, frame.compose_mvp);
    // 纹理退场 + delta 清空（不 clear 会 drop panic——0.37 实测坑；
    // free 全槽广播）
    for r in rt.renderers.values_mut() {
        for id in &output.textures_delta.free {
            r.free_texture(id);
        }
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

    // ── 词汇扩充 v2（2026-10-24 批次 47：文本输入 / 选择 / 数值拖拽）──

    /// text_edit(text) -> str：单行文本框（返回本帧内容——每帧以传入值
    /// 重置，值状态由脚本侧保存）
    #[pymethod]
    fn text_edit(&self, text: PyStrRef, vm: &VirtualMachine) -> PyResult<String> {
        let t = as_str(&text, vm)?.to_owned();
        with_ui(vm, |ui| {
            let mut s = t.clone();
            ui.text_edit_singleline(&mut s);
            s
        })
    }

    /// radio_group(options, selected) -> int：单选组（返回本帧选中序号）
    #[pymethod]
    fn radio_group(
        &self,
        options: Vec<PyStrRef>,
        selected: usize,
        vm: &VirtualMachine,
    ) -> PyResult<usize> {
        let opts: Vec<String> = options
            .iter()
            .map(|o| as_str(o, vm).map(str::to_owned))
            .collect::<PyResult<_>>()?;
        with_ui(vm, |ui| {
            let mut sel = selected;
            for (i, o) in opts.iter().enumerate() {
                ui.radio_value(&mut sel, i, o.as_str());
            }
            sel
        })
    }

    /// selectable(text, selected) -> bool：可选行（本帧被点击 = True）
    #[pymethod]
    fn selectable(&self, text: PyStrRef, selected: bool, vm: &VirtualMachine) -> PyResult<bool> {
        let t = as_str(&text, vm)?;
        with_ui(vm, |ui| ui.selectable_label(selected, t).clicked())
    }

    /// progress_bar(fraction)：进度条（0..1 自动钳制；带百分比文本）
    #[pymethod]
    fn progress_bar(&self, fraction: ArgIntoFloat, vm: &VirtualMachine) -> PyResult<()> {
        let f = (fraction.into_float() as f32).clamp(0.0, 1.0);
        with_ui(vm, |ui| {
            ui.add(egui::ProgressBar::new(f).show_percentage());
        })
    }

    /// drag_f32(text, value, speed, min, max) -> float：拖拽（全位置传参；
    /// 数值（返回本帧值；text 作前缀标签）
    #[pymethod]
    fn drag_f32(
        &self,
        text: PyStrRef,
        value: ArgIntoFloat,
        speed: Option<ArgIntoFloat>,
        min: Option<ArgIntoFloat>,
        max: Option<ArgIntoFloat>,
        vm: &VirtualMachine,
    ) -> PyResult<f64> {
        let label = as_str(&text, vm)?.to_owned();
        let speed = speed.map(|s| s.into_float()).unwrap_or(1.0);
        let min = min.map(|m| m.into_float() as f32);
        let max = max.map(|m| m.into_float() as f32);
        with_ui(vm, |ui| {
            let mut v = value.into_float() as f32;
            let mut dv = egui::DragValue::new(&mut v)
                .speed(speed)
                .prefix(format!("{label}: "));
            if let (Some(lo), Some(hi)) = (min, max) {
                dv = dv.range(lo..=hi);
            }
            ui.add(dv);
            v as f64
        })
    }

    /// combo_box(label, options, selected) -> int：下拉选择（返回本帧选中
    /// 序号；同 label 的两个下拉共享 egui 内部 id——label 全局取唯一名）
    #[pymethod]
    fn combo_box(
        &self,
        label: PyStrRef,
        options: Vec<PyStrRef>,
        selected: usize,
        vm: &VirtualMachine,
    ) -> PyResult<usize> {
        let label = as_str(&label, vm)?.to_owned();
        let opts: Vec<String> = options
            .iter()
            .map(|o| as_str(o, vm).map(str::to_owned))
            .collect::<PyResult<_>>()?;
        with_ui(vm, |ui| {
            let mut sel = selected;
            egui::ComboBox::from_id_salt(format!("pygame_gui_combo_{label}"))
                .selected_text(opts.get(sel).map(String::as_str).unwrap_or(""))
                .show_ui(ui, |ui| {
                    for (i, o) in opts.iter().enumerate() {
                        ui.selectable_value(&mut sel, i, o.as_str());
                    }
                });
            sel
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

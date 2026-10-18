//! 引擎桥：Python 面（native_pygame/pygame_impls）触达 starfish 的
//! **RP 薄壳层**（批次 41 后零引擎状态——状态归 starfish 公共 API）。
//!
//! 剩余职责（全部因函数签名含 RP 类型而不可下沉引擎）：
//! - **错误映射**：`with_session` / `with_mrt_session`（引擎
//!   `NoActiveSession` → Python 异常）、`with_mixer`（AudioError →
//!   ValueError）——错误文案是 Python 面契约；
//! - **with_batch 收参壳**：blit_area / vertices / geometry /
//!   quad_corners——PySurface downcast、字符串白名单、indices 提取
//!   （RP 类型操作），数字核心已归 `DrawTarget` default method；
//! - **音频收参**：`sound_source_to_data`（None/str/bytes 三形态分派
//!   ——PyObjectRef 拆解）；
//! - **debug 设施**：[`devlog`]（三通道诊断日志——用户定约允许 bridge
//!   保有状态与 debug 代码）。
//!
//! 全部走 starfish 公共 API：零 starfish 反向感知。



//   回答你的问题：剩下的接口不是“还没搬完”，是类型边界本身

//   现在 bridge 里每一块的存在理由只有一个：函数签名含 RP 类型，物理上无法下沉 starfish。starfish 是纯引擎（零 Python 资产红线），不可能收 &VirtualMachine /
//   PyObjectRef / PyResult——这是单向红线的另一面。

//   ┌───────────────────────────┬────────────────────────┬────────────────────────────────────────────────────────────────────────────────────────────────┐
//   │          剩余块           │    签名里的 RP 类型    │                                           它在做什么                                           │
//   ├───────────────────────────┼────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ with_session /            │ &VirtualMachine →      │ 引擎说 Result<T, NoActiveSession>；Python 世界说 raise TypeError("draw 需在 with               │
//   │ with_mrt_session          │ PyResult               │ screen.render(): 会话内调用")——中间必须有人翻译，错误文案是 Python 面契约                      │
//   ├───────────────────────────┼────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ with_mixer                │ 同上                   │ AudioError → ValueError，10+ 处调用共享这一个映射点                                            │
//   ├───────────────────────────┼────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ blit_area / quad_corners  │ PyObjectRef downcast   │ PyObject → PySurface 的类型拆解 + Python 语义校验（"src 须为 pygame.Surface                    │
//   │                           │                        │ 实例”）——数字核心已在引擎                                                                      │
//   ├───────────────────────────┼────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ vertices / geometry       │ &VirtualMachine        │ "triangles" 字符串白名单（ValueError 文案）+ indices 从 Python 序列提取                        │
//   ├───────────────────────────┼────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ sound_source_to_data      │ Option<PyObjectRef>    │ None/str/bytes 三形态分派（try_bytes_like 是 RP API）                                          │
//   ├───────────────────────────┼────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ devlog                    │ 无                     │ debug 设施——你定约的“bridge 可以有自己的代码用于状态、debug"                                   │
//   └───────────────────────────┴────────────────────────┴────────────────────────────────────────────────────────────────────────────────────────────────┘

//   换个说法：starfish 止步于返回 Result 和枚举，bridge 止步于把 Result 翻译成 Python 异常、把 Python
//   对象拆成原始值。这条线两侧没有一方能替对方干活——再往下“减”就只能是把 RP 类型塞进 starfish（违反红线）或把 Python
//   错误文案硬编码进引擎（违反“行为一致”，Rust 用户会看到莫名其妙的中文 Python 错误）。

//   唯一理论上还能抠的是 quad_corners 里 4 行 tint 换算（其他三个的数字核心已经下沉了）——但它就是直通口本体，为一个函数给引擎加 API 不值得。剩下的 253
//   行已经贴着这条类型边界了。

use std::sync::Arc;

use rustpython_vm::convert::TryFromObject;
use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

use crate::rpy::args::{QuadCornersArgs, SfColor, SfPoint, SfRect};

use starfish::base::audio::{AudioMixer, SoundData};
use starfish::pygame::render::{BatchTopology, GeometryKind};
use starfish::pygame::render::RenderPass;

/// 诊断日志（println + console_log + 文件 + TCP；与宿主壳 dlog 同构）。
/// 文件路径与 binding_probe 的 LOG_PATH 保持一致。
/// Android TCP 排障通道仅 debug 构建生效（connect_timeout 300ms +
/// 一次失败即拉黑——无超时阻塞 connect 在主线程 = ANR，2026-09-28 实测坑）；
/// 目标机 IP 按当次真机环境改 [`DEVLOG_TCP_TARGET`]。
pub fn devlog(msg: &str) {
    println!("{}", msg);
    starfish::base::debug::console_log(msg);
    const LOG_PATH: &str = "/sdcard/Download/bp_log.txt";
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_PATH)
    {
        use std::io::Write;
        let _ = writeln!(f, "{}", msg);
    }
    #[cfg(all(target_os = "android", debug_assertions))]
    {
        use std::io::Write;
        use std::sync::atomic::{AtomicBool, Ordering};
        /// 真机 TCP 诊断接收端（开发机局域网地址，随排障现场改）
        const DEVLOG_TCP_TARGET: &str = "192.168.124.13:9000";
        static TCP_DEAD: AtomicBool = AtomicBool::new(false);
        if !TCP_DEAD.load(Ordering::Relaxed) {
            if let Ok(addr) = DEVLOG_TCP_TARGET.parse::<std::net::SocketAddr>() {
                match std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300))
                {
                    Ok(mut s) => {
                        let req = format!(
                            "POST /log HTTP/1.1\r\nHost: {DEVLOG_TCP_TARGET}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            msg.len(),
                            msg
                        );
                        let _ = s.write_all(req.as_bytes());
                        let _ = s.flush();
                    }
                    Err(_) => TCP_DEAD.store(true, Ordering::Relaxed),
                }
            }
        }
    }
}


/// MRT 会话内绘制路由（无活跃 MRT 会话报 ValueError）。
pub(crate) fn with_mrt_session<T>(
    vm: &VirtualMachine,
    f: impl FnOnce(&RenderPass<'static>) -> T,
) -> PyResult<T> {
    starfish::pygame::render::session::with_targets(f).map_err(|_| {
        vm.new_value_error("无活跃 MRT 会话（须在 `with pygame.targets([...])` 块内调用）")
    })
}

/// 在活跃会话上执行绘制（无会话 = 调用方用法错误，报类型错误）
pub(crate) fn with_session<T>(
    vm: &VirtualMachine,
    f: impl FnOnce(&RenderPass<'static>) -> T,
) -> PyResult<T> {
    starfish::pygame::render::session::with_screen(f).map_err(|_| {
        vm.new_type_error("draw 需在 `with screen.render():` 会话内调用（ADR-5）")
    })
}

// ── with_batch 直通口收参壳（低层几何：downcast/白名单校验归此，
//    数字核心归引擎 DrawTarget default method）────────────────────

use starfish::pygame::render::DrawTarget as BdTarget;

/// blit_area 收参壳：src 须为 pygame.Surface、area 为正（其余 UV 换算/
/// flush 语义归引擎 `DrawTarget::blit_area`）。
pub(crate) fn blit_area<T: BdTarget>(
    target: &T,
    src_obj: &PyObjectRef,
    dest: SfPoint,
    area: SfRect,
    color: Option<SfColor>,
    vm: &VirtualMachine,
) -> PyResult<starfish::pygame::Rect> {
    let src = src_obj
        .downcast_ref::<crate::rpy::native_pygame::PySurface>()
        .ok_or_else(|| vm.new_type_error("src 须为 pygame.Surface 实例"))?;
    if area.w <= 0 || area.h <= 0 || src.size().0 == 0 || src.size().1 == 0 {
        return Err(vm.new_value_error("area 尺寸须为正且源面非空"));
    }
    Ok(target.blit_area(
        &src.surface,
        (dest.0, dest.1),
        starfish::pygame::Rect::new(area.x, area.y, area.w, area.h),
        color.map(|c| c.0).unwrap_or([255, 255, 255, 255]),
    ))
}

/// vertices 收参壳：topo 白名单映射（RP 错误面；点集构建归引擎
/// `DrawTarget::push_points`）。
pub(crate) fn vertices<T: BdTarget>(
    target: &T,
    args: &crate::rpy::args::VerticesArgs,
    vm: &VirtualMachine,
) -> PyResult<()> {
    let topo = match args.topo.as_str() {
        "triangles" => BatchTopology::Triangles,
        "lines" => BatchTopology::Lines,
        other => {
            return Err(vm.new_value_error(format!(
                "topo 须为 'triangles' 或 'lines'：{other:?}"
            )))
        }
    };
    target.push_points(
        topo,
        None,
        &args.points.0,
        args.color.map(|c| c.0).unwrap_or([255, 255, 255, 255]),
    );
    Ok(())
}

/// geometry 收参壳：kind 白名单映射 + indices 收参（RP 错误面；
/// 顶点/索引几何构建归引擎 `DrawTarget::push_points_indexed`）。
pub(crate) fn geometry<T: BdTarget>(
    target: &T,
    args: &crate::rpy::args::GeometryArgs,
    vm: &VirtualMachine,
) -> PyResult<()> {
    let kind = match args.kind.as_str() {
        "filled" => GeometryKind::Filled,
        "line" => GeometryKind::Line,
        other => {
            return Err(vm.new_value_error(format!(
                "kind 须为 'filled' 或 'line'：{other:?}"
            )))
        }
    };
    let indices: Vec<u16> = match &args.indices {
        Some(o) => vm.extract_elements_with(o, |v| u16::try_from_object(vm, v.clone()))?,
        None => Vec::new(),
    };
    target.push_points_indexed(
        kind,
        &args.points.0,
        &indices,
        args.color.map(|c| c.0).unwrap_or([255, 255, 255, 255]),
    );
    Ok(())
}

/// quad_corners 主体：任意四角贴图进目标 batch（corners/uvs 各 4 组，
/// uv 归一 0..1；顺序 TL/TR/BR/BL）。
pub(crate) fn quad_corners<T: BdTarget>(
    target: &T,
    args: QuadCornersArgs,
    vm: &VirtualMachine,
) -> PyResult<()> {
    let src = args
        .texture
        .downcast_ref::<crate::rpy::native_pygame::PySurface>()
        .ok_or_else(|| vm.new_type_error("texture 须为 pygame.Surface 实例"))?;
    if args.corners.0.len() != 4 || args.uvs.0.len() != 4 {
        return Err(vm.new_value_error(format!(
            "corners/uvs 各须 4 组点，实际 {}/{}",
            args.corners.0.len(),
            args.uvs.0.len()
        )));
    }
    let pt = |p: [(f32, f32); 4]| -> [[f32; 2]; 4] {
        [p[0].into(), p[1].into(), p[2].into(), p[3].into()]
    };
    let corners = pt(args.corners.0.try_into().unwrap());
    let uvs = pt(args.uvs.0.try_into().unwrap());
    let raw = args.color.0;
    let tint = [
        raw[0] as f32 / 255.0,
        raw[1] as f32 / 255.0,
        raw[2] as f32 / 255.0,
        raw[3] as f32 / 255.0,
    ];
    target.with_batch(|b| {
        b.push_quad_corners(corners, uvs, tint, Some(src.surface.texture()))
    });
    Ok(())
}

// ── 音频桥薄壳（混音器生命周期/Web 手势门/内嵌提示音已归
//    starfish pygame::mixer——本层只剩 RP 错误映射与收参）────────

/// 访问混音器（引擎侧惰性建 8 通道；AudioError → ValueError 映射）
pub(crate) fn with_mixer<R>(
    vm: &VirtualMachine,
    f: impl FnOnce(&mut AudioMixer) -> R,
) -> PyResult<R> {
    starfish::pygame::mixer::with(f)
        .map_err(|e| vm.new_value_error(format!("音频设备初始化失败: {e:?}")))
}

/// 音频源对象 → SoundData（None = 内嵌提示音 / str = 文件路径 /
/// bytes = 内存解码；OGG/MP3/FLAC/WAV 按内容探测）
pub(crate) fn sound_source_to_data(
    source: Option<PyObjectRef>,
    vm: &VirtualMachine,
) -> PyResult<Arc<SoundData>> {
    match source {
        // Font(None) 惯例延续：无参 = 内嵌标准提示音
        None => Ok(starfish::pygame::mixer::beep()),
        Some(o) => {
            if let Some(s) = o.downcast_ref::<rustpython_vm::builtins::PyStr>() {
                let t = s
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("音频路径含非 UTF-8 字符"))?;
                SoundData::from_file(t)
            } else {
                // bytes / bytearray / memoryview（try_bytes_like 对非字节对象
                // 自报 TypeError）
                o.try_bytes_like(vm, |b| SoundData::from_bytes(b.to_vec()))?
            }
        }
        .map(Arc::new)
        .map_err(|e| vm.new_value_error(format!("音频错误: {e:?}")))
    }
}


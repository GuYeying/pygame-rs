//! 收参层：Python 参数 → Rust 值的唯一整形点。
//!
//! 两组内容（消费方 = 原生 pygame 模块的 pymethod；批次 41 后 bridge 只剩
//! 收参壳调用，探针/旧全局已退役）：
//! - [`SfColor`]/[`SfRect`]/[`SfPoint`]/[`SfPoints`]——参数类型
//!   （TryFromObject，pymethod 与自由函数同一条 bind_for 路径）；
//! - `Draw*Args` / `FillArgs`——draw 族的 FromArgs 收参结构体（pymethod 与
//!   自由函数皆可直接收整个结构体）。
//!
//! 定约：
//! - 元素一律 **i32 严格 int**（float 拒收，RP 原生报错）；
//! - 颜色分量 0..=255，越界中文 TypeError；`(r, g, b)` 自动补 alpha=255；
//! - 序列形态宽收（tuple / list / 任意可迭代），固定长度在类型层校验；
//! - 批次 C 的 Rect pyclass 落地后，[`SfRect`] / [`SfPoint`] 在此增加
//!   PyRef 直通分支（"直通句柄"终态）。（2026-10-24 修订：Rect 类型退役，
//!   SfRect 直通分支移除——纯序列收参。）

use rustpython_derive::FromArgs;
use rustpython_vm::convert::TryFromObject;
use rustpython_vm::function::ArgIntoFloat;
use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

// ── 提取辅助 ─────────────────────────────────────────────────────

/// 任意可迭代 → `Vec<i32>`（元素严格 int，非 int 由 RP 原生报错）。
fn seq_i32(vm: &VirtualMachine, obj: PyObjectRef) -> PyResult<Vec<i32>> {
    vm.extract_elements_with(&obj, |o| i32::try_from_object(vm, o))
}

/// 固定长度校验（`what` = 期望形态的中文描述，统一报错口径）。
fn expect_len(vm: &VirtualMachine, items: &[i32], n: usize, what: &str) -> PyResult<()> {
    if items.len() != n {
        return Err(vm.new_type_error(format!(
            "{what} 长度必须为 {n}，实际 {} 个分量",
            items.len()
        )));
    }
    Ok(())
}

// ── 参数类型 ─────────────────────────────────────────────────────

/// 颜色 `(r, g, b)` 或 `(r, g, b, a)` → RGBA（缺 alpha 补 255）。
#[derive(Debug, Clone, Copy)]
pub struct SfColor(pub [u8; 4]);

impl TryFromObject for SfColor {
    fn try_from_object(vm: &VirtualMachine, obj: PyObjectRef) -> PyResult<Self> {
        // 直通句柄分支：pygame.Color 实例 → 零转换拷贝（批次三十九）
        if let Some(c) = obj.downcast_ref::<crate::rpy::pygame::pygame::PyColor>() {
            let c = c.get();
            return Ok(Self([c.r, c.g, c.b, c.a]));
        }
        let items = seq_i32(vm, obj)?;
        let rgba: [i32; 4] = match items.as_slice() {
            [r, g, b] => [*r, *g, *b, 255],
            [r, g, b, a] => [*r, *g, *b, *a],
            _ => {
                return Err(vm.new_type_error(format!(
                    "颜色须为 (r, g, b) 或 (r, g, b, a)，实际 {} 个分量",
                    items.len()
                )))
            }
        };
        for c in rgba {
            if !(0..=255).contains(&c) {
                return Err(vm.new_type_error(format!("颜色分量越界（0..=255）：{c}")));
            }
        }
        Ok(Self([rgba[0] as u8, rgba[1] as u8, rgba[2] as u8, rgba[3] as u8]))
    }
}

/// 矩形 `(x, y, w, h)`（序列形态；Rect 类型已退役——2026-10-24 定约）。
#[derive(Debug, Clone, Copy)]
pub struct SfRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl TryFromObject for SfRect {
    fn try_from_object(vm: &VirtualMachine, obj: PyObjectRef) -> PyResult<Self> {
        let items = seq_i32(vm, obj)?;
        expect_len(vm, &items, 4, "rect (x, y, w, h)")?;
        let [x, y, w, h] = items.try_into().unwrap(); // 长度已验
        Ok(Self { x, y, w, h })
    }
}

/// 平面点 `(x, y)`。
#[derive(Debug, Clone, Copy)]
pub struct SfPoint(pub i32, pub i32);

impl TryFromObject for SfPoint {
    fn try_from_object(vm: &VirtualMachine, obj: PyObjectRef) -> PyResult<Self> {
        let items = seq_i32(vm, obj)?;
        expect_len(vm, &items, 2, "点 (x, y)")?;
        let [x, y] = items.try_into().unwrap(); // 长度已验
        Ok(Self(x, y))
    }
}

/// 点列：可迭代的 `(x, y)` 序列（pygame draw.polygon 的 points 形态；
/// 替代 shim 时代"扁平化 `[x,y,...]` + `chunks(2)` 组对"的旧 hack）。
#[derive(Debug, Clone)]
pub struct SfPoints(pub Vec<(i32, i32)>);

impl TryFromObject for SfPoints {
    fn try_from_object(vm: &VirtualMachine, obj: PyObjectRef) -> PyResult<Self> {
        let pts = vm.extract_elements_with(&obj, |o| {
            SfPoint::try_from_object(vm, o).map(|p| (p.0, p.1))
        })?;
        Ok(Self(pts))
    }
}

/// 平面点（f32 宽收——int/float 均收；角点/亚像素坐标用）
#[derive(Debug, Clone, Copy)]
pub struct SfFPoint(pub f32, pub f32);

impl TryFromObject for SfFPoint {
    fn try_from_object(vm: &VirtualMachine, obj: PyObjectRef) -> PyResult<Self> {
        use rustpython_vm::function::ArgIntoFloat;
        let items: Vec<f64> = vm.extract_elements_with(&obj, |o| {
            ArgIntoFloat::try_from_object(vm, o).map(|v| v.into_float())
        })?;
        if items.len() != 2 {
            return Err(vm.new_type_error(format!(
                "点 (x, y) 长度必须为 2，实际 {} 个分量",
                items.len()
            )));
        }
        Ok(Self(items[0] as f32, items[1] as f32))
    }
}

/// f32 点列（quad 角点/uv 固定 4 组；vertices/geometry 顶点列任意长度
/// ——长度在会话方法层校验）
#[derive(Debug, Clone)]
pub struct SfFPoints(pub Vec<(f32, f32)>);

/// vertices 收参（topo = "triangles"/"lines"）
#[derive(FromArgs)]
pub(crate) struct VerticesArgs {
    #[pyarg(any)]
    pub(crate) topo: String,
    #[pyarg(any)]
    pub(crate) points: SfFPoints,
    /// 统一颜色（缺省白）
    #[pyarg(any, optional)]
    pub(crate) color: Option<SfColor>,
}

/// geometry 收参（kind = "filled"/"line"；indices 空 = 逐 3 顶点一切三角）
#[derive(FromArgs)]
pub(crate) struct GeometryArgs {
    #[pyarg(any)]
    pub(crate) kind: String,
    #[pyarg(any)]
    pub(crate) points: SfFPoints,
    /// u16 索引（空 = 逐 3 顶点一切三角）
    #[pyarg(any, optional)]
    pub(crate) indices: Option<PyObjectRef>,
    #[pyarg(any, optional)]
    pub(crate) color: Option<SfColor>,
}

// ── pygame.egui 收参（批次 42）──────────────────────────────────

/// ui.horizontal(callback=None)：callback 缺省 None（无参 = with 区域
/// 形态，返回 UiRegion；传回调 = egui 原生闭包形态）
#[derive(FromArgs)]
pub(crate) struct HorizontalArgs {
    #[pyarg(any, optional)]
    pub(crate) cb: Option<PyObjectRef>,
}

impl TryFromObject for SfFPoints {
    fn try_from_object(vm: &VirtualMachine, obj: PyObjectRef) -> PyResult<Self> {
        let pts = vm.extract_elements_with(&obj, |o| {
            SfFPoint::try_from_object(vm, o).map(|p| (p.0, p.1))
        })?;
        Ok(Self(pts))
    }
}

/// `blit_area(src, dest, area, color=None)`：with_batch UV 子区域贴图
/// （Session/SurfaceSession/TargetsSession 三会话同签名）
#[derive(FromArgs)]
pub(crate) struct BlitAreaArgs {
    #[pyarg(any)]
    pub(crate) src: PyObjectRef,
    #[pyarg(any)]
    pub(crate) dest: SfPoint,
    /// 源面像素坐标子区域 (x, y, w, h)
    #[pyarg(any)]
    pub(crate) area: SfRect,
    /// 调色（缺省白 = 原样贴出）
    #[pyarg(any, optional)]
    pub(crate) color: Option<SfColor>,
}

/// `quad(texture, corners, uvs, color)`：任意四角贴图（with_batch
/// push_quad_corners 直通；角点/uv 各 4 组，uv 归一 0..1）
#[derive(FromArgs)]
pub(crate) struct QuadCornersArgs {
    #[pyarg(any)]
    pub(crate) texture: PyObjectRef,
    #[pyarg(any)]
    pub(crate) corners: SfFPoints,
    #[pyarg(any)]
    pub(crate) uvs: SfFPoints,
    #[pyarg(any)]
    pub(crate) color: SfColor,
}

// ── draw 族收参结构体（签名契约源 = bridge.rs 的注册名）─────────

#[derive(FromArgs)]
pub(crate) struct DrawRectArgs {
    #[pyarg(any)]
    pub(crate) color: SfColor,
    #[pyarg(any)]
    pub(crate) rect: SfRect,
    /// 缺省 0 = 填充（pygame draw.rect 惯例）
    #[pyarg(any, optional)]
    pub(crate) width: Option<i32>,
}

#[derive(FromArgs)]
pub(crate) struct DrawCircleArgs {
    #[pyarg(any)]
    pub(crate) color: SfColor,
    #[pyarg(any)]
    pub(crate) center: SfPoint,
    #[pyarg(any)]
    pub(crate) radius: i32,
    /// 缺省 0 = 填充（pygame draw.circle 惯例）
    #[pyarg(any, optional)]
    pub(crate) width: Option<i32>,
}

#[derive(FromArgs)]
pub(crate) struct DrawLineArgs {
    #[pyarg(any)]
    pub(crate) color: SfColor,
    #[pyarg(any)]
    pub(crate) start: SfPoint,
    #[pyarg(any)]
    pub(crate) end: SfPoint,
    /// 缺省 1（pygame draw.line 惯例）。width 已全程透传至 starfish；
    /// starfish v1 的 line 渲染恒 1px（`draw.rs` `let _ = width`），
    /// 加粗随其后续版本落地
    #[pyarg(any, optional)]
    pub(crate) width: Option<i32>,
}

#[derive(FromArgs)]
pub(crate) struct DrawPolygonArgs {
    #[pyarg(any)]
    pub(crate) color: SfColor,
    #[pyarg(any)]
    pub(crate) points: SfPoints,
    /// 缺省 0 = 填充（pygame draw.polygon 惯例）
    #[pyarg(any, optional)]
    pub(crate) width: Option<i32>,
}

#[derive(FromArgs)]
pub(crate) struct DrawEllipseArgs {
    #[pyarg(any)]
    pub(crate) color: SfColor,
    #[pyarg(any)]
    pub(crate) rect: SfRect,
    /// 缺省 0 = 填充（pygame draw.ellipse 惯例）
    #[pyarg(any, optional)]
    pub(crate) width: Option<i32>,
}

#[derive(FromArgs)]
pub(crate) struct DrawArcArgs {
    #[pyarg(any)]
    pub(crate) color: SfColor,
    #[pyarg(any)]
    pub(crate) rect: SfRect,
    /// 起始角（**弧度制**，0 = +x 方向——pygame draw.arc 语义）
    #[pyarg(any)]
    pub(crate) start_angle: ArgIntoFloat,
    /// 终止角（弧度制，向正角方向扫到）
    #[pyarg(any)]
    pub(crate) stop_angle: ArgIntoFloat,
    /// v1 恒 1px（starfish arc 描边简化，宽度收下不生效）；缺省 1
    #[pyarg(any, optional)]
    pub(crate) width: Option<i32>,
}

#[derive(FromArgs)]
pub(crate) struct FillArgs {
    #[pyarg(any)]
    pub(crate) color: SfColor,
}

/// `pygame.Surface((w, h), depth_buffer=False)` 收参（绑定定约
/// 架构 §九.2：**depth_buffer=** 表达深度缓冲——真 pygame 的 `depth=`
/// 是每像素位数语义，禁用；缺省 = 仅 color 面）
#[derive(FromArgs)]
pub(crate) struct SurfaceArgs {
    #[pyarg(any)]
    pub(crate) size: SfPoint,
    #[pyarg(any, optional)]
    pub(crate) depth_buffer: Option<bool>,
}

/// `pygame.mask.Mask((w, h), fill=False)` 收参
#[derive(FromArgs)]
pub(crate) struct MaskArgs {
    #[pyarg(any)]
    pub(crate) size: SfPoint,
    #[pyarg(any, optional)]
    pub(crate) fill: Option<bool>,
}

#[derive(FromArgs)]
pub(crate) struct MaskSetAtArgs {
    #[pyarg(any)]
    pub(crate) pos: SfPoint,
    /// 缺省 True（pygame set_at 惯例）
    #[pyarg(any, optional)]
    pub(crate) value: Option<bool>,
}

#[derive(FromArgs)]
pub(crate) struct MaskFillArgs {
    #[pyarg(any, optional)]
    pub(crate) value: Option<bool>,
}

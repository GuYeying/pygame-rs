//! pygame.render 绑定：渲染配置口（render.init 愿望）+ 全局相机
//!（set_camera / reset_camera / camera_mvp + Camera 构造）。
//!
//! 定约（渲染架构批次五十二）：相机全局唯一——Screen/Surface 无相机
//! 字段，pack 按全局 CameraState 解 MVP（Auto = 按目标尺寸隐式；
//! Manual = set_camera override，一处设置全体生效）。render.init =
//! set_mode 前愿望（dim / fov / 剔除 / 混合细粒度；后设优先，Gpu
//! 装配时消费一次）；pygame.init(dim=) 为其引擎级简写。Camera 构造
//! 经模块函数（camera_ortho / camera_perspective——pygame.math 的
//! mat4_* 同款先例）。

//! pygame.render 绑定：渲染配置口（render.init 愿望）+ 全局相机
//!（set_camera / reset_camera / camera_mvp + Camera 构造）+ **底层资源
//! 两入口**（ResourceAccess 建资源 + 主屏 RenderSurface 查询）与
//! raw_render 直绘会话（批次 C/D）。
//!
//! 批次 C/D（暴露面收缩定约）：**wgpu 裸对象（Device/Queue/Buffer/
//! TextureView）零出 Python 界**——自建管线走 ResourceAccess 包装
//!（shader/buffer/mesh/bind_group/pipeline 安全收参：布局预设、枚举
//! 字符串）；自定义管线遵循**全局相机定约**（group0/b0 = 64B 列主
//! mat4 MVP——`write_camera_mvp(buf)` 帧内写入）即自动吃全局相机；
//! `with screen.raw_render() as p:` 直绘会话（命令记录 → 目标提交
//! 时点经 OverlayCtx 回放，恒在游戏内容之上）。

use rustpython_derive::pymodule;

#[pymodule]
pub(crate) mod render {
    use rustpython_derive::{pyclass, FromArgs};
    use rustpython_vm::convert::TryFromObject;
    use rustpython_vm::{PyPayload, PyObjectRef, PyRef, PyResult, VirtualMachine};

    use crate::rpy::pygame::math::math::PyMat4;
    use crate::rpy::pygame::pygame::PySurface;

    use starfish_pygame::render as sf;
    use starfish_render::pipeline::{CullMode, DepthMode};
    use starfish_render::render_resource_access::RenderResourceAccess;
    use std::sync::Arc;

    // ── 绑定层工具（Debug 兜底宏 / kwargs 可选参数解析）──────────

    /// pyclass Debug 兜底（透明句柄——不泄内部类型）
    macro_rules! pydbg {
        ($t:ty, $s:expr) => {
            impl std::fmt::Debug for $t {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str($s)
                }
            }
        };
    }

    /// kwargs 可选参数解析（FuncArgs 手动形态——RustPython pyfunction
    /// 的裸 Option 参数不自动可选，坑位记录：可选参数一律 kwargs）
    fn kw_bool(
        args: &rustpython_vm::function::FuncArgs,
        vm: &VirtualMachine,
        key: &str,
    ) -> Option<bool> {
        use rustpython_vm::convert::TryFromBorrowedObject;
        args.kwargs
            .get(key)
            .and_then(|o| bool::try_from_borrowed_object(vm, o).ok())
    }

    /// kwargs 整数解析（同 kw_bool 形态）
    fn kw_int(
        args: &rustpython_vm::function::FuncArgs,
        vm: &VirtualMachine,
        key: &str,
    ) -> Option<i64> {
        use rustpython_vm::convert::TryFromBorrowedObject;
        args.kwargs
            .get(key)
            .and_then(|o| i64::try_from_borrowed_object(vm, o).ok())
    }

    /// kwargs 浮点解析（int/float 均收）
    fn kw_f64(
        args: &rustpython_vm::function::FuncArgs,
        vm: &VirtualMachine,
        key: &str,
    ) -> Option<f64> {
        args.kwargs.get(key).and_then(|o| {
            rustpython_vm::function::ArgIntoFloat::try_from_object(vm, o.clone())
                .ok()
                .map(|f| f.into_float())
        })
    }

    /// kwargs 字符串解析
    fn kw_str(
        args: &rustpython_vm::function::FuncArgs,
        _vm: &VirtualMachine,
        key: &str,
    ) -> Option<String> {
        args.kwargs.get(key).and_then(|o| {
            o.downcast_ref::<rustpython_vm::builtins::PyStr>()
                .and_then(|s| s.to_str())
                .map(|s| s.to_owned())
        })
    }

    // ── Camera：全局相机的 Python 形态（构造经模块函数）──────────

    /// 相机（视图 + 投影；mvp() 唯一上传量）。camera_ortho /
    /// camera_perspective 构造；直呼不可实例化。
    #[derive(Clone, Copy, Debug, PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "Camera", unhashable = true)]
    pub(crate) struct PyCamera(pub(crate) sf::Camera);

    #[pyclass]
    impl PyCamera {
        /// 替换视图矩阵（3D 位姿：look_at 等 Mat4；2D 恒 Identity）。
        #[pymethod]
        fn with_view(zelf: PyRef<Self>, view: PyRef<PyMat4>, vm: &VirtualMachine) -> PyRef<Self> {
            Self(zelf.0.with_view(view.0.get())).into_ref(&vm.ctx)
        }

        /// MVP（projection × view——唯一上传量；调试/教学口）。
        #[pymethod]
        fn mvp(&self, vm: &VirtualMachine) -> PyRef<PyMat4> {
            PyMat4(std::cell::Cell::new(self.0.mvp())).into_ref(&vm.ctx)
        }
    }

    // ── 相机构造（模块函数）─────────────────────────────────────

    /// 2D 正交相机：像素坐标直入（(0,0) 左上 / (w,h) 右下，y 向下）。
    #[pyfunction]
    fn camera_ortho(width: f32, height: f32, vm: &VirtualMachine) -> PyRef<PyCamera> {
        PyCamera(sf::Camera::ortho(width, height)).into_ref(&vm.ctx)
    }

    /// 3D 透视相机（fov 度数制、右手系、看向 -Z；位姿接 .with_view）。
    #[pyfunction]
    fn camera_perspective(
        fov_degrees: f32,
        aspect: f32,
        near: f32,
        far: f32,
        vm: &VirtualMachine,
    ) -> PyRef<PyCamera> {
        PyCamera(sf::Camera::perspective(fov_degrees, aspect, near, far)).into_ref(&vm.ctx)
    }

    // ── 全局相机（一处设置、全体生效；须 display.set_mode 后）────

    /// 全局相机 override（后续全部渲染生效直到 reset_camera）。
    #[pyfunction]
    fn set_default_camera(camera: PyRef<PyCamera>) {
        sf::set_default_camera(&camera.0);
    }

    /// 当前全局 MVP（Auto 状态按主屏尺寸现算——resize 天然跟随）。
    #[pyfunction]
    fn camera_mvp(vm: &VirtualMachine) -> PyRef<PyMat4> {
        PyMat4(std::cell::Cell::new(sf::camera_mvp())).into_ref(&vm.ctx)
    }

    /// 呈现拟合矩阵（NDC 域 clip 空间仿射）——**自管相机的 letterbox
    /// 对齐口**：write_buffer 直写矩阵的自绘管线，引擎无法拦截组合，
    /// 脚本自行左乘即与主绘制同一映射
    ///（`present_fit().mul_mat4(自管相机.mvp())`）。桌面/全屏恒等，
    /// Android 窗口化把自绘内容收进居中画布
    #[pyfunction]
    fn present_fit(vm: &VirtualMachine) -> PyRef<PyMat4> {
        PyMat4(std::cell::Cell::new(
            starfish_pygame::display::get_screen().present_fit(),
        ))
        .into_ref(&vm.ctx)
    }

    /// 当前相机状态（"auto2d" | "auto3d" | "manual"）。
    #[pyfunction]
    fn camera_state() -> String {
        match sf::camera_state() {
            sf::CameraState::Auto2d => "auto2d".into(),
            sf::CameraState::Auto3d { .. } => "auto3d".into(),
            sf::CameraState::Manual(_) => "manual".into(),
        }
    }

    // ── render.init：渲染愿望（set_mode 前配置，Gpu 装配消费一次）──

    /// init(dim=None, fov=None, near=None, far=None, cull=None,
    /// blend=None)：渲染管线细粒度愿望——**部分覆盖**（未传保留现值，
    /// 后设优先）。
    /// - dim：2 | 3（3 = Auto3d 透视默认相机；默认 2）
    /// - fov/near/far：Auto3d 相机参数（fov 度数制）
    /// - cull："none" | "back" | "front"（默认 none——镜像矩阵绕序
    ///   天然安全；开剔除后镜像 quad 背面消失属物理正确）
    /// - blend："alpha" | "opaque" | "premultiplied"（默认 alpha）
    #[pyfunction]
    fn init(
        args: rustpython_vm::function::FuncArgs,
        vm: &VirtualMachine,
    ) -> PyResult<()> {
        let dim = kw_int(&args, vm, "dim");
        let fov = kw_f64(&args, vm, "fov");
        let near = kw_f64(&args, vm, "near");
        let far = kw_f64(&args, vm, "far");
        let cull = kw_str(&args, vm, "cull");
        let blend = kw_str(&args, vm, "blend");
        let mut wish = sf::current_wish();
        if let Some(d) = dim {
            if d != 2 && d != 3 {
                return Err(vm.new_value_error(format!("dim 须为 2 或 3，实际 {d}")));
            }
            wish.dim3d = d == 3;
        }
        if let Some(v) = fov {
            wish.fov_deg = v as f32;
        }
        if let Some(v) = near {
            wish.near = v as f32;
        }
        if let Some(v) = far {
            wish.far = v as f32;
        }
        if let Some(c) = &cull {
            wish.cull = match c.as_str() {
                "none" => CullMode::None,
                "back" => CullMode::Back,
                "front" => CullMode::Front,
                other => {
                    return Err(vm.new_value_error(format!(
                        "cull 须为 none/back/front，实际 {other:?}"
                    )))
                }
            };
        }
        if let Some(b) = &blend {
            wish.blend = match b.as_str() {
                "alpha" => sf::SpriteBlend::Alpha,
                "opaque" => sf::SpriteBlend::Opaque,
                "premultiplied" => sf::SpriteBlend::Premultiplied,
                other => {
                    return Err(vm.new_value_error(format!(
                        "blend 须为 alpha/opaque/premultiplied，实际 {other:?}"
                    )))
                }
            };
        }
        sf::store_wish(wish);
        // GPU 槽已建立（宿主壳预置 set_mode 的 pygame-rs 形态）且相机
        // 维被改：即时生效——reset_camera 即"回到当前愿望"。cull/blend
        // 为管线编译期定型，不追溯（下次 set_mode 生效）。
        if dim.is_some() || fov.is_some() || near.is_some() || far.is_some() {
            if sf::gpu_info().is_some() {
                sf::reset_camera();
            }
        }
        Ok(())
    }

    // ══════════════════════════════════════════════════════════════
    // 批次 C：底层资源两入口（ResourceAccess 建资源 + 主屏查询）
    // ══════════════════════════════════════════════════════════════

    /// 资源访问（**两入口之一**——自建管线的建造工厂）。经
    /// `pygame.render.access()` 取：主屏版（默认颜色格式 = 交换链格式
    ///——平台漂移：Web/macOS Bgra8、Windows Rgba8）；offscreen=True
    /// 为离屏版（恒 Rgba8——管线与目标格式严格配对，wgpu 硬约束）。
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "ResourceAccess", unhashable = true)]
    pub(crate) struct PyResourceAccess {
        access: Arc<RenderResourceAccess>,
    }

    /// access(offscreen=False) -> ResourceAccess（须 set_mode 后；
    /// offscreen 为 kwargs 可选）
    #[pyfunction]
    fn access(
        args: rustpython_vm::function::FuncArgs,
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<PyResourceAccess>> {
        let gpu = sf::gpu_info()
            .ok_or_else(|| vm.new_runtime_error("GPU 未就绪（须先 pygame.display.set_mode）"))?;
        let access = if kw_bool(&args, vm, "offscreen").unwrap_or(false) {
            gpu.offscreen_access
        } else {
            gpu.access
        };
        Ok(PyResourceAccess { access }.into_ref(&vm.ctx))
    }

    // ── FromArgs 收参结构（可选参数规范形态——裸 Option 不自动可选坑）──

    /// access.shader(wgsl, label=None)
    #[derive(FromArgs)]
    pub(crate) struct ShaderArgs {
        #[pyarg(any)]
        wgsl: String,
        #[pyarg(any, optional)]
        label: Option<String>,
    }

    /// access.buffer(size, label=None)
    #[derive(FromArgs)]
    pub(crate) struct BufferArgs {
        #[pyarg(any)]
        size: u64,
        #[pyarg(any, optional)]
        label: Option<String>,
        /// usage = "uniform"（默认）| "indirect" | "storage"
        #[pyarg(any, optional)]
        usage: Option<String>,
    }

    /// access.update_mesh(mesh, vertices, indices=None)
    #[derive(FromArgs)]
    pub(crate) struct UpdateMeshArgs {
        #[pyarg(any)]
        mesh: PyRef<PyMesh>,
        #[pyarg(any)]
        vertices: PyObjectRef,
        #[pyarg(any, optional)]
        indices: Option<PyObjectRef>,
    }

    /// access.mesh(layout, vertices, indices=None, label=None)
    #[derive(FromArgs)]
    pub(crate) struct MeshArgs {
        #[pyarg(any)]
        layout: String,
        #[pyarg(any)]
        vertices: PyObjectRef,
        #[pyarg(any, optional)]
        indices: Option<PyObjectRef>,
        #[pyarg(any, optional)]
        label: Option<String>,
    }

    /// access.bind_group(bindings, label=None)
    #[derive(FromArgs)]
    pub(crate) struct BindGroupArgs {
        #[pyarg(any)]
        bindings: PyObjectRef,
        #[pyarg(any, optional)]
        label: Option<String>,
    }

    /// access.pipeline(shader, mesh, bind_groups, flavor="2d",
    /// topo="triangles", cull="none", depth=False, label=None)
    #[derive(FromArgs)]
    pub(crate) struct PipelineArgs {
        #[pyarg(any)]
        shader: PyRef<PyShader>,
        #[pyarg(any)]
        mesh: PyRef<PyMesh>,
        #[pyarg(any)]
        bind_groups: PyObjectRef,
        /// flavor = "2d"（默认，builder_2d）| "3d"（builder_3d）——builder
        /// 家族选择（topo/cull/depth 仍可逐项覆盖）
        #[pyarg(any, optional)]
        flavor: Option<String>,
        #[pyarg(any, optional)]
        topo: Option<String>,
        #[pyarg(any, optional)]
        cull: Option<String>,
        #[pyarg(any, optional)]
        depth: Option<bool>,
        #[pyarg(any, optional)]
        label: Option<String>,
    }

    /// access.compute_pipeline(shader, bind_groups, entry="cs_main",
    /// label=None)
    #[derive(FromArgs)]
    pub(crate) struct ComputePipelineArgs {
        #[pyarg(any)]
        shader: PyRef<PyShader>,
        #[pyarg(any)]
        bind_groups: PyObjectRef,
        #[pyarg(any, optional)]
        entry: Option<String>,
        #[pyarg(any, optional)]
        label: Option<String>,
    }

    /// access.sampler(mag="linear", min="linear", mipmap="linear",
    /// address="clamp", label=None)——四组字符串安全收参；address 同时
    /// 应用 u/v/w
    #[derive(FromArgs)]
    pub(crate) struct SamplerArgs {
        #[pyarg(any, optional)]
        mag: Option<String>,
        #[pyarg(any, optional)]
        min: Option<String>,
        #[pyarg(any, optional)]
        mipmap: Option<String>,
        #[pyarg(any, optional)]
        address: Option<String>,
        #[pyarg(any, optional)]
        label: Option<String>,
    }

    /// access.texture(size=(w,h), rgba=None, kind="sampled", label=None)
    #[derive(FromArgs)]
    pub(crate) struct TextureArgs {
        #[pyarg(any)]
        size: PyObjectRef,
        /// RGBA8 字节列表（w×h×4，0..=255）；None = 零填充
        #[pyarg(any, optional)]
        rgba: Option<PyObjectRef>,
        #[pyarg(any, optional)]
        kind: Option<String>,
        #[pyarg(any, optional)]
        label: Option<String>,
    }

    /// f32 列表 → LE bytes（顶点/uniform 打包共用）
    fn floats_to_bytes(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<u8>> {
        let vals: Vec<f64> = vm.extract_elements_with(obj, |o| {
            f64::try_from_object(vm, o.to_owned())
        })?;
        Ok(vals
            .iter()
            .flat_map(|v| (*v as f32).to_le_bytes())
            .collect())
    }

    /// 顶点布局预设 → wgpu VertexFormat 列表（安全收参：不暴露 wgpu 枚举）
    fn parse_layout(name: &str) -> Option<Vec<wgpu::VertexFormat>> {
        use wgpu::VertexFormat::*;
        Some(match name {
            "pos3" => vec![Float32x3],
            "pos3uv2" => vec![Float32x3, Float32x2],
            "pos3color4" => vec![Float32x3, Float32x4],
            // SpriteVertex 同布局（引擎批顶点——互操作）
            "pos3uv2color4" => vec![Float32x3, Float32x2, Float32x4],
            _ => return None,
        })
    }

    /// 布局预设的每顶点分量数（vertices 列表打包校验用）
    fn layout_stride_floats(name: &str) -> usize {
        match name {
            "pos3" => 3,
            "pos3uv2" => 5,
            "pos3color4" => 7,
            "pos3uv2color4" => 9,
            _ => 0,
        }
    }

    /// (w, h) 二元组提取（texture size 收参）
    fn parse_size_2d(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<(u32, u32)> {
        let elems: Vec<PyObjectRef> = vm.extract_elements_with(obj, |o| Ok(o.to_owned()))?;
        if elems.len() != 2 {
            return Err(vm.new_value_error("size 须为 (宽, 高) 二元组"));
        }
        let w = u32::try_from_object(vm, elems[0].clone())
            .map_err(|_| vm.new_type_error("宽须为非负整数"))?;
        let h = u32::try_from_object(vm, elems[1].clone())
            .map_err(|_| vm.new_type_error("高须为非负整数"))?;
        Ok((w, h))
    }

    fn parse_filter(s: &str) -> Option<wgpu::FilterMode> {
        match s {
            "linear" => Some(wgpu::FilterMode::Linear),
            "nearest" => Some(wgpu::FilterMode::Nearest),
            _ => None,
        }
    }

    fn parse_mipmap_filter(s: &str) -> Option<wgpu::MipmapFilterMode> {
        match s {
            "linear" => Some(wgpu::MipmapFilterMode::Linear),
            "nearest" => Some(wgpu::MipmapFilterMode::Nearest),
            _ => None,
        }
    }

    fn parse_address(s: &str) -> Option<wgpu::AddressMode> {
        match s {
            "clamp" => Some(wgpu::AddressMode::ClampToEdge),
            "repeat" => Some(wgpu::AddressMode::Repeat),
            "mirror" => Some(wgpu::AddressMode::MirrorRepeat),
            "border" => Some(wgpu::AddressMode::ClampToBorder),
            _ => None,
        }
    }

    #[pyclass]
    impl PyResourceAccess {
        /// shader(wgsl, label=None) -> Shader：WGSL 源码编译
        #[pymethod]
        fn shader(&self, args: ShaderArgs, vm: &VirtualMachine) -> PyResult<PyRef<PyShader>> {
            let m = self
                .access
                .shader_module_builder(args.wgsl)
                .build(args.label.as_deref());
            Ok(PyShader { inner: m }.into_ref(&vm.ctx))
        }

        /// buffer(size, label=None) -> Buffer：raw uniform 缓冲（字节自管
        /// ——自定义 struct 语义；相机定约 64B）。write 经本类 write_buffer。
        #[pymethod]
        fn buffer(&self, args: BufferArgs, vm: &VirtualMachine) -> PyResult<PyRef<PyBuffer>> {
            let usage = match args.usage.as_deref().unwrap_or("uniform") {
                "uniform" => wgpu::BufferUsages::UNIFORM,
                "indirect" => wgpu::BufferUsages::INDIRECT,
                "storage" => wgpu::BufferUsages::STORAGE,
                other => {
                    return Err(vm.new_value_error(format!(
                        "usage 须为 uniform/indirect/storage，实际 {other:?}"
                    )))
                }
            } | wgpu::BufferUsages::COPY_DST;
            let b = self.access.create_raw_buffer(
                args.label.as_deref().or(Some("py_render_buffer")),
                args.size,
                usage,
            );
            Ok(PyBuffer { inner: b, size: args.size }.into_ref(&vm.ctx))
        }

        /// mesh(layout, vertices, indices=None, label=None) -> Mesh：
        /// layout = 布局预设（pos3/pos3uv2/pos3color4/pos3uv2color4）；
        /// vertices = 扁平数值列表（按布局 stride 校验）；indices = u16
        /// 索引列表（可选）。
        #[pymethod]
        fn mesh(&self, args: MeshArgs, vm: &VirtualMachine) -> PyResult<PyRef<PyMesh>> {
            let formats = parse_layout(&args.layout).ok_or_else(|| {
                vm.new_value_error(format!(
                    "layout 须为 pos3/pos3uv2/pos3color4/pos3uv2color4，实际 {:?}",
                    args.layout
                ))
            })?;
            let n: Vec<f64> = vm.extract_elements_with(&args.vertices, |o| {
                f64::try_from_object(vm, o.to_owned())
            })?;
            let stride = layout_stride_floats(&args.layout);
            if stride == 0 || n.len() % stride != 0 {
                return Err(vm.new_value_error(format!(
                    "vertices 数量 {} 须为布局 stride {stride} 的整数倍",
                    n.len()
                )));
            }
            let bytes: Vec<u8> = n.iter().flat_map(|v| (*v as f32).to_le_bytes()).collect();
            // 顶点恒带 COPY_DST（update_mesh 的 write_buffer 硬要求——
            // 动态几何是 mesh 常规用法，绑定层默认开启免用户踩坑）
            let mut b = self
                .access
                .mesh_builder(formats, bytes)
                .with_vertex_usages(wgpu::BufferUsages::COPY_DST);
            if let Some(idx) = args.indices {
                let idx: Vec<i64> =
                    vm.extract_elements_with(&idx, |o| i64::try_from_object(vm, o.to_owned()))?;
                let mut u16s = Vec::with_capacity(idx.len());
                for i in idx {
                    u16s.push(u16::try_from(i).map_err(|_| {
                        vm.new_value_error(format!("索引越界（0..=65535）：{i}"))
                    })?);
                }
                b = b
                    .with_short_indices(u16s)
                    .with_index_usages(wgpu::BufferUsages::COPY_DST);
            }
            let m = b.build(args.label.as_deref(), None);
            Ok(PyMesh { inner: m }.into_ref(&vm.ctx))
        }

        /// bind_group(bindings, label=None) -> BindGroup：bindings =
        /// [(slot, kind, obj), ...]——kind "uniform"（obj=Buffer）/
        /// "texture"（obj=pygame.Surface，取其纹理视图）/"sampler"
        ///（obj=None，默认线性 clamp）。
        #[pymethod]
        fn bind_group(
            &self,
            args: BindGroupArgs,
            vm: &VirtualMachine,
        ) -> PyResult<PyRef<PyBindGroup>> {
            let bindings = args.bindings;
            let items: Vec<PyObjectRef> =
                vm.extract_elements_with(&bindings, |o| Ok(o.to_owned()))?;
            let mut b = self.access.bind_group_builder();
            for it in items {
                let elems: Vec<PyObjectRef> =
                    vm.extract_elements_with(&it, |o| Ok(o.to_owned()))?;
                if elems.len() != 3 {
                    return Err(vm.new_value_error("bindings 元素须为 (slot, kind, obj) 三元组"));
                }
                let slot = u32::try_from_object(vm, elems[0].clone())
                    .map_err(|_| vm.new_type_error("slot 须为整数"))?;
                let kind = elems[1]
                    .downcast_ref::<rustpython_vm::builtins::PyStr>()
                    .ok_or_else(|| vm.new_type_error("kind 须为字符串"))?
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("kind 含非 UTF-8 字符"))?
                    .to_owned();
                let obj = elems[2].clone();
                match kind.as_str() {
                    "uniform" => {
                        let buf = obj
                            .downcast_ref::<PyBuffer>()
                            .ok_or_else(|| vm.new_type_error("\"uniform\" 绑定 obj 须为 Buffer"))?;
                        b = b.uniform_raw(slot, buf.inner.clone(), buf.size);
                    }
                    "texture" => {
                        // pygame.Surface（面纹理）或 Texture（access.texture
                        // 产物——自建/计算可写纹理同口）
                        if let Some(surf) = obj.downcast_ref::<PySurface>() {
                            b = b.texture_view(slot, surf.surface.texture_view());
                        } else if let Some(tex) = obj.downcast_ref::<PyTexture>() {
                            b = b.texture_view(slot, tex.default_view());
                        } else {
                            return Err(vm.new_type_error(
                                "\"texture\" 绑定 obj 须为 pygame.Surface 或 Texture",
                            ));
                        }
                    }
                    "sampler" => {
                        // PySampler（access.sampler 产物）或 None（默认线性
                        // clamp——便捷形态）
                        let sp = obj.downcast_ref::<PySampler>();
                        let is_none =
                            obj.downcast_ref::<rustpython_vm::builtins::PyNone>().is_some();
                        let s = match (sp, is_none) {
                            (Some(sp), _) => sp.inner.clone(),
                            (None, true) => Arc::new(self.access.create_sampler(
                                "py_render_sampler",
                                &starfish_render::sampler_desc::SamplerDescriptor::linear_clamp(),
                            )),
                            _ => {
                                return Err(vm.new_type_error(
                                    "\"sampler\" 绑定 obj 须为 Sampler 或 None",
                                ))
                            }
                        };
                        b = b.sampler(slot, s);
                    }
                    "storage" | "storage_rw" => {
                        // 只读存储（顶点/片元/计算三态可见）与可写存储
                        //（仅计算段）——obj 须为 usage="storage" 的 Buffer
                        let buf = obj.downcast_ref::<PyBuffer>().ok_or_else(|| {
                            vm.new_type_error(format!(
                                "{kind:?} 绑定 obj 须为 Buffer（usage=\"storage\" 创建）"
                            ))
                        })?;
                        b = b.storage_raw(slot, buf.inner.clone(), kind == "storage");
                    }
                    other => {
                        return Err(vm.new_value_error(format!(
                            "kind 须为 uniform/texture/sampler/storage/storage_rw，实际 {other:?}"
                        )))
                    }
                }
            }
            let g = b.build(args.label.as_deref());
            Ok(PyBindGroup { inner: g }.into_ref(&vm.ctx))
        }

        /// pipeline(shader, mesh, bind_groups, topo="triangles",
        /// cull="none", depth=False, label=None) -> Pipeline：
        /// 管线按本 access 的默认颜色格式建（access() 对位目标格式）；
        /// 顶点布局取自 mesh。**深度契约**：depth=True 须配
        /// raw_render(depth=True)（管线深度目标与通道附件严格校验）。
        #[pymethod]
        fn pipeline(
            &self,
            args: PipelineArgs,
            vm: &VirtualMachine,
        ) -> PyResult<PyRef<PyPipeline>> {
            let bgs: Vec<PyRef<PyBindGroup>> =
                vm.extract_elements_with(&args.bind_groups, |o| {
                    o.clone()
                        .downcast::<PyBindGroup>()
                        .map_err(|_| vm.new_type_error("bind_groups 元素须为 bind_group() 产物"))
                })?;
            let topo = args.topo.as_deref().unwrap_or("triangles");
            let topo = match topo {
                "triangles" => wgpu::PrimitiveTopology::TriangleList,
                "lines" => wgpu::PrimitiveTopology::LineList,
                other => {
                    return Err(vm.new_value_error(format!(
                        "topo 须为 triangles/lines，实际 {other:?}"
                    )))
                }
            };
            let cull = args.cull.as_deref().unwrap_or("none");
            let cull = match cull {
                "none" => CullMode::None,
                "back" => CullMode::Back,
                "front" => CullMode::Front,
                other => {
                    return Err(vm.new_value_error(format!(
                        "cull 须为 none/back/front，实际 {other:?}"
                    )))
                }
            };
            let mut b = match args.flavor.as_deref().unwrap_or("2d") {
                "2d" => self
                    .access
                    .render_pipeline_builder_2d(&args.shader.inner),
                "3d" => self
                    .access
                    .render_pipeline_builder_3d(&args.shader.inner),
                other => {
                    return Err(vm.new_value_error(format!(
                        "flavor 须为 2d/3d，实际 {other:?}"
                    )))
                }
            }
            .topology(topo)
            .cull(cull);
            if args.depth.unwrap_or(false) {
                b = b.depth_mode(DepthMode::Standard).depth_write(true);
            }
            let refs: Vec<&starfish_pygame::render::BindGroup> =
                bgs.iter().map(|g| &g.inner).collect();
            let p = b.build(&refs, &args.mesh.inner, args.label.as_deref());
            Ok(PyPipeline { inner: p }.into_ref(&vm.ctx))
        }

        /// write_buffer(buf, data)：data = 数值列表（LE f32 bytes）或
        /// **Mat4**（16 列主 floats 直写——raw_render 自管相机的正道：
        /// `ac.write_buffer(cam_buf, cam.mvp())`，零全局相机参与）。
        #[pymethod]
        fn write_buffer(
            &self,
            buf: PyRef<PyBuffer>,
            data: PyObjectRef,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let bytes = if let Some(m) = data.downcast_ref::<PyMat4>() {
                m.0.get()
                    .to_cols_array()
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect()
            } else {
                floats_to_bytes(&data, vm)?
            };
            self.access.write_buffer(&buf.inner, 0, &bytes);
            Ok(())
        }

        /// write_buffer_u32(buf, ints)：u32 列表 → LE bytes（indirect
        /// 绘制参数等整数语义数据的写入口）
        #[pymethod]
        fn write_buffer_u32(
            &self,
            buf: PyRef<PyBuffer>,
            ints: PyObjectRef,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let vals: Vec<u32> = vm.extract_elements_with(&ints, |o| {
                u32::try_from_object(vm, o.to_owned())
            })?;
            let bytes: Vec<u8> =
                vals.iter().flat_map(|v| v.to_le_bytes()).collect();
            self.access.write_buffer(&buf.inner, 0, &bytes);
            Ok(())
        }

        /// update_mesh(mesh, vertices, indices=None)：**动态几何**——
        /// 顶点（/索引）字节就地更新，免每帧重建 mesh（粒子/变形路径）。
        /// ⚠️ 容量以建时为准：顶点数不得超过 mesh() 建时的数量
        ///（超出 = wgpu 越界校验错；更大容量请重建）。
        #[pymethod]
        fn update_mesh(
            &self,
            args: UpdateMeshArgs,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let bytes = floats_to_bytes(&args.vertices, vm)?;
            self.access.write_vertex_buffer(&args.mesh.inner, &bytes);
            if let Some(idx) = args.indices {
                let idx: Vec<i64> =
                    vm.extract_elements_with(&idx, |o| {
                        i64::try_from_object(vm, o.to_owned())
                    })?;
                let mut u16s = Vec::with_capacity(idx.len());
                for i in idx {
                    u16s.push(u16::try_from(i).map_err(|_| {
                        vm.new_value_error(format!("索引越界（0..=65535）：{i}"))
                    })?);
                }
                let bytes: Vec<u8> =
                    u16s.iter().flat_map(|v| v.to_le_bytes()).collect();
                self.access
                    .write_index_buffer(&args.mesh.inner, &bytes)
                    .map_err(|e| {
                        vm.new_value_error(format!("索引缓冲更新失败：{e:?}"))
                    })?;
            }
            Ok(())
        }

        /// sampler(mag="linear", min="linear", mipmap="linear",
        /// address="clamp", label=None) -> Sampler：采样器（字符串安全
        /// 收参；address 同时应用 u/v/w；compare 语义 v2 再放）
        #[pymethod]
        fn sampler(&self, args: SamplerArgs, vm: &VirtualMachine) -> PyResult<PyRef<PySampler>> {
            let err = |what: &str, s: &str| {
                vm.new_value_error(format!(
                    "{what} 取值非法：{s:?}（filter = linear/nearest；address = \
                     clamp/repeat/mirror/border）"
                ))
            };
            let mag_s = args.mag.as_deref().unwrap_or("linear");
            let min_s = args.min.as_deref().unwrap_or("linear");
            let mip_s = args.mipmap.as_deref().unwrap_or("linear");
            let addr_s = args.address.as_deref().unwrap_or("clamp");
            let mag = parse_filter(mag_s).ok_or_else(|| err("mag", mag_s))?;
            let min = parse_filter(min_s).ok_or_else(|| err("min", min_s))?;
            let mip = parse_mipmap_filter(mip_s).ok_or_else(|| err("mipmap", mip_s))?;
            let addr = parse_address(addr_s).ok_or_else(|| err("address", addr_s))?;
            let desc = starfish_render::sampler_desc::SamplerDescriptor::new(
                min, mag, mip, addr, addr, addr, None,
            );
            let s = self.access.create_sampler(
                args.label.as_deref().unwrap_or("py_render_sampler"),
                &desc,
            );
            Ok(PySampler { inner: Arc::new(s) }.into_ref(&vm.ctx))
        }

        /// texture(size=(w,h), rgba=None, kind="sampled", label=None)
        /// -> Texture：GPU 纹理（rgba = w×h×4 字节列表，None = 零填充；
        /// kind 目前 "sampled"，storage 形态随计算纹理 v2）
        #[pymethod]
        fn texture(&self, args: TextureArgs, vm: &VirtualMachine) -> PyResult<PyRef<PyTexture>> {
            let (w, h) = parse_size_2d(&args.size, vm)?;
            let kind = args.kind.as_deref().unwrap_or("sampled");
            if kind != "sampled" {
                return Err(vm.new_value_error(format!(
                    "kind 须为 sampled（storage 形态 v2），实际 {kind:?}"
                )));
            }
            let bytes: Vec<u8> = match &args.rgba {
                Some(o) => vm.extract_elements_with(o, |x| {
                    u8::try_from_object(vm, x.clone())
                })?,
                None => vec![0u8; (w as usize) * (h as usize) * 4],
            };
            if bytes.len() != (w as usize) * (h as usize) * 4 {
                return Err(vm.new_value_error(format!(
                    "rgba 字节数 {} ≠ {w}×{h}×4",
                    bytes.len()
                )));
            }
            let image_data = starfish_render::resources::image::ImageData::rgba8(w, h, bytes)
                .map_err(|_| {
                    vm.new_value_error("rgba 字节数与尺寸不符")
                })?;
            let desc = sf::RenderTextureDescriptor::new(
                sf::RenderTextureSemantic::Color,
                sf::RenderTextureUsage::Sampled,
                sf::RenderTextureDim::D2,
                None,
                None,
            );
            let tex = self.access.create_texture_from_rgba8_2d(
                args.label.as_deref().unwrap_or("py_render_texture"),
                &image_data,
                desc,
            );
            Ok(PyTexture {
                texture: Arc::new(tex),
            }
            .into_ref(&vm.ctx))
        }

        /// compute_pipeline(shader, bind_groups, entry="cs_main",
        /// label=None) -> ComputePipeline：计算管线（bind_groups = 计算
        /// 布局声明；Dispatch 经 raw_compute 会话）
        #[pymethod]
        fn compute_pipeline(
            &self,
            args: ComputePipelineArgs,
            vm: &VirtualMachine,
        ) -> PyResult<PyRef<PyComputePipeline>> {
            let bgs: Vec<PyRef<PyBindGroup>> =
                vm.extract_elements_with(&args.bind_groups, |o| {
                    o.clone()
                        .downcast::<PyBindGroup>()
                        .map_err(|_| vm.new_type_error("bind_groups 元素须为 bind_group() 产物"))
                })?;
            let refs: Vec<&starfish_pygame::render::BindGroup> =
                bgs.iter().map(|g| &g.inner).collect();
            let p = self
                .access
                .compute_pipeline_builder(&args.shader.inner)
                .entry_point(args.entry.unwrap_or_else(|| "cs_main".into()))
                .build(&refs, args.label.as_deref());
            Ok(PyComputePipeline { inner: p }.into_ref(&vm.ctx))
        }
    }

    // ── 资源句柄（access.* 产物；透明句柄——wgpu 对象不出 Python 界）──

    /// WGSL 着色器（access.shader 产物）
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "Shader", unhashable = true)]
    pub(crate) struct PyShader {
        inner: Arc<sf::ShaderModule>,
    }

    #[pyclass]
    impl PyShader {}

    /// raw uniform 缓冲（access.buffer 产物；透明句柄）
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "Buffer", unhashable = true)]
    pub(crate) struct PyBuffer {
        inner: Arc<wgpu::Buffer>,
        size: u64,
    }

    #[pyclass]
    impl PyBuffer {}

    /// 顶点网格（access.mesh 产物；透明句柄——vertex_count 可查）
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "Mesh", unhashable = true)]
    pub(crate) struct PyMesh {
        inner: sf::Mesh,
    }

    #[pyclass]
    impl PyMesh {
        /// vertex_count() -> int
        #[pymethod]
        fn vertex_count(&self) -> u32 {
            self.inner.vertex_count()
        }

        /// index_count() -> int（无索引 = 0）
        #[pymethod]
        fn index_count(&self) -> u32 {
            self.inner.index_count()
        }

        /// has_index_buffer() -> bool
        #[pymethod]
        fn has_index_buffer(&self) -> bool {
            self.inner.has_index_buffer()
        }
    }

    /// 绑定组（access.bind_group 产物；透明句柄）
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "BindGroup", unhashable = true)]
    pub(crate) struct PyBindGroup {
        inner: sf::BindGroup,
    }

    #[pyclass]
    impl PyBindGroup {}

    /// 渲染管线（access.pipeline 产物；透明句柄）
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "Pipeline", unhashable = true)]
    pub(crate) struct PyPipeline {
        inner: Arc<sf::RenderPipeline>,
    }

    #[pyclass]
    impl PyPipeline {}

    /// 计算管线（access.compute_pipeline 产物；透明句柄）
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "ComputePipeline", unhashable = true)]
    pub(crate) struct PyComputePipeline {
        inner: Arc<sf::ComputePipeline>,
    }

    #[pyclass]
    impl PyComputePipeline {}

    /// 采样器（access.sampler 产物；透明句柄）
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "Sampler", unhashable = true)]
    pub(crate) struct PySampler {
        inner: Arc<wgpu::Sampler>,
    }

    #[pyclass]
    impl PySampler {}

    /// GPU 纹理（access.texture 产物；透明句柄——sampled 采样 /
    /// bind_group "texture" 绑定同口）
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "Texture", unhashable = true)]
    pub(crate) struct PyTexture {
        texture: Arc<sf::RenderTexture>,
    }

    #[pyclass]
    impl PyTexture {
        /// size() -> (int, int)（像素）
        #[pymethod]
        fn size(&self) -> (u32, u32) {
            (self.texture.width(), self.texture.height())
        }

        /// 默认采样视图（2D 全 mip；缓存于纹理内部）
        pub(crate) fn default_view(&self) -> Arc<wgpu::TextureView> {
            use sf::RenderTextureViewKey;
            self.texture.get_view(&RenderTextureViewKey {
                base_mip: 0,
                mip_count: 1,
                base_layer: 0,
                layer_count: None,
                dimension: wgpu::TextureViewDimension::D2,
                aspect: wgpu::TextureAspect::All,
            })
        }
    }

    // ── 主屏查询（两入口之二——attachment/格式/尺寸经 RenderSurface）──

    /// 主屏当前帧颜色附件（快照副本；查询/未来自绘逃生口——不透明句柄）
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "ColorAttachment", unhashable = true)]
    pub(crate) struct PyColorAttachment;

    #[pyclass]
    impl PyColorAttachment {}

    /// screen_attachment() -> ColorAttachment | None（当前帧颜色附件；
    /// 须在帧内（with 会话中）调用——present 后失效）
    #[pyfunction]
    fn screen_attachment(vm: &VirtualMachine) -> PyResult<Option<PyRef<PyColorAttachment>>> {
        let screen = starfish_pygame::display::get_screen();
        let got = screen.with_surface(|s| s.get_current_color_attachment()).is_some();
        Ok(if got {
            Some(PyColorAttachment.into_ref(&vm.ctx))
        } else {
            None
        })
    }

    /// screen_format() -> str（主屏颜色格式名——wgpu 枚举字符串形态，
    /// 非危险对象）
    #[pyfunction]
    fn screen_format() -> String {
        format!("{:?}", starfish_pygame::display::get_screen().color_format())
    }

    /// screen_size() -> (w, h)（物理像素）
    #[pyfunction]
    fn screen_size() -> (u32, u32) {
        sf::DrawTarget::size(starfish_pygame::display::get_screen())
    }

    /// screen_samples() -> int（MSAA 样本数）
    #[pyfunction]
    fn screen_samples() -> u32 {
        starfish_pygame::display::get_screen().sample_count()
    }

    /// write_camera_mvp(buf[, cam_mvp])：矩阵（列主 16 float）写入
    /// Buffer——**自绘管线在设置相机处的引擎侧 letterbox 插入点**
    ///（group0/b0 = 64B mat4 定约）。两形态：
    /// · 单参 = 全局相机 MVP（present_mvp，帧内快照语义）；
    /// · 双参 = 自管相机矩阵（如 camera_ortho/perspective 的 .mvp()）
    ///   ——引擎自动左乘呈现拟合收进画布（脚本零矩阵知识；桌面/全屏
    ///   恒等，Android 窗口化与主绘制/UI/物理调试线同一映射）。
    #[pyfunction]
    fn write_camera_mvp(buf: PyRef<PyBuffer>, cam_mvp: Option<PyRef<PyMat4>>) {
        let screen = starfish_pygame::display::get_screen();
        let m = match cam_mvp {
            Some(cm) => {
                let fit = screen.present_fit();
                let composed = fit * cm.0.get();
                // 一次性诊断（Android letterbox 判定）：拟合四要素 +
                // 脚本相机平移列 + 合成结果——"组合是否落进 buffer"
                // 一眼可判（sx/sy 应 = 画布占屏比，恒等应 = 1/1）
                static DIAG: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if !DIAG.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    let f = fit.to_cols_array();
                    let c = cm.0.get().to_cols_array();
                    let r = composed.to_cols_array();
                    starfish_std::debug::console_log(&format!(
                        "[wcmvp] fit sx={:.4} sy={:.4} tx={:.4} ty={:.4} | cam平移=({:.1},{:.1}) | 合成平移=({:.1},{:.1}) 合成sx={:.4}",
                        f[0], f[5], f[12], f[13], c[12], c[13],
                        r[12], r[13], r[0],
                    ));
                }
                composed
            }
            None => screen.present_mvp(),
        };
        let bytes: Vec<u8> = m
            .to_cols_array()
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        if let Some(gpu) = sf::gpu_info() {
            gpu.queue.write_buffer(&buf.inner, 0, &bytes);
        }
    }

    /// screen_frame_state() -> str：帧生命周期状态（诊断——begun/
    /// color_frame/物理尺寸，定位探针时序与跳帧）
    #[pyfunction]
    fn screen_frame_state() -> String {
        starfish_pygame::display::get_screen().frame_state()
    }

    /// set_fb_dump(on)：帧缓冲取证 dump 开关（诊断——present 内 resolve
    /// 后采样帧缓冲，经 console_log 落 [fb-dump] 行；Android = dev_log）
    #[pyfunction]
    fn set_fb_dump(on: bool) {
        starfish_render::render_surface::set_fb_dump(on);
    }

    /// screen_probe() -> [(x, y, r, g, b, a), ...]：帧缓冲网格回读
    /// （诊断专用；**flip 前调用有效**——读当前交换链真实内容，绕过
    /// 截图/合成器）。同时经 console_log 落 [screen-probe] 行
    ///（Android = dev_log.txt 可取证）。
    #[pyfunction]
    fn screen_probe(vm: &VirtualMachine) -> PyResult<PyObjectRef> {
        let samples = starfish_pygame::display::get_screen().readback_probe();
        let fmt: Vec<String> = samples
            .iter()
            .map(|(x, y, c)| format!("({x},{y})=#{:02X}{:02X}{:02X}", c[0], c[1], c[2]))
            .collect();
        starfish_std::debug::console_log(&format!("[screen-probe] {}", fmt.join(" ")));
        let items: Vec<PyObjectRef> = samples
            .iter()
            .map(|(x, y, c)| {
                vm.ctx
                    .new_tuple(vec![
                        vm.ctx.new_int(*x as i64).into(),
                        vm.ctx.new_int(*y as i64).into(),
                        vm.ctx.new_int(c[0] as i64).into(),
                        vm.ctx.new_int(c[1] as i64).into(),
                        vm.ctx.new_int(c[2] as i64).into(),
                        vm.ctx.new_int(c[3] as i64).into(),
                    ])
                    .into()
            })
            .collect();
        Ok(vm.ctx.new_list(items).into())
    }

    // ══════════════════════════════════════════════════════════════
    // 批次 D：raw_render 直绘会话（命令记录 → 提交时点回放）
    // ══════════════════════════════════════════════════════════════

    /// 直绘命令（记录-回放：pass 借用无法跨 Python 持有）
    enum RawCmd {
        Pipeline(PyRef<PyPipeline>),
        BindGroup(u32, PyRef<PyBindGroup>),
        Mesh(PyRef<PyMesh>),
        Draw(u32, u32),
        DrawIndexed(u32, u32),
        /// 全量绘制（set_mesh + 按自身计数——免手填顶点范围）
        DrawMesh(PyRef<PyMesh>),
        Scissor(u32, u32, u32, u32),
        Viewport(f32, f32, f32, f32, f32, f32),
        /// 模板动态状态（管线开 stencil 面时）
        StencilRef(u32),
        /// 混合动态常量（blend 用 Constant 因子时）
        BlendConstant(f64, f64, f64, f64),
        /// 调试标注（RenderDoc/Nsight 捕获时可见）
        PushDebugGroup(String),
        PopDebugGroup,
        DebugMarker(String),
        /// indirect 绘制（范围参数在 buffer 内：u32 序列）
        DrawIndirect(PyRef<PyBuffer>, u64),
        DrawIndexedIndirect(PyRef<PyBuffer>, u64),
    }

    /// raw_render 直绘会话（`with t.raw_render(depth=False) as p:`
    /// ——与 `with render()` 并列的底层入口）：p 的方法**记录命令**，
    /// __exit__ 排程到目标提交时点（Screen=present 全部批之后 /
    /// Surface=flush 之后）经 OverlayCtx 回放——恒在游戏内容之上；
    /// 深度恒清 1.0（客人叠加定约）。管线深度须与 depth 参数配对。
    /// **严格精细控制定约（2026-10-25 用户定夺）**：引擎不做相机兜底
    /// ——相机 buffer 由用户全权管理（会话内 `write_camera_mvp(buf)`
    /// 即时写；同帧多 raw 会话须各持独立 buf，共享同一 buf 会被
    /// 后写覆盖[回放延迟性]）。
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "RawPass", unhashable = true)]
    pub(crate) struct PyRawPass {
        cmds: std::cell::RefCell<Vec<RawCmd>>,
        depth: bool,
        /// None = 主屏（present 收口）；Some = 离屏面（flush 收口）
        surface: Option<std::sync::Arc<sf::Surface>>,
    }

    #[pyclass]
    impl PyRawPass {
        #[pymethod]
        fn __enter__(zelf: PyRef<Self>) -> PyRef<Self> {
            zelf
        }

        /// 会话收口：命令排程到目标提交时点回放（登记序即执行序；
        /// 恒在游戏内容之上——客人叠加定约，深度恒清 1.0）
        #[pymethod]
        fn __exit__(&self, _exc: rustpython_vm::function::FuncArgs, vm: &VirtualMachine) -> bool {
            let cmds = std::mem::take(&mut *self.cmds.borrow_mut());
            let depth = self.depth;
            let replay = move |octx: &mut sf::OverlayCtx| {
                octx.with_pass("raw_render", depth, |pass| {
                    for c in &cmds {
                        match c {
                            RawCmd::Pipeline(p) => pass.set_pipeline(&p.inner),
                            RawCmd::BindGroup(i, g) => pass.set_bind_group(*i, &g.inner),
                            RawCmd::Mesh(m) => pass.set_mesh(&m.inner),
                            RawCmd::Draw(a, b) => pass.draw(*a..*b, 0..1),
                            RawCmd::DrawIndexed(a, b) => {
                                pass.draw_indexed(*a..*b, 0, 0..1)
                            }
                            RawCmd::DrawMesh(m) => pass.draw_mesh(&m.inner),
                            RawCmd::Scissor(x, y, w, h) => {
                                pass.set_scissor_rect(*x, *y, *w, *h)
                            }
                            RawCmd::Viewport(x, y, w, h, n, f) => {
                                pass.set_viewport(*x, *y, *w, *h, *n, *f)
                            }
                            RawCmd::StencilRef(v) => pass.set_stencil_reference(*v),
                            RawCmd::BlendConstant(r, g, b, a) => {
                                pass.set_blend_constant(wgpu::Color {
                                    r: *r,
                                    g: *g,
                                    b: *b,
                                    a: *a,
                                })
                            }
                            RawCmd::PushDebugGroup(l) => pass.push_debug_group(l),
                            RawCmd::PopDebugGroup => pass.pop_debug_group(),
                            RawCmd::DebugMarker(l) => pass.insert_debug_marker(l),
                            RawCmd::DrawIndirect(buf, off) => {
                                pass.draw_indirect(&buf.inner, *off)
                            }
                            RawCmd::DrawIndexedIndirect(buf, off) => {
                                pass.draw_indexed_indirect(&buf.inner, *off)
                            }
                        }
                    }
                });
            };
            match &self.surface {
                None => {
                    let _ = starfish_pygame::render::session::with_screen(|pass| {
                        pass.defer_overlay(Box::new(replay))
                    });
                }
                Some(surf) => surf.defer_overlay(replay),
            }
            let _ = vm;
            false
        }

        /// set_pipeline(pipeline)
        #[pymethod]
        fn set_pipeline(&self, pipeline: PyRef<PyPipeline>) {
            self.cmds.borrow_mut().push(RawCmd::Pipeline(pipeline));
        }

        /// set_bind_group(index, group)
        #[pymethod]
        fn set_bind_group(&self, index: u32, group: PyRef<PyBindGroup>) {
            self.cmds.borrow_mut().push(RawCmd::BindGroup(index, group));
        }

        /// set_mesh(mesh)（顶点 + 索引一并绑定）
        #[pymethod]
        fn set_mesh(&self, mesh: PyRef<PyMesh>) {
            self.cmds.borrow_mut().push(RawCmd::Mesh(mesh));
        }

        /// draw(first, end)：顶点范围绘制
        #[pymethod]
        fn draw(&self, first: u32, end: u32) {
            self.cmds.borrow_mut().push(RawCmd::Draw(first, end));
        }

        /// draw_indexed(first, end)：索引范围绘制（须 set_mesh 有索引）
        #[pymethod]
        fn draw_indexed(&self, first: u32, end: u32) {
            self.cmds.borrow_mut().push(RawCmd::DrawIndexed(first, end));
        }

        /// set_scissor(x, y, w, h)（像素裁剪）
        #[pymethod]
        fn set_scissor(&self, x: u32, y: u32, w: u32, h: u32) {
            self.cmds.borrow_mut().push(RawCmd::Scissor(x, y, w, h));
        }

        /// set_viewport(x, y, w, h, min_depth, max_depth)
        #[pymethod]
        fn set_viewport(&self, x: f32, y: f32, w: f32, h: f32, min_d: f32, max_d: f32) {
            self.cmds
                .borrow_mut()
                .push(RawCmd::Viewport(x, y, w, h, min_d, max_d));
        }

        /// draw_mesh(mesh)：全量绘制（自动 set_mesh + 按自身顶点/索引
        /// 计数——免手填范围；set_mesh+draw 的便利形态）
        #[pymethod]
        fn draw_mesh(&self, mesh: PyRef<PyMesh>) {
            self.cmds.borrow_mut().push(RawCmd::DrawMesh(mesh));
        }

        /// set_stencil_reference(value)：模板动态参考值（管线开
        /// stencil 面时的 per-draw 动态状态）
        #[pymethod]
        fn set_stencil_reference(&self, value: u32) {
            self.cmds.borrow_mut().push(RawCmd::StencilRef(value));
        }

        /// set_blend_constant(r, g, b, a)：混合动态常量（blend 因子
        /// 用 Constant/OneMinusConstant 时）
        #[pymethod]
        fn set_blend_constant(&self, r: f64, g: f64, b: f64, a: f64) {
            self.cmds
                .borrow_mut()
                .push(RawCmd::BlendConstant(r, g, b, a));
        }

        /// push_debug_group(label)：调试组开（RenderDoc/Nsight 捕获
        /// 时的命令分组标注）
        #[pymethod]
        fn push_debug_group(&self, label: String) {
            self.cmds.borrow_mut().push(RawCmd::PushDebugGroup(label));
        }

        /// pop_debug_group()：调试组闭
        #[pymethod]
        fn pop_debug_group(&self) {
            self.cmds.borrow_mut().push(RawCmd::PopDebugGroup);
        }

        /// insert_debug_marker(label)：单点调试标注
        #[pymethod]
        fn insert_debug_marker(&self, label: String) {
            self.cmds.borrow_mut().push(RawCmd::DebugMarker(label));
        }

        /// draw_indirect(buf, offset)：间接绘制——顶点/实例范围在
        /// buffer 内（u32×4：vertex_count, instance_count, first_vertex,
        /// first_instance；buffer 须 usage="indirect" 创建 + u32 写入）
        #[pymethod]
        fn draw_indirect(&self, buf: PyRef<PyBuffer>, offset: u64) {
            self.cmds
                .borrow_mut()
                .push(RawCmd::DrawIndirect(buf, offset));
        }

        /// draw_indexed_indirect(buf, offset)：索引间接绘制（u32×5：
        /// index_count, instance_count, first_index, base_vertex,
        /// first_instance）
        #[pymethod]
        fn draw_indexed_indirect(&self, buf: PyRef<PyBuffer>, offset: u64) {
            self.cmds
                .borrow_mut()
                .push(RawCmd::DrawIndexedIndirect(buf, offset));
        }
    }

    /// raw_render(depth=False, surface=None) -> RawPass：直绘会话入口
    ///（PyScreenSession/PySurfaceSession 的 raw_render 委托到此；主屏
    /// 版须在 with screen.render() 块内——排程经活跃会话，Surface 版
    /// 经面 flush 收口）
    pub(crate) fn raw_pass(
        depth: bool,
        surface: Option<std::sync::Arc<sf::Surface>>,
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<PyRawPass>> {
        if surface.is_none() {
            starfish_pygame::render::session::with_screen(|_| {}).map_err(|_| {
                vm.new_type_error(
                    "raw_render 须在 `with screen.render():` 会话块内调用（p = t.raw_render()）",
                )
            })?;
        }
        Ok(PyRawPass {
            cmds: std::cell::RefCell::new(Vec::new()),
            depth,
            surface,
        }
        .into_ref(&vm.ctx))
    }

    // ── raw_compute 计算会话（compute 绑定收官）──────────────────────

    /// 计算命令（记录-回放；独立 compute pass——无附件，资源经 bind
    /// group 访问）
    enum ComputeCmd {
        Pipeline(PyRef<PyComputePipeline>),
        BindGroup(u32, PyRef<PyBindGroup>),
        Dispatch(u32, u32, u32),
    }

    /// raw_compute 计算会话（`with t.raw_compute() as p:`——p 记录
    /// set_pipeline/set_bind_group/dispatch，__exit__ 排程到 present 收口
    /// 回放（独立 compute pass，恒在渲染内容之后提交）。同帧多会话各持
    /// 命令，按登记序执行。
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame.render", name = "RawCompute", unhashable = true)]
    pub(crate) struct PyRawCompute {
        cmds: std::cell::RefCell<Vec<ComputeCmd>>,
    }

    #[pyclass]
    impl PyRawCompute {
        #[pymethod]
        fn __enter__(zelf: PyRef<Self>) -> PyRef<Self> {
            zelf
        }

        /// 会话收口：排程到 present 收口（独立 compute pass 回放）
        #[pymethod]
        fn __exit__(&self, _exc: rustpython_vm::function::FuncArgs, vm: &VirtualMachine) -> bool {
            let cmds = std::mem::take(&mut *self.cmds.borrow_mut());
            let replay = move |octx: &mut sf::OverlayCtx| {
                octx.with_compute_pass("raw_compute", |cp| {
                    for c in &cmds {
                        match c {
                            ComputeCmd::Pipeline(p) => cp.set_pipeline(&p.inner),
                            // bind 组位按登记序自动排（引擎 compute pass
                            // 定约——set_pipeline 后从 0 递增）
                            ComputeCmd::BindGroup(_i, g) => cp.set_bind_group(&g.inner),
                            ComputeCmd::Dispatch(x, y, z) => cp.dispatch(*x, *y, *z),
                        }
                    }
                });
            };
            let _ = starfish_pygame::render::session::with_screen(|pass| {
                pass.defer_overlay(Box::new(replay))
            });
            let _ = vm;
            false
        }

        /// set_pipeline(pipeline)
        #[pymethod]
        fn set_pipeline(&self, pipeline: PyRef<PyComputePipeline>) {
            self.cmds.borrow_mut().push(ComputeCmd::Pipeline(pipeline));
        }

        /// set_bind_group(index, group)
        #[pymethod]
        fn set_bind_group(&self, index: u32, group: PyRef<PyBindGroup>) {
            self.cmds.borrow_mut().push(ComputeCmd::BindGroup(index, group));
        }

        /// dispatch(x, y=1, z=1)：工作组派发
        #[pymethod]
        fn dispatch(&self, x: u32, y: Option<u32>, z: Option<u32>) {
            self.cmds
                .borrow_mut()
                .push(ComputeCmd::Dispatch(x, y.unwrap_or(1), z.unwrap_or(1)));
        }
    }

    /// raw_compute() -> RawCompute：计算会话入口（主屏版须在 with
    /// screen.render() 块内——排程经活跃会话，同 raw_render）
    pub(crate) fn raw_compute_session(
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<PyRawCompute>> {
        starfish_pygame::render::session::with_screen(|_| {}).map_err(|_| {
            vm.new_type_error(
                "raw_compute 须在 `with screen.render():` 会话块内调用（p = t.raw_compute()）",
            )
        })?;
        Ok(PyRawCompute {
            cmds: std::cell::RefCell::new(Vec::new()),
        }
        .into_ref(&vm.ctx))
    }
    pydbg!(PyResourceAccess, "ResourceAccess(<gpu>)");
    pydbg!(PyShader, "Shader(<wgsl>)");
    pydbg!(PyBuffer, "Buffer(<raw>)");
    pydbg!(PyMesh, "Mesh(<gpu>)");
    pydbg!(PyBindGroup, "BindGroup(<gpu>)");
    pydbg!(PyPipeline, "Pipeline(<gpu>)");
    pydbg!(PyColorAttachment, "ColorAttachment(<frame>)");
    pydbg!(PyRawPass, "RawPass(<recording>)");
    pydbg!(PySampler, "Sampler(<gpu>)");
    pydbg!(PyTexture, "Texture(<gpu>)");
    pydbg!(PyComputePipeline, "ComputePipeline(<gpu>)");
    pydbg!(PyRawCompute, "RawCompute(<recording>)");
}

pub(crate) use render::module_def;

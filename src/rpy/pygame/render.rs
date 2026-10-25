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

use rustpython_derive::pymodule;

#[pymodule]
pub(crate) mod render {
    use rustpython_derive::pyclass;
    use rustpython_vm::{PyPayload, PyRef, PyResult, VirtualMachine};

    use crate::rpy::pygame::math::math::PyMat4;

    use starfish_pygame::render as sf;
    use starfish_render::pipeline::CullMode;

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
    fn set_camera(camera: PyRef<PyCamera>) {
        sf::set_camera(&camera.0);
    }

    /// 回自动相机（Auto2d / Auto3d——创建期愿望）。
    #[pyfunction]
    fn reset_camera() {
        sf::reset_camera();
    }

    /// 当前全局 MVP（Auto 状态按主屏尺寸现算——resize 天然跟随）。
    #[pyfunction]
    fn camera_mvp(vm: &VirtualMachine) -> PyRef<PyMat4> {
        PyMat4(std::cell::Cell::new(sf::camera_mvp())).into_ref(&vm.ctx)
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
        dim: Option<i64>,
        fov: Option<f64>,
        near: Option<f64>,
        far: Option<f64>,
        cull: Option<String>,
        blend: Option<String>,
        vm: &VirtualMachine,
    ) -> PyResult<()> {
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
}

pub(crate) use render::module_def;

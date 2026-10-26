//! rpy 壳层：与 RustPython 的**唯一接触面**。
//!
//! RP 接线（vendor main 快照 0.6.0-dev，commit 23fcb8d9）：
//! - `default-features = false` 去 host_env——wasm 必需（crt_fd 不存在），
//!   ctypes 恒不用（C 能力走绑定面 Rust 原生函数）
//! - `freeze-stdlib`：stdlib 冻结进二进制
//! - 三处 android 补丁（libffi 剔除 / ctypes 门控 / static_cell 全局单元）
//!   位置与缘由见 `../starfish/reference/rustpython改动备忘录.md`
//!
//! 绑定面形态（2026-10-24 架构抽象定稿）：**每顶层 Python 模块一文件夹**
//! ——`pygame/`（主模块：顶层类型壳 + 22 子模块绑定）、`pygame_gui/`
//! （feature "gui"）、`pygame_physics/`（feature "physics2d"）；文件夹内
//! 自含各自的 #[pymodule] 声明 / 类型 / 运行时本体。**rpy 根只放共用
//! 面**：本文件（解释器装配 + 安装入口）、[`registry`]（共享挂载器）、
//! [`args`]（收参层）、[`bridge`]（RP 薄壳）、testhost/smoke（宿主）。
//! **新增模块 = 建文件夹（照抄 pygame_gui）+ registry 一行 + Cargo
//! feature 按需**。
//!
//! 外接客人库实现本体：pygame_gui（egui-starfish-we）、pygame_physics
//! （rapier-starfish-debug）——Python 面独立 import，渲染挂接仍经
//! pygame 会话的 draw_ui/draw_debug（Rust 内部协作，import 无耦合）。

pub mod pygame;
#[cfg(feature = "gui")]
pub mod pygame_gui;
// pygame_physics = 双维度单模块（2D 类面 physics2d + 3D 类面 physics3d，
// 3D 类带 3D 后缀）——任一维度 feature 开启即挂载
#[cfg(any(feature = "physics2d", feature = "physics3d"))]
pub mod pygame_physics;
pub mod registry;
pub mod args;
pub mod bridge;
pub mod modfind;
pub mod smoke;
pub mod testhost;

use rustpython_vm::{Interpreter, Settings};

/// RP 解释器装配（统一入口；特性表见 Cargo.toml）。
///
/// init_hook：recursion_limit 抬 1000（debug 默认 256，Python 帧计数守卫）。
/// Windows debug 另有 C 栈软限位撞线，由 build.rs /STACK 解决。
///
/// **注意**：模块注入**不在此处**——init_hook 运行在解释器初始化
/// 深处（importlib 引导未完），此刻动模块机制会递归爆栈（批次 B 实测）。
/// 注入用 [`install_modules`]，宿主壳在 `interp.enter` 内、用户脚本前调用
/// （与旧 shim 的装配点位一致）。
pub fn new_interpreter() -> Interpreter {
    Interpreter::builder(Settings::default())
        .init_hook(|vm| {
            vm.recursion_limit.set(1000);
        })
        .add_frozen_modules(rustpython_pylib::FROZEN_STDLIB)
        .build()
}

/// 绑定面模块注入（宿主壳在 `interp.enter` 内、用户脚本前调用；
/// 见 [`new_interpreter`] 的生命周期说明）。挂载序/依赖约束见
/// [`registry`]。
pub fn install_modules(vm: &rustpython_vm::VirtualMachine) {
    registry::install(vm);
}

pub use modfind::{install_module_finder, install_module_finder_with_table};

pub use smoke::run_smoke;

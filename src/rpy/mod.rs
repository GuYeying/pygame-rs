//! rpy 壳层：与 RustPython 的**唯一接触面**。
//!
//! RP 接线（vendor main 快照 0.6.0-dev，commit 23fcb8d9）：
//! - `default-features = false` 去 host_env——wasm 必需（crt_fd 不存在），
//!   ctypes 恒不用（C 能力走绑定面 Rust 原生函数）
//! - `freeze-stdlib`：stdlib 冻结进二进制
//! - 三处 android 补丁（libffi 剔除 / ctypes 门控 / static_cell 全局单元）
//!   位置与缘由见 `../starfish/reference/rustpython改动备忘录.md`
//!
//! 绑定面形态（批次 B 起）：**原生 `pygame` 模块**（`native_pygame`：
//! pymodule 树 + Screen/Session/Event pyclass），`pygame_shim.py` 已退役；
//! [`bridge`] = 引擎桥（宿主↔Python 共享状态 + 会话/事件/绘制真身 +
//! 探针/旧别名注册），[`args`] = 收参层（FromArgs 结构体 + 参数类型）。

mod native_pygame;
mod pygame_impls;
pub mod args;
pub mod bridge;
pub mod smoke;
pub mod testhost;

use rustpython_vm::{Interpreter, Settings};

/// RP 解释器装配（统一入口；特性表见 Cargo.toml）。
///
/// init_hook：recursion_limit 抬 1000（debug 默认 256，Python 帧计数守卫）。
/// Windows debug 另有 C 栈软限位撞线，由 build.rs /STACK 解决。
///
/// **注意**：pygame 模块注入**不在此处**——init_hook 运行在解释器初始化
/// 深处（importlib 引导未完），此刻动模块机制会递归爆栈（批次 B 实测）。
/// 注入用 [`install_pygame`]，宿主壳在 `interp.enter` 内、用户脚本前调用
/// （与旧 shim 的装配点位一致）。
pub fn new_interpreter() -> Interpreter {
    Interpreter::builder(Settings::default())
        .init_hook(|vm| {
            vm.recursion_limit.set(1000);
        })
        .add_frozen_modules(rustpython_pylib::FROZEN_STDLIB)
        .build()
}

/// 原生 `pygame` 模块注入（宿主壳在 `interp.enter` 内、用户脚本前调用；
/// 见 [`new_interpreter`] 的生命周期说明）。
pub fn install_pygame(vm: &rustpython_vm::VirtualMachine) {
    native_pygame::install(vm);
}

pub use smoke::run_smoke;

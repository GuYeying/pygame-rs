//! pygame.egui 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
#![allow(unused_imports)]
use rustpython_derive::{pymodule, pyclass, FromArgs};
use crate::rpy::{bridge, args};
use crate::rpy::pygame::pygame::{PySurface, PySurfaceSession, PyScreen, PyScreenSession, PyColor};
use rustpython_vm::{
    PyObjectRef, Py, PyPayload, PyRef, PyResult, VirtualMachine,
    function::{FuncArgs, PyComparisonValue},
    types::{Comparable, Constructor, PyComparisonOp, Representable},
    builtins::{PyModule, PyStr, PyStrRef, PyType},
};
use std::cell::Cell;
#[pymodule]
    pub mod egui {
        use egui_starfish_we::egui;
        use rustpython_derive::pyclass;
        use rustpython_vm::{
            PyObjectRef, PyPayload, PyRef, PyResult, Py, VirtualMachine, builtins::PyModule,
        };

        use super::super::event::event::PyEvent;

        /// Ui 对象（frame 回调注入；no_attr 自由类型——方法面在 pyegui）
        #[derive(PyPayload)]
        #[pyclass(no_attr, module = false, name = "Ui", unhashable = true)]
        pub(crate) struct PyUi;

        impl std::fmt::Debug for PyUi {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Ui(<frame-callback>)")
            }
        }

        /// with 布局区对象（`ui.horizontal()` 无参形态的返回值；__exit__
        /// 回填父光标——世代守卫防跨帧持有，方法面在 pyegui）
        #[derive(PyPayload)]
        #[pyclass(no_attr, module = false, name = "UiRegion", unhashable = true)]
        pub(crate) struct PyUiRegion {
            pub(crate) parent: *mut egui::Ui,
            pub(crate) epoch: usize,
        }

        impl std::fmt::Debug for PyUiRegion {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("UiRegion(<with-block>)")
            }
        }

        /// init()：建 Context/输入桥/绘制端（幂等；须在 set_mode 之后）。
        /// **不自动捕获事件**——需要输入显式 `egui.capture()`。
        #[pyfunction]
        fn init(vm: &VirtualMachine) -> PyResult<()> {
            crate::rpy::pyegui::init(vm)
        }

        /// UiBase 基类源码（**Python 层定义**——RP 子类化零障碍、
        /// 子类构造器任意形态；Rust pyclass 做基类会被 Constructor
        /// Args 形态卡死，故走 exec 注入）
        const UIBASE_PY: &str = r#"
class UiBase:
    """egui 面板基类：子类必须实现 draw(self, ui)。

    pygame.egui.frame(panel) 每帧调用 panel.draw(ui)——面板经实例
    属性捕获状态与游戏对象（OOP 门面定约，2026-10-24）。
    """

    def draw(self, ui):
        raise NotImplementedError(
            "egui 面板须继承 pygame.egui.UiBase 并实现 draw(self, ui)"
        )
"#;

        /// module_exec：先补回自动挂载（用户版 exec 顶替了宏自动版——
        /// no_attr 类的静态类型初始化在 __init_attributes 里，漏调即
        /// "static type has not been initialized" panic；pygame 顶层
        /// module_exec 同款先例），再注入 UiBase
        pub fn module_exec(vm: &VirtualMachine, module: &Py<PyModule>) -> PyResult<()> {
            __init_attributes(vm, module);
            let code = vm
                .compile(
                    UIBASE_PY,
                    rustpython_vm::compiler::Mode::Exec,
                    "<pygame.egui.UiBase>".to_owned(),
                )
                .map_err(|e| vm.new_runtime_error(format!("UiBase 编译失败: {e}")))?;
            let scope = vm.new_scope_with_builtins();
            vm.run_code_obj(code, scope.clone())?;
            let uibase = scope.globals.get_item("UiBase", vm)?;
            module.set_attr("UiBase", uibase, vm)?;
            Ok(())
        }

        /// event(e)：**逐条显式喂事件**（`pygame.event.get()` 循环内调用；
        /// 不喂则 egui 无输入——捕获行为在数据流层完全可见）。pygame 事件
        /// → 原始引擎事件全保真重建（PyEvent 载荷持有引擎枚举，`e.mod`
        /// KMOD 位 → 完整修饰键）。
        #[pyfunction]
        fn event(e: PyRef<PyEvent>, vm: &VirtualMachine) -> PyResult<()> {
            crate::rpy::pyegui::event(&e, vm)
        }
        // 注：frame/draw 已合并为会话方法 `t.draw_ui(panel)`（见
        // PyScreenSession）——"画到哪个目标"由会话对象表达。

        /// wants_pointer() -> bool：egui 想要指针输入（游戏让位判断）
        #[pyfunction]
        fn wants_pointer(_vm: &VirtualMachine) -> bool {
            crate::rpy::pyegui::wants_pointer()
        }

        /// wants_keyboard() -> bool：egui 想要键盘输入
        #[pyfunction]
        fn wants_keyboard(_vm: &VirtualMachine) -> bool {
            crate::rpy::pyegui::wants_keyboard()
        }

        /// shutdown()：撤事件监听 + 卸运行时（幂等）
        #[pyfunction]
        fn shutdown(_vm: &VirtualMachine) {
            crate::rpy::pyegui::shutdown()
        }
    }

    // ── rapier：rapier-starfish-debug 原生风格绑定面（2D v1）──────
    // 类型/方法名与 rapier 0.36 词汇一致（不 pygame 化）；实现本体在
    // crate::rpy::pyrapier（帧序/坐标契约/构造 kwargs 定约见其文件头）。
    // feature "physics2d" 门控（3D = physics3d + pygame.rapier3d，后续批次）

// ── 文件级 re-export（外部引用归位：pyegui/pyrapier 引用短路一层）──
pub(crate) use egui::{PyUi, PyUiRegion};
#[cfg(feature = "gui")]
pub(crate) use egui::module_def;

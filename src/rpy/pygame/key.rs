//! pygame.key 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod key {
        use rustpython_vm::{Py, PyPayload, PyRef, PyResult, VirtualMachine, builtins::PyModule, pyclass};
        use starfish_we::KeyCode;
        use crate::rpy::pygame::event::PyKey;

        /// K_* 全表（starfish locals::key 的 108 键，typed KeyCode；
        /// SHIFT/CTRL/ALT/META 为左键别名）。module_exec 钩子注册到
        /// `pygame.key`（sig_probe 契约：`pygame.key.K_a`）。
        /// 依赖 PyKey 的 static type——挂载序上 event 必须先于本模块
        /// （registry children 序的定约）。
        pub(crate) const KEY_TABLE: &[(&str, KeyCode)] = &[
            // 字母
            ("K_a", KeyCode::KeyA), ("K_b", KeyCode::KeyB), ("K_c", KeyCode::KeyC),
            ("K_d", KeyCode::KeyD), ("K_e", KeyCode::KeyE), ("K_f", KeyCode::KeyF),
            ("K_g", KeyCode::KeyG), ("K_h", KeyCode::KeyH), ("K_i", KeyCode::KeyI),
            ("K_j", KeyCode::KeyJ), ("K_k", KeyCode::KeyK), ("K_l", KeyCode::KeyL),
            ("K_m", KeyCode::KeyM), ("K_n", KeyCode::KeyN), ("K_o", KeyCode::KeyO),
            ("K_p", KeyCode::KeyP), ("K_q", KeyCode::KeyQ), ("K_r", KeyCode::KeyR),
            ("K_s", KeyCode::KeyS), ("K_t", KeyCode::KeyT), ("K_u", KeyCode::KeyU),
            ("K_v", KeyCode::KeyV), ("K_w", KeyCode::KeyW), ("K_x", KeyCode::KeyX),
            ("K_y", KeyCode::KeyY), ("K_z", KeyCode::KeyZ),
            // 数字行
            ("K_0", KeyCode::Digit0), ("K_1", KeyCode::Digit1), ("K_2", KeyCode::Digit2),
            ("K_3", KeyCode::Digit3), ("K_4", KeyCode::Digit4), ("K_5", KeyCode::Digit5),
            ("K_6", KeyCode::Digit6), ("K_7", KeyCode::Digit7), ("K_8", KeyCode::Digit8),
            ("K_9", KeyCode::Digit9),
            // 功能键
            ("K_F1", KeyCode::F1), ("K_F2", KeyCode::F2), ("K_F3", KeyCode::F3),
            ("K_F4", KeyCode::F4), ("K_F5", KeyCode::F5), ("K_F6", KeyCode::F6),
            ("K_F7", KeyCode::F7), ("K_F8", KeyCode::F8), ("K_F9", KeyCode::F9),
            ("K_F10", KeyCode::F10), ("K_F11", KeyCode::F11), ("K_F12", KeyCode::F12),
            // 控制键
            ("K_ESCAPE", KeyCode::Escape), ("K_TAB", KeyCode::Tab),
            ("K_CAPSLOCK", KeyCode::CapsLock), ("K_SPACE", KeyCode::Space),
            ("K_RETURN", KeyCode::Enter), ("K_BACKSPACE", KeyCode::Backspace),
            ("K_DELETE", KeyCode::Delete), ("K_INSERT", KeyCode::Insert),
            // 修饰键（左键别名组）
            ("K_SHIFT", KeyCode::ShiftLeft), ("K_LSHIFT", KeyCode::ShiftLeft),
            ("K_RSHIFT", KeyCode::ShiftRight),
            ("K_CTRL", KeyCode::ControlLeft), ("K_LCTRL", KeyCode::ControlLeft),
            ("K_RCTRL", KeyCode::ControlRight),
            ("K_ALT", KeyCode::AltLeft), ("K_LALT", KeyCode::AltLeft),
            ("K_RALT", KeyCode::AltRight),
            ("K_META", KeyCode::MetaLeft), ("K_LMETA", KeyCode::MetaLeft),
            ("K_RMETA", KeyCode::MetaRight),
            ("K_MENU", KeyCode::ContextMenu),
            // 方向 / 翻页
            ("K_UP", KeyCode::ArrowUp), ("K_DOWN", KeyCode::ArrowDown),
            ("K_LEFT", KeyCode::ArrowLeft), ("K_RIGHT", KeyCode::ArrowRight),
            ("K_HOME", KeyCode::Home), ("K_END", KeyCode::End),
            ("K_PAGEUP", KeyCode::PageUp), ("K_PAGEDOWN", KeyCode::PageDown),
            // 符号
            ("K_MINUS", KeyCode::Minus), ("K_EQUALS", KeyCode::Equal),
            ("K_LEFTBRACKET", KeyCode::BracketLeft), ("K_RIGHTBRACKET", KeyCode::BracketRight),
            ("K_BACKSLASH", KeyCode::Backslash), ("K_SEMICOLON", KeyCode::Semicolon),
            ("K_QUOTE", KeyCode::Quote), ("K_BACKQUOTE", KeyCode::Backquote),
            ("K_COMMA", KeyCode::Comma), ("K_PERIOD", KeyCode::Period),
            ("K_SLASH", KeyCode::Slash),
            // 小键盘
            ("K_KP0", KeyCode::Numpad0), ("K_KP1", KeyCode::Numpad1),
            ("K_KP2", KeyCode::Numpad2), ("K_KP3", KeyCode::Numpad3),
            ("K_KP4", KeyCode::Numpad4), ("K_KP5", KeyCode::Numpad5),
            ("K_KP6", KeyCode::Numpad6), ("K_KP7", KeyCode::Numpad7),
            ("K_KP8", KeyCode::Numpad8), ("K_KP9", KeyCode::Numpad9),
            ("K_KP_PLUS", KeyCode::NumpadAdd), ("K_KP_MINUS", KeyCode::NumpadSubtract),
            ("K_KP_MULTIPLY", KeyCode::NumpadMultiply), ("K_KP_DIVIDE", KeyCode::NumpadDivide),
            ("K_KP_ENTER", KeyCode::NumpadEnter), ("K_KP_PERIOD", KeyCode::NumpadDecimal),
            // 其他
            ("K_PRINT", KeyCode::PrintScreen), ("K_SCROLLLOCK", KeyCode::ScrollLock),
            ("K_PAUSE", KeyCode::Pause), ("K_NUMLOCK", KeyCode::NumLock),
        ];
        ///先补挂子模块（自动版被用户 exec 顶替），再注册
        /// K_* typed 常量到 pygame 顶层。Result
        pub fn module_exec(vm: &VirtualMachine, module: &Py<PyModule>) -> PyResult<()> {
            __init_attributes(vm, module);
            for (name, kc) in KEY_TABLE {
                module.set_attr(*name, PyKey(*kc).into_pyobject(vm), vm)?;
            }
            Ok(())
        }

        // ── 修饰键位常量（位值自定，承诺名字与语义）──
        #[pyattr]
        pub(crate) const KMOD_SHIFT: u32 = starfish_pygame::key::KMOD_SHIFT;
        #[pyattr]
        pub(crate) const KMOD_CTRL: u32 = starfish_pygame::key::KMOD_CTRL;
        #[pyattr]
        pub(crate) const KMOD_ALT: u32 = starfish_pygame::key::KMOD_ALT;
        #[pyattr]
        pub(crate) const KMOD_META: u32 = starfish_pygame::key::KMOD_META;


        #[derive(Debug, PyPayload)]
        #[pyclass(no_attr, module = false, name = "Pressed", unhashable = true)]
        pub(crate) struct PyPressed;

        #[pyclass]
        impl PyPressed {
            #[pymethod]
            fn get(&self, key: PyRef<PyKey>) -> bool {
                starfish_pygame::key::get_pressed().get(key.0)
            }
        }


        /// pygame.key.get_pressed() → Pressed 视图（.get(K_x) 查按下态）。
        #[pyfunction]
        fn get_pressed(vm: &VirtualMachine) -> PyResult<PyRef<PyPressed>> {
            Ok(PyPressed.into_ref(&vm.ctx))
        }

        /// 修饰键位集（KMOD_* 位；KeyDown/KeyUp 事件喂）。
        #[pyfunction]
        fn get_mods(_vm: &VirtualMachine) -> u32 {
            starfish_pygame::key::get_mods()
        }

        /// 键名（W3C KeyboardEvent.code 命名——name(K_x) = "KeyX" 等）。
        #[pyfunction]
        fn name(key: PyRef<PyKey>, _vm: &VirtualMachine) -> String {
            starfish_pygame::key::name(key.0)
        }

        /// 窗口键盘聚焦态。
        #[pyfunction]
        fn get_focused(_vm: &VirtualMachine) -> bool {
            starfish_pygame::key::get_focused()
        }

        /// 键重复软件合成（垫底：自动重复由 OS repeat 标记承载，收下不生效）。
        #[pyfunction]
        fn set_repeat(_delay: u32, _interval: u32) {}

        /// 键重复配置查询（垫底：(0, 0) = 未启用软件重复）。
        #[pyfunction]
        fn get_repeat(_vm: &VirtualMachine) -> (u32, u32) {
            (0, 0)
        }


    }


pub(crate) use key::module_def;

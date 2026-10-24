//! pygame.event 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod event {
        use rustpython_vm::builtins::PyModule;
        use starfish_we::KeyCode;
        use starfish_pygame::Event as EngineEvent;
        use rustpython_derive::{pyclass, pymodule};
        use rustpython_vm::types::{Comparable, PyComparisonOp, Representable};
        use rustpython_vm::function::PyComparisonValue;
        use rustpython_vm::{
            Py, PyObject, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
            builtins::{PyStr, PyStrRef},
        };
        use crate::rpy::{bridge, args};


        // ── 常量（tag/键值定约与 bridge.rs 事件映射一致）──
        #[pyattr]
        pub(crate) const QUIT: &str = "quit";
        #[pyattr]
        pub(crate) const KEYDOWN: &str = "keydown";
        #[pyattr]
        pub(crate) const KEYUP: &str = "keyup";
        #[pyattr]
        pub(crate) const MOUSEMOTION: &str = "mousemotion";
        #[pyattr]
        pub(crate) const MOUSEBUTTONDOWN: &str = "mousebuttondown";
        #[pyattr]
        pub(crate) const MOUSEBUTTONUP: &str = "mousebuttonup";
        #[pyattr]
        pub(crate) const TEXTINPUT: &str = "textinput";



        // ── Key：类型化键值（K_* 常量的载体；引擎无整数键码表，typed 是
        //    starfish locals 的既定决策）──
        #[derive(Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame", name = "Key", unhashable = true)]
        pub(crate) struct PyKey(pub(crate) KeyCode);

        #[pyclass(with(Comparable))]
        impl PyKey {}

        impl Comparable for PyKey {
            fn cmp(
                zelf: &Py<Self>,
                other: &PyObject,
                op: PyComparisonOp,
                _vm: &VirtualMachine,
            ) -> PyResult<PyComparisonValue> {
                if op == PyComparisonOp::Eq {
                    if let Some(o) = other.downcast_ref::<PyKey>() {
                        return Ok(PyComparisonValue::Implemented(zelf.0 == o.0));
                    }
                }
                Ok(PyComparisonValue::NotImplemented)
            }
        }

        
        // ── Pressed：key.get_pressed() 的按键按下视图 ──
        // 零尺寸视图不持有（starfish Pressed 无 Debug 不满足 payload 约束），
        // 查询时实时取视图——状态本体在 key 的 thread-local，语义等价。
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



        // ── Event：事件对象（pygame.event.get() 产物）。**全保真载荷**
        //    （2026-10-24 重构）：引擎 typed 枚举原样持有（egui 等事件
        //    逐条消费者可重建完整原始输入），预展开展示字段供 getset 免
        //    vm 取用。`type` 为 tag 字符串；`mod` = KMOD 位（真 pygame
        //    `e.mod` parity）；textinput 走 `text` 槽（旧形态塞 key 槽的
        //    hack 退役）；resized 走 w/h（旧形态塌缩成 "other"）──
        #[derive(Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame", name = "Event", unhashable = true)]
        pub(crate) struct PyEvent {
            pub(crate) ev: EngineEvent,
            pub(crate) typ: PyStrRef,
            pub(crate) key: PyObjectRef,
            pub(crate) x: i32,
            pub(crate) y: i32,
            pub(crate) button: i32,
            /// KMOD 位集（keydown/keyup；真 pygame `e.mod` parity）
            pub(crate) modifiers: u32,
            /// 自动重复（keydown）
            pub(crate) repeat: bool,
            /// 文本（textinput）
            pub(crate) text: PyStrRef,
            /// 尺寸（resized）
            pub(crate) w: i32,
            pub(crate) h: i32,
        }

        #[pyclass(with(Representable))]
        impl PyEvent {
            #[pygetset(name = "type")]
            fn typ(&self) -> PyStrRef {
                self.typ.clone()
            }

            #[pygetset]
            fn key(&self) -> PyObjectRef {
                self.key.clone()
            }

            #[pygetset]
            fn x(&self) -> i32 {
                self.x
            }

            #[pygetset]
            fn y(&self) -> i32 {
                self.y
            }

            #[pygetset]
            fn button(&self) -> i32 {
                self.button
            }

            /// 修饰键位集（KMOD_*；keydown/keyup 携带，其余 0）
            #[pygetset(name = "mod")]
            fn modifiers(&self) -> u32 {
                self.modifiers
            }

            /// 自动重复标记（keydown；其余 False）
            #[pygetset]
            fn repeat(&self) -> bool {
                self.repeat
            }

            /// 文本（textinput；其余 ""）
            #[pygetset]
            fn text(&self) -> PyStrRef {
                self.text.clone()
            }

            /// 宽（resized；其余 0）
            #[pygetset]
            fn w(&self) -> i32 {
                self.w
            }

            /// 高（resized；其余 0）
            #[pygetset]
            fn h(&self) -> i32 {
                self.h
            }
        }

        impl Representable for PyEvent {
            fn repr(zelf: &Py<Self>, vm: &VirtualMachine) -> PyResult<PyRef<PyStr>> {
                Ok(vm.ctx.new_str(format!(
                    "<Event {} key={} pos=({}, {}) button={}>",
                    zelf.typ,
                    zelf.key.repr(vm)?,
                    zelf.x,
                    zelf.y,
                    zelf.button
                )))
            }
        }

        impl PyEvent {
            /// 引擎事件 → PyEvent（**全保真**：枚举原样进载荷 + 展示字段
            /// 预展开。typed 键值定约批次 C：`e.key == pygame.key.K_x` 用
            /// Key 实例值相等比较（K_* 全表在 key 模块——`pygame.event.K_*`
            /// 不存在）；非键事件的 key 位为 int 0——textinput 的
            /// 文本走 `text` 槽）。
            pub(crate) fn from_engine(vm: &VirtualMachine, ev: EngineEvent) -> Self {
                let tag = |s: &str| -> PyStrRef { vm.ctx.intern_str(s).to_owned() };
                let int_obj = |v: u32| -> PyObjectRef { vm.ctx.new_int(v).into() };
                let key_obj = |k: KeyCode| -> PyObjectRef { PyKey(k).into_pyobject(vm) };
                let empty = || vm.ctx.new_str("").to_owned();
                match ev {
                    EngineEvent::Quit => Self {
                        ev, typ: tag("quit"), key: int_obj(0), x: 0, y: 0, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::KeyDown { key, modifiers, repeat } => Self {
                        ev, typ: tag("keydown"), key: key_obj(key), x: 0, y: 0, button: 0,
                        modifiers, repeat, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::KeyUp { key, modifiers } => Self {
                        ev, typ: tag("keyup"), key: key_obj(key), x: 0, y: 0, button: 0,
                        modifiers, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::TextInput { text } => {
                        // text 同时进载荷与展示槽——载荷侧克隆保全（String 非 Copy）
                        let ev = EngineEvent::TextInput { text: text.clone() };
                        Self {
                            ev, typ: tag("textinput"), key: int_obj(0), x: 0, y: 0, button: 0,
                            modifiers: 0, repeat: false,
                            text: vm.ctx.new_str(text).to_owned(), w: 0, h: 0,
                        }
                    }
                    EngineEvent::MouseMotion { x, y } => Self {
                        ev, typ: tag("mousemotion"), key: int_obj(0), x, y, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::MouseButtonDown { button, x, y } => Self {
                        ev, typ: tag("mousebuttondown"), key: int_obj(0), x, y,
                        button: i32::from(button), modifiers: 0, repeat: false,
                        text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::MouseButtonUp { button, x, y } => Self {
                        ev, typ: tag("mousebuttonup"), key: int_obj(0), x, y,
                        button: i32::from(button), modifiers: 0, repeat: false,
                        text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::MouseWheel { x, y } => Self {
                        ev, typ: tag("mousewheel"), key: int_obj(0), x, y, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::Resized { width, height } => Self {
                        ev, typ: tag("resized"), key: int_obj(0), x: 0, y: 0, button: 0,
                        modifiers: 0, repeat: false, text: empty(),
                        w: width as i32, h: height as i32,
                    },
                    EngineEvent::FocusGained => Self {
                        ev, typ: tag("focusgained"), key: int_obj(0), x: 0, y: 0, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::FocusLost => Self {
                        ev, typ: tag("focuslost"), key: int_obj(0), x: 0, y: 0, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::Touch { phase: _, finger, x, y } => Self {
                        ev, typ: tag("touch"), key: int_obj(finger as u32), x, y, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::Suspend => Self {
                        ev, typ: tag("suspend"), key: int_obj(0), x: 0, y: 0, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::Resume => Self {
                        ev, typ: tag("resume"), key: int_obj(0), x: 0, y: 0, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::User { code } => Self {
                        ev, typ: tag("user"), key: int_obj(code), x: 0, y: 0, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                    EngineEvent::Other => Self {
                        ev, typ: tag("other"), key: int_obj(0), x: 0, y: 0, button: 0,
                        modifiers: 0, repeat: false, text: empty(), w: 0, h: 0,
                    },
                }
            }
        }

        /// 排空事件队列（引擎 event::pump 每帧灌入；引擎事件 → PyEvent
        /// 映射同源）。
        #[pyfunction]
        fn get(vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let items: Vec<PyObjectRef> = starfish_pygame::event::get()
                .into_iter()
                .map(|ev| PyEvent::from_engine(vm, ev).into_pyobject(vm))
                .collect();
            Ok(vm.ctx.new_list(items).into())
        }

        /// poll() -> Event | None：单发弹出队首（其余事件留在队列——
        /// 与 get() 的"全量排空"互补）。
        #[pyfunction]
        fn poll(vm: &VirtualMachine) -> PyResult<Option<PyObjectRef>> {
            Ok(starfish_pygame::event::poll()
                .map(|ev| PyEvent::from_engine(vm, ev).into_pyobject(vm)))
        }

        /// clear()：清空事件队列。
        #[pyfunction]
        fn clear(_vm: &VirtualMachine) {
            starfish_pygame::event::clear();
        }
    }


pub(crate) use event::{module_def, PyEvent, PyKey};

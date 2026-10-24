//! pygame.net 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod net {
        use rustpython_derive::pyclass;
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{PyObjectRef, Py, PyPayload, PyResult, VirtualMachine, builtins::{PyStrRef, PyType}};
        use std::cell::RefCell;

        use starfish_net::{ConnState, TcpConn, UdpSock};

        fn data_bytes(data: PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<u8>> {
            if let Some(s) = data.downcast_ref::<rustpython_vm::builtins::PyStr>() {
                return Ok(s
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("内容含非 UTF-8 字符"))?
                    .as_bytes()
                    .to_vec());
            }
            data.try_bytes_like(vm, |b| b.to_vec())
        }

        /// TCP(addr)：消息分帧连接（addr = "host:port"；Web = "ws://host:port"）。
        /// connect 立即返回（后台握手），Connecting 期间 send 入队缓冲。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.net", name = "TCP", unhashable = true)]
        pub(crate) struct PyTcp(RefCell<TcpConn>);

        impl std::fmt::Debug for PyTcp {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("TCP")
            }
        }

        #[pyclass(with(Constructor))]
        impl PyTcp {
            #[pymethod]
            fn send(&self, data: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
                self.0
                    .borrow_mut()
                    .send(&data_bytes(data, vm)?)
                    .map_err(|e| vm.new_value_error(format!("net 发送失败: {e}")))
            }

            /// recv() -> bytes | None：收一帧（非阻塞；无帧 = None，下帧再查）
            #[pymethod]
            fn recv(&self, vm: &VirtualMachine) -> PyResult<Option<PyObjectRef>> {
                Ok(self
                    .0
                    .borrow_mut()
                    .try_recv()
                    .map(|b| vm.ctx.new_bytes(b).into()))
            }

            /// state() -> "connecting" | "connected" | "closed"
            #[pymethod]
            fn state(&self) -> &'static str {
                match self.0.borrow().state() {
                    ConnState::Connecting => "connecting",
                    ConnState::Connected => "connected",
                    ConnState::Closed => "closed",
                }
            }

            #[pymethod]
            fn close(&self) {
                self.0.borrow_mut().close();
            }
        }

        impl Constructor for PyTcp {
            type Args = (String,);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                let conn = TcpConn::connect(&args.0)
                    .map_err(|e| vm.new_value_error(format!("net 连接失败: {e}")))?;
                Ok(Self(RefCell::new(conn)))
            }
        }

        /// UDP(local_addr)：非阻塞数据报套接字（原生平台；Web 调用即报错）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.net", name = "UDP", unhashable = true)]
        pub(crate) struct PyUdp(RefCell<UdpSock>);

        impl std::fmt::Debug for PyUdp {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("UDP")
            }
        }

        #[pyclass(with(Constructor))]
        impl PyUdp {
            /// send_to(data, peer)：对端 "host:port"。
            #[pymethod]
            fn send_to(
                &self,
                data: PyObjectRef,
                peer: PyStrRef,
                vm: &VirtualMachine,
            ) -> PyResult<()> {
                #[cfg(target_arch = "wasm32")]
                return Err(vm.new_value_error("pygame.net: Web 端不支持 UDP"));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let peer = peer
                        .to_str()
                        .ok_or_else(|| vm.new_value_error("对端地址含非 UTF-8 字符"))?;
                    self.0
                        .borrow_mut()
                        .send_to(&data_bytes(data, vm)?, peer)
                        .map_err(|e| vm.new_value_error(format!("net 发送失败: {e}")))
                }
            }

            /// recv_from() -> (bytes, addr) | None：收一报（非阻塞）。
            #[pymethod]
            fn recv_from(&self, vm: &VirtualMachine) -> PyResult<Option<PyObjectRef>> {
                #[cfg(target_arch = "wasm32")]
                return Err(vm.new_value_error("pygame.net: Web 端不支持 UDP"));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    Ok(self.0.borrow_mut().try_recv_from().map(|(data, addr)| {
                        let bytes: PyObjectRef = vm.ctx.new_bytes(data).into();
                        let peer: PyObjectRef = vm.ctx.new_str(addr.to_string()).into();
                        vm.new_tuple((bytes, peer)).into()
                    }))
                }
            }

            /// set_broadcast(on)：广播开关。
            #[pymethod]
            fn set_broadcast(&self, on: bool, vm: &VirtualMachine) -> PyResult<()> {
                #[cfg(target_arch = "wasm32")]
                return Err(vm.new_value_error("pygame.net: Web 端不支持 UDP"));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = vm;
                    self.0
                        .borrow_mut()
                        .set_broadcast(on)
                        .map_err(|e| vm.new_value_error(format!("net set_broadcast 失败: {e}")))
                }
            }
        }

        impl Constructor for PyUdp {
            type Args = (String,);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                #[cfg(target_arch = "wasm32")]
                return Err(vm.new_value_error("pygame.net: Web 端不支持 UDP"));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let sock = UdpSock::bind(&args.0)
                        .map_err(|e| vm.new_value_error(format!("net 绑定失败: {e}")))?;
                    Ok(Self(RefCell::new(sock)))
                }
            }
        }
    }

    // ── dialog：文件打开 / 保存（批次 M；桌面模态——rfd 内置模态消息
    // 循环，阻塞当前帧但窗口保持响应）。Web（input+下载）与 Android
    //（SAF 轮询式 job）v1 不提供同步面，调用即报错——轮询式 job API

pub(crate) use net::module_def;

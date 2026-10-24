//! pygame.math 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod math {
        use rustpython_derive::{pymodule, pyclass};
        use std::cell::Cell;
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::function::ArgIntoFloat;
        use rustpython_vm::protocol::PyNumberMethods;
        use rustpython_vm::types::{AsNumber, Comparable, Constructor, PyComparisonOp};
        use rustpython_vm::function::PyComparisonValue;
        use rustpython_vm::{
            Py, PyObject, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
            builtins::{PyModule, PyStr, PyStrRef, PyType},
        };


        #[pyfunction]
        fn lerp(a: ArgIntoFloat, b: ArgIntoFloat, t: ArgIntoFloat) -> f32 {
            starfish_pygame::math::lerp(a.into_float() as f32, b.into_float() as f32, t.into_float() as f32)
        }

        #[pyfunction]
        fn clamp(x: ArgIntoFloat, min: ArgIntoFloat, max: ArgIntoFloat) -> f32 {
            starfish_pygame::math::clamp(x.into_float() as f32, min.into_float() as f32, max.into_float() as f32)
        }

        #[pyfunction]
        fn inv_lerp(a: ArgIntoFloat, b: ArgIntoFloat, v: ArgIntoFloat) -> f32 {
            starfish_pygame::math::inv_lerp(a.into_float() as f32, b.into_float() as f32, v.into_float() as f32)
        }

        #[pyfunction]
        fn remap(
            in_a: ArgIntoFloat,
            in_b: ArgIntoFloat,
            out_a: ArgIntoFloat,
            out_b: ArgIntoFloat,
            v: ArgIntoFloat,
        ) -> f32 {
            starfish_pygame::math::remap(
                in_a.into_float() as f32,
                in_b.into_float() as f32,
                out_a.into_float() as f32,
                out_b.into_float() as f32,
                v.into_float() as f32,
            )
        }

        #[pyfunction]
        fn smoothstep(e0: ArgIntoFloat, e1: ArgIntoFloat, x: ArgIntoFloat) -> f32 {
            starfish_pygame::math::smoothstep(e0.into_float() as f32, e1.into_float() as f32, x.into_float() as f32)
        }

        #[pyfunction]
        fn smootherstep(e0: ArgIntoFloat, e1: ArgIntoFloat, x: ArgIntoFloat) -> f32 {
            starfish_pygame::math::smootherstep(e0.into_float() as f32, e1.into_float() as f32, x.into_float() as f32)
        }

        // ── Vector2：基础向量（批次 H v1；rotate/angle_to 等留增强批）──
        #[derive(Clone, Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.math", name = "Vector2", unhashable = true)]
        pub(crate) struct PyVector2 {
            pub(crate) x: Cell<f32>,
            pub(crate) y: Cell<f32>,
        }

        impl PyVector2 {
            /// glam 视图（算术协议用）
            fn vec2(&self) -> starfish_pygame::math::Vector2 {
                starfish_pygame::math::Vector2::new(self.x.get(), self.y.get())
            }

            fn from_vec2(vm: &VirtualMachine, v: starfish_pygame::math::Vector2) -> PyRef<Self> {
                Self {
                    x: Cell::new(v.x),
                    y: Cell::new(v.y),
                }
                .into_ref(&vm.ctx)
            }

            // ── 算术分发（AsNumber 闭包复用）：加减仅向量；乘/除收标量
            // 或向量（元素级——glam 语义，差异标注：真 pygame 乘法仅标量）。
            // 不支持的操作数 → NotImplemented 交回数字层 → TypeError ──
            fn arith_add(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec2(vm, x.vec2() + y.vec2()).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_sub(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec2(vm, x.vec2() - y.vec2()).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_mul(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                // 全对称：任一侧向量、另一侧标量/向量（RP 无 right_* 槽——
                // 反射乘法经"尝试 b 的槽"以同参数序到达，必须双侧处理）
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec2(vm, x.vec2() * y.vec2()).into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    return Ok(Self::from_vec2(vm, x.vec2() * (k.into_float() as f32)).into());
                }
                if let (Some(y), Ok(k)) = (
                    b.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, a.to_owned()),
                ) {
                    return Ok(Self::from_vec2(vm, y.vec2() * (k.into_float() as f32)).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_true_div(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec2(vm, x.vec2() / y.vec2()).into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    let k = k.into_float() as f32;
                    if k.abs() < f32::EPSILON {
                        return Err(vm.new_zero_division_error("vector division by zero"));
                    }
                    return Ok(Self::from_vec2(vm, x.vec2() / k).into());
                }
                Ok(vm.ctx.not_implemented())
            }
        }

        impl AsNumber for PyVector2 {
            fn as_number() -> &'static PyNumberMethods {
                static AS_NUMBER: PyNumberMethods = PyNumberMethods {
                    add: Some(PyVector2::arith_add),
                    subtract: Some(PyVector2::arith_sub),
                    multiply: Some(PyVector2::arith_mul),
                    true_divide: Some(PyVector2::arith_true_div),
                    // v += w / v -= w：就地变更（恒等保持——pygame 向量
                    // 语义）；标量操作数 → NotImplemented 回退重绑定
                    inplace_add: Some(|a, b, vm| {
                        if let (Some(zelf), Some(y)) =
                            (a.downcast_ref::<PyVector2>(), b.downcast_ref::<PyVector2>())
                        {
                            let r = zelf.vec2() + y.vec2();
                            zelf.x.set(r.x);
                            zelf.y.set(r.y);
                            return Ok(a.to_owned());
                        }
                        Ok(vm.ctx.not_implemented())
                    }),
                    inplace_subtract: Some(|a, b, vm| {
                        if let (Some(zelf), Some(y)) =
                            (a.downcast_ref::<PyVector2>(), b.downcast_ref::<PyVector2>())
                        {
                            let r = zelf.vec2() - y.vec2();
                            zelf.x.set(r.x);
                            zelf.y.set(r.y);
                            return Ok(a.to_owned());
                        }
                        Ok(vm.ctx.not_implemented())
                    }),
                    negative: Some(|number, vm| {
                        let v = PyVector2::number_downcast(number).vec2();
                        Ok(PyVector2::from_vec2(vm, -v).into())
                    }),
                    ..PyNumberMethods::NOT_IMPLEMENTED
                };
                &AS_NUMBER
            }
        }

        #[pyclass(with(Comparable, Constructor, AsNumber))]
        impl PyVector2 {
            // x/y 属性（读写；pygame Vector2 的基础属性面）
            #[pygetset]
            fn x(&self) -> f32 {
                self.x.get()
            }
            #[pygetset(setter)]
            fn set_x(&self, v: f32) {
                self.x.set(v);
            }
            #[pygetset]
            fn y(&self) -> f32 {
                self.y.get()
            }
            #[pygetset(setter)]
            fn set_y(&self, v: f32) {
                self.y.set(v);
            }

            #[pymethod]
            fn dot(&self, other: PyRef<Self>) -> f32 {
                self.x.get() * other.x.get() + self.y.get() * other.y.get()
            }

            #[pymethod]
            fn length(&self) -> f32 {
                (self.x.get() * self.x.get() + self.y.get() * self.y.get()).sqrt()
            }

            #[pymethod]
            fn normalize(&self, vm: &VirtualMachine) -> PyResult<PyRef<Self>> {
                let len = self.length();
                let (x, y) = if len > f32::EPSILON {
                    (self.x.get() / len, self.y.get() / len)
                } else {
                    (0.0, 0.0) // 零向量归一化 = 零向量（pygame 语义）
                };
                Ok(PyVector2 {
                    x: Cell::new(x),
                    y: Cell::new(y),
                }
                .into_ref(&vm.ctx))
            }

        }

        impl Comparable for PyVector2 {
            fn cmp(
                zelf: &Py<Self>,
                other: &PyObject,
                op: PyComparisonOp,
                _vm: &VirtualMachine,
            ) -> PyResult<PyComparisonValue> {
                if op == PyComparisonOp::Eq {
                    if let Some(o) = other.downcast_ref::<PyVector2>() {
                        return Ok(PyComparisonValue::Implemented(
                            zelf.x.get() == o.x.get() && zelf.y.get() == o.y.get(),
                        ));
                    }
                }
                Ok(PyComparisonValue::NotImplemented)
            }
        }

        impl Constructor for PyVector2 {
            type Args = (ArgIntoFloat, ArgIntoFloat);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self {
                    x: Cell::new(args.0.into_float() as f32),
                    y: Cell::new(args.1.into_float() as f32),
                })
            }
        }

        // ── Vector3：三维向量（批次 K；starfish math Vector3 = glam::Vec3）──
        #[derive(Clone, Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.math", name = "Vector3", unhashable = true)]
        pub(crate) struct PyVector3 {
            pub(crate) x: Cell<f32>,
            pub(crate) y: Cell<f32>,
            pub(crate) z: Cell<f32>,
        }

        impl PyVector3 {
            /// glam 视图（方法面从此构造）
            fn vec(&self) -> starfish_pygame::math::Vector3 {
                starfish_pygame::math::Vector3::new(self.x.get(), self.y.get(), self.z.get())
            }

            fn from_vec(vm: &VirtualMachine, v: starfish_pygame::math::Vector3) -> PyRef<Self> {
                Self {
                    x: Cell::new(v.x),
                    y: Cell::new(v.y),
                    z: Cell::new(v.z),
                }
                .into_ref(&vm.ctx)
            }

            // ── 算术分发（AsNumber 闭包复用）：同 Vector2 约定 ──
            fn arith_add(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec(vm, x.vec() + y.vec()).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_sub(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec(vm, x.vec() - y.vec()).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_mul(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                // 全对称：任一侧向量、另一侧标量/向量（同 Vector2 注）
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec(vm, x.vec() * y.vec()).into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    return Ok(Self::from_vec(vm, x.vec() * (k.into_float() as f32)).into());
                }
                if let (Some(y), Ok(k)) = (
                    b.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, a.to_owned()),
                ) {
                    return Ok(Self::from_vec(vm, y.vec() * (k.into_float() as f32)).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_true_div(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec(vm, x.vec() / y.vec()).into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    let k = k.into_float() as f32;
                    if k.abs() < f32::EPSILON {
                        return Err(vm.new_zero_division_error("vector division by zero"));
                    }
                    return Ok(Self::from_vec(vm, x.vec() / k).into());
                }
                Ok(vm.ctx.not_implemented())
            }
        }

        impl AsNumber for PyVector3 {
            fn as_number() -> &'static PyNumberMethods {
                static AS_NUMBER: PyNumberMethods = PyNumberMethods {
                    add: Some(PyVector3::arith_add),
                    subtract: Some(PyVector3::arith_sub),
                    multiply: Some(PyVector3::arith_mul),
                    true_divide: Some(PyVector3::arith_true_div),
                    inplace_add: Some(|a, b, vm| {
                        if let (Some(zelf), Some(y)) =
                            (a.downcast_ref::<PyVector3>(), b.downcast_ref::<PyVector3>())
                        {
                            let r = zelf.vec() + y.vec();
                            zelf.x.set(r.x);
                            zelf.y.set(r.y);
                            zelf.z.set(r.z);
                            return Ok(a.to_owned());
                        }
                        Ok(vm.ctx.not_implemented())
                    }),
                    inplace_subtract: Some(|a, b, vm| {
                        if let (Some(zelf), Some(y)) =
                            (a.downcast_ref::<PyVector3>(), b.downcast_ref::<PyVector3>())
                        {
                            let r = zelf.vec() - y.vec();
                            zelf.x.set(r.x);
                            zelf.y.set(r.y);
                            zelf.z.set(r.z);
                            return Ok(a.to_owned());
                        }
                        Ok(vm.ctx.not_implemented())
                    }),
                    negative: Some(|number, vm| {
                        let v = PyVector3::number_downcast(number).vec();
                        Ok(PyVector3::from_vec(vm, -v).into())
                    }),
                    ..PyNumberMethods::NOT_IMPLEMENTED
                };
                &AS_NUMBER
            }
        }

        #[pyclass(with(Comparable, Constructor, AsNumber))]
        impl PyVector3 {
            #[pygetset]
            fn x(&self) -> f32 {
                self.x.get()
            }
            #[pygetset(setter)]
            fn set_x(&self, v: f32) {
                self.x.set(v);
            }
            #[pygetset]
            fn y(&self) -> f32 {
                self.y.get()
            }
            #[pygetset(setter)]
            fn set_y(&self, v: f32) {
                self.y.set(v);
            }
            #[pygetset]
            fn z(&self) -> f32 {
                self.z.get()
            }
            #[pygetset(setter)]
            fn set_z(&self, v: f32) {
                self.z.set(v);
            }

            #[pymethod]
            fn dot(&self, other: PyRef<Self>) -> f32 {
                self.vec().dot(other.vec())
            }

            /// 叉积（右手系）
            #[pymethod]
            fn cross(&self, other: PyRef<Self>, vm: &VirtualMachine) -> PyRef<Self> {
                Self::from_vec(vm, self.vec().cross(other.vec()))
            }

            #[pymethod]
            fn length(&self) -> f32 {
                self.vec().length()
            }

            #[pymethod]
            fn length_squared(&self) -> f32 {
                self.vec().length_squared()
            }

            /// 归一化（返回新向量；零向量 = 零向量，pygame 语义）
            #[pymethod]
            fn normalize(&self, vm: &VirtualMachine) -> PyRef<Self> {
                let v = self.vec();
                if v.length_squared() > f32::EPSILON {
                    Self::from_vec(vm, v.normalize())
                } else {
                    Self::from_vec(vm, starfish_pygame::math::Vector3::ZERO)
                }
            }

            /// 到 other 的直线距离（pygame 命名）。
            #[pymethod]
            fn distance_to(&self, other: PyRef<Self>) -> f32 {
                self.vec().distance(other.vec())
            }

            /// 线性插值（t = 0 本体 → 1 other）。
            #[pymethod]
            fn lerp(&self, other: PyRef<Self>, t: ArgIntoFloat, vm: &VirtualMachine) -> PyRef<Self> {
                Self::from_vec(vm, self.vec().lerp(other.vec(), t.into_float() as f32))
            }

            /// 就地缩放到指定长度（pygame 语义：改自身）。
            #[pymethod]
            fn scale_to_length(&self, target: ArgIntoFloat) {
                let v = self.vec();
                let len = v.length();
                if len > f32::EPSILON {
                    let k = target.into_float() as f32 / len;
                    self.x.set(v.x * k);
                    self.y.set(v.y * k);
                    self.z.set(v.z * k);
                }
            }

        }

        impl Comparable for PyVector3 {
            fn cmp(
                zelf: &Py<Self>,
                other: &PyObject,
                op: PyComparisonOp,
                _vm: &VirtualMachine,
            ) -> PyResult<PyComparisonValue> {
                if op == PyComparisonOp::Eq {
                    if let Some(o) = other.downcast_ref::<PyVector3>() {
                        return Ok(PyComparisonValue::Implemented(
                            zelf.x.get() == o.x.get()
                                && zelf.y.get() == o.y.get()
                                && zelf.z.get() == o.z.get(),
                        ));
                    }
                }
                Ok(PyComparisonValue::NotImplemented)
            }
        }

        impl Constructor for PyVector3 {
            type Args = (ArgIntoFloat, ArgIntoFloat, ArgIntoFloat);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self {
                    x: Cell::new(args.0.into_float() as f32),
                    y: Cell::new(args.1.into_float() as f32),
                    z: Cell::new(args.2.into_float() as f32),
                })
            }
        }

        // ── Quaternion：四元数（starfish 扩展类型 = glam::Quat 直通）──
        // 构造经模块函数（quat_identity / quat_from_axis_angle /
        // quat_from_euler，度数制）；类直呼 = xyzw 四参。
        #[derive(Clone, Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.math", name = "Quaternion", unhashable = true)]
        pub(crate) struct PyQuaternion(pub(crate) Cell<starfish_pygame::math::Quaternion>);

        impl PyQuaternion {
            /// 算术分发（批次 P）：quat × quat（组合）或 quat × 标量
            ///（缩放，glam 语义）；全对称（反射经 b 的槽同参数序到达）。
            fn arith_mul(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self(Cell::new(x.0.get() * y.0.get()))
                        .into_ref(&vm.ctx)
                        .into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    return Ok(Self(Cell::new(x.0.get() * (k.into_float() as f32)))
                        .into_ref(&vm.ctx)
                        .into());
                }
                if let (Some(y), Ok(k)) = (
                    b.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, a.to_owned()),
                ) {
                    return Ok(Self(Cell::new(y.0.get() * (k.into_float() as f32)))
                        .into_ref(&vm.ctx)
                        .into());
                }
                Ok(vm.ctx.not_implemented())
            }
        }

        impl AsNumber for PyQuaternion {
            fn as_number() -> &'static PyNumberMethods {
                static AS_NUMBER: PyNumberMethods = PyNumberMethods {
                    multiply: Some(PyQuaternion::arith_mul),
                    negative: Some(|number, vm| {
                        let q = PyQuaternion::number_downcast(number).0.get();
                        Ok(PyQuaternion(Cell::new(-q)).into_ref(&vm.ctx).into())
                    }),
                    ..PyNumberMethods::NOT_IMPLEMENTED
                };
                &AS_NUMBER
            }
        }

        #[pyclass(with(Comparable, Constructor, AsNumber))]
        impl PyQuaternion {
            #[pygetset]
            fn x(&self) -> f32 {
                self.0.get().x
            }
            #[pygetset]
            fn y(&self) -> f32 {
                self.0.get().y
            }
            #[pygetset]
            fn z(&self) -> f32 {
                self.0.get().z
            }
            #[pygetset]
            fn w(&self) -> f32 {
                self.0.get().w
            }

            #[pymethod]
            fn length(&self) -> f32 {
                self.0.get().length()
            }

            /// 归一化（返回新四元数；零长度 = 原样——glam 归零会产生 NaN）
            #[pymethod]
            fn normalize(&self, vm: &VirtualMachine) -> PyRef<Self> {
                let q = self.0.get();
                let out = if q.length_squared() > f32::EPSILON {
                    q.normalize()
                } else {
                    q
                };
                Self(Cell::new(out)).into_ref(&vm.ctx)
            }

            #[pymethod]
            fn dot(&self, other: PyRef<Self>) -> f32 {
                self.0.get().dot(other.0.get())
            }

            /// 球面插值（t = 0 本体 → 1 other）。
            #[pymethod]
            fn slerp(&self, other: PyRef<Self>, t: ArgIntoFloat, vm: &VirtualMachine) -> PyRef<Self> {
                Self(Cell::new(
                    self.0.get().slerp(other.0.get(), t.into_float() as f32),
                ))
                .into_ref(&vm.ctx)
            }
        }

        impl Comparable for PyQuaternion {
            fn cmp(
                zelf: &Py<Self>,
                other: &PyObject,
                op: PyComparisonOp,
                _vm: &VirtualMachine,
            ) -> PyResult<PyComparisonValue> {
                if op == PyComparisonOp::Eq {
                    if let Some(o) = other.downcast_ref::<PyQuaternion>() {
                        let (a, b) = (zelf.0.get(), o.0.get());
                        return Ok(PyComparisonValue::Implemented(
                            a.x == b.x && a.y == b.y && a.z == b.z && a.w == b.w,
                        ));
                    }
                }
                Ok(PyComparisonValue::NotImplemented)
            }
        }

        impl Constructor for PyQuaternion {
            type Args = (ArgIntoFloat, ArgIntoFloat, ArgIntoFloat, ArgIntoFloat);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self(Cell::new(starfish_pygame::math::Quaternion::from_xyzw(
                    args.0.into_float() as f32,
                    args.1.into_float() as f32,
                    args.2.into_float() as f32,
                    args.3.into_float() as f32,
                ))))
            }
        }

        // ── Mat4：4x4 矩阵（glam 直通；无算符协议——乘法用 mul_mat4）──
        // 构造经模块函数（mat4_identity / mat4_from_quat / mat4_translation /
        // mat4_scale）；直呼不可实例化。
        #[derive(Clone, Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.math", name = "Mat4", unhashable = true)]
        pub(crate) struct PyMat4(pub(crate) Cell<starfish_pygame::math::Mat4>);

        impl PyMat4 {
            /// 算术分发（批次 P）：mat × mat（组合）或 mat × 标量；全对称。
            fn arith_mul(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self(Cell::new(x.0.get() * y.0.get()))
                        .into_ref(&vm.ctx)
                        .into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    return Ok(Self(Cell::new(x.0.get() * (k.into_float() as f32)))
                        .into_ref(&vm.ctx)
                        .into());
                }
                if let (Some(y), Ok(k)) = (
                    b.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, a.to_owned()),
                ) {
                    return Ok(Self(Cell::new(y.0.get() * (k.into_float() as f32)))
                        .into_ref(&vm.ctx)
                        .into());
                }
                Ok(vm.ctx.not_implemented())
            }
        }

        impl AsNumber for PyMat4 {
            fn as_number() -> &'static PyNumberMethods {
                static AS_NUMBER: PyNumberMethods = PyNumberMethods {
                    multiply: Some(PyMat4::arith_mul),
                    negative: Some(|number, vm| {
                        let m = PyMat4::number_downcast(number).0.get();
                        Ok(PyMat4(Cell::new(-m)).into_ref(&vm.ctx).into())
                    }),
                    ..PyNumberMethods::NOT_IMPLEMENTED
                };
                &AS_NUMBER
            }
        }

        #[pyclass(with(AsNumber))]
        impl PyMat4 {
            /// 矩阵乘（self * other——变换叠加顺序同 glam）。
            #[pymethod]
            fn mul_mat4(&self, other: PyRef<Self>, vm: &VirtualMachine) -> PyRef<Self> {
                Self(Cell::new(self.0.get() * other.0.get())).into_ref(&vm.ctx)
            }

            /// 变换三维点（含透视除法前的位置变换；glam transform_point3）。
            #[pymethod]
            fn transform_point3(&self, v: PyRef<PyVector3>, vm: &VirtualMachine) -> PyRef<PyVector3> {
                PyVector3::from_vec(vm, self.0.get().transform_point3(v.vec()))
            }
        }

        // ── 3D 数学模块函数（度数制入口；弧度制走 glam 语义层）─────────

        /// 单位四元数。
        #[pyfunction]
        fn quat_identity(vm: &VirtualMachine) -> PyRef<PyQuaternion> {
            PyQuaternion(Cell::new(starfish_pygame::math::Quaternion::IDENTITY))
                .into_ref(&vm.ctx)
        }

        /// 轴角 → 四元数（axis 分量 + 度数制角；轴无需预归一）。
        #[pyfunction]
        fn quat_from_axis_angle(
            ax: ArgIntoFloat,
            ay: ArgIntoFloat,
            az: ArgIntoFloat,
            deg: ArgIntoFloat,
            vm: &VirtualMachine,
        ) -> PyRef<PyQuaternion> {
            let axis = starfish_pygame::math::Vector3::new(
                ax.into_float() as f32,
                ay.into_float() as f32,
                az.into_float() as f32,
            )
            .normalize_or_zero();
            let q = starfish_pygame::math::Quaternion::from_axis_angle(
                axis,
                (deg.into_float() as f32).to_radians(),
            );
            PyQuaternion(Cell::new(q)).into_ref(&vm.ctx)
        }

        /// 欧拉角 → 四元数（pitch/yaw/roll 度数制，XYZ 序——glam EulerRot::XYZ）。
        #[pyfunction]
        fn quat_from_euler(
            pitch: ArgIntoFloat,
            yaw: ArgIntoFloat,
            roll: ArgIntoFloat,
            vm: &VirtualMachine,
        ) -> PyRef<PyQuaternion> {
            use glam::EulerRot;
            let q = starfish_pygame::math::Quaternion::from_euler(
                EulerRot::XYZ,
                (pitch.into_float() as f32).to_radians(),
                (yaw.into_float() as f32).to_radians(),
                (roll.into_float() as f32).to_radians(),
            );
            PyQuaternion(Cell::new(q)).into_ref(&vm.ctx)
        }

        /// 4x4 单位矩阵。
        #[pyfunction]
        fn mat4_identity(vm: &VirtualMachine) -> PyRef<PyMat4> {
            PyMat4(Cell::new(starfish_pygame::math::Mat4::IDENTITY)).into_ref(&vm.ctx)
        }

        /// 四元数 → 旋转矩阵。
        #[pyfunction]
        fn mat4_from_quat(q: PyRef<PyQuaternion>, vm: &VirtualMachine) -> PyRef<PyMat4> {
            PyMat4(Cell::new(
                starfish_pygame::math::Mat4::from_quat(q.0.get()),
            ))
            .into_ref(&vm.ctx)
        }

        /// 平移矩阵。
        #[pyfunction]
        fn mat4_translation(
            x: ArgIntoFloat,
            y: ArgIntoFloat,
            z: ArgIntoFloat,
            vm: &VirtualMachine,
        ) -> PyRef<PyMat4> {
            PyMat4(Cell::new(starfish_pygame::math::Mat4::from_translation(
                starfish_pygame::math::Vector3::new(
                    x.into_float() as f32,
                    y.into_float() as f32,
                    z.into_float() as f32,
                ),
            )))
            .into_ref(&vm.ctx)
        }

        /// 缩放矩阵。
        #[pyfunction]
        fn mat4_scale(
            x: ArgIntoFloat,
            y: ArgIntoFloat,
            z: ArgIntoFloat,
            vm: &VirtualMachine,
        ) -> PyRef<PyMat4> {
            PyMat4(Cell::new(starfish_pygame::math::Mat4::from_scale(
                starfish_pygame::math::Vector3::new(
                    x.into_float() as f32,
                    y.into_float() as f32,
                    z.into_float() as f32,
                ),
            )))
            .into_ref(&vm.ctx)
        }
    }

pub(crate) use math::module_def;

//! pygame 模块注册（2026-10-24 整理批次：原 native_pygame.rs 的
//! install/try_install 收拢于此，改手工挂载——拆分后子模块为独立
//! #[pymodule] 文件，父壳 pygame.rs 仅承载 def）。
//!
//! 挂载顺序定约：
//! 1. 顶层类型（Surface/Rect/Color 经 `make_static_type`——子模块的
//!    事件/绘制面引用其 static type）；
//! 2. 子模块按依赖序（display → … → sprite → egui → rapier），
//!    **event 先于 key**——key 的 K_* 挂载钩子取 PyKey 的 static type，
//!    PyKey 在 event 的 `__init_attributes` 里创建（实测 panic 教训）；
//! 3. sys.modules 补账（`import pygame.xxx` 直达；集合与拆分前一致）。
//!
//! install 时点不变：**必须在 interp.enter 内、essential init 完成后
//! 调用**（init_hook 时点调用会递归爆栈——解释器初始化深处动模块机制
//! 的实测教训，见 `rpy::new_interpreter` 的生命周期说明）。

use rustpython_vm::{PyObjectRef, VirtualMachine, builtins::PyModuleDef};

use crate::rpy::bridge::exc_msg;
use crate::rpy::mods;
use crate::rpy::pygame::pygame;

/// 多阶段 init 后注入 sys.modules（`import pygame` 的直达路径）。
pub fn install(vm: &VirtualMachine) {
    if let Err(msg) = try_install(vm) {
        panic!("pygame 原生模块注入失败: {msg}");
    }
}

/// 子模块挂载描述（attr = 父模块属性名；def = 各 mods/*.rs 的 def）
struct Child {
    attr: &'static str,
    full: &'static str,
    def: &'static PyModuleDef,
}

fn try_install(vm: &VirtualMachine) -> Result<(), String> {
    let def = pygame::module_def(&vm.ctx);
    let module = def
        .create_module(vm)
        .map_err(|e| format!("create_module: {}", exc_msg(vm, e)))?;
    def.exec_module(vm, &module)
        .map_err(|e| format!("exec_module: {}", exc_msg(vm, e)))?;
    let sys_modules = vm
        .sys_module
        .get_attr("modules", vm)
        .map_err(|e| format!("sys.modules fetch: {}", exc_msg(vm, e)))?;
    sys_modules
        .set_item("pygame", module.clone().into(), vm)
        .map_err(|e| format!("sys.modules inject: {}", exc_msg(vm, e)))?;


    // 2. 子模块按依赖序挂载（event 最后）
    let children: &[Child] = &[
        child("display", mods::display::module_def(&vm.ctx)),
        child("time", mods::time::module_def(&vm.ctx)),
        child("font", mods::font::module_def(&vm.ctx)),
        child("mixer", mods::mixer::module_def(&vm.ctx)),
        child("math", mods::math::module_def(&vm.ctx)),
        child("sndarray", mods::sndarray::module_def(&vm.ctx)),
        child("version", mods::version::module_def(&vm.ctx)),
        child("transform", mods::transform::module_def(&vm.ctx)),
        child("image", mods::image::module_def(&vm.ctx)),
        // event 先于 key：key 的 K_* 挂载钩子（module_exec）取 PyKey 的
        // static type——PyKey 类型在 event 的 __init_attributes 里创建。
        child("event", mods::event::module_def(&vm.ctx)),
        child("key", mods::key::module_def(&vm.ctx)),
        child("mouse", mods::mouse::module_def(&vm.ctx)),
        child("gamepad", mods::gamepad::module_def(&vm.ctx)),
        child("touch", mods::touch::module_def(&vm.ctx)),
        child("io", mods::io::module_def(&vm.ctx)),
        child("net", mods::net::module_def(&vm.ctx)),
        child("dialog", mods::dialog::module_def(&vm.ctx)),
        child("gfxdraw", mods::gfxdraw::module_def(&vm.ctx)),
        child("mask", mods::mask::module_def(&vm.ctx)),
        child("permission", mods::permission::module_def(&vm.ctx)),
        child("video", mods::video::module_def(&vm.ctx)),
        child("sprite", mods::sprite::module_def(&vm.ctx)),
        #[cfg(feature = "gui")]
        child("egui", mods::egui::module_def(&vm.ctx)),
        #[cfg(feature = "physics2d")]
        child("rapier", mods::rapier::module_def(&vm.ctx)),
    ];
    for c in children {
        let child = c
            .def
            .create_module(vm)
            .map_err(|e| format!("child {}: {}", c.attr, exc_msg(vm, e)))?;
        c.def
            .exec_module(vm, &child)
            .map_err(|e| format!("child {}: {}", c.attr, exc_msg(vm, e)))?;
        let child_obj: PyObjectRef = child.clone().into();
        module
            .set_attr(c.attr, child_obj, vm)
            .map_err(|e| format!("child {}: {}", c.attr, exc_msg(vm, e)))?;
        // sys.modules 补账（原集合——`import pygame.xxx` 直达；其余子模块
        // 靠属性访问，行为与拆分前一致）
        let in_sys = matches!(
            c.full,
            "pygame.display"
                | "pygame.event"
                | "pygame.time"
                | "pygame.font"
                | "pygame.mixer"
                | "pygame.transform"
                | "pygame.image"
                | "pygame.mask"
                | "pygame.permission"
                | "pygame.video"
                | "pygame.egui"
                | "pygame.rapier"
        );
        if in_sys {
            sys_modules
                .set_item(c.full, child.into(), vm)
                .map_err(|e| format!("child inject {}: {}", c.full, exc_msg(vm, e)))?;
        }
    }
    Ok(())
}

fn child(attr: &'static str, def: &'static PyModuleDef) -> Child {
    Child {
        attr,
        full: match attr {
            "display" => "pygame.display",
            "time" => "pygame.time",
            "font" => "pygame.font",
            "mixer" => "pygame.mixer",
            "math" => "pygame.math",
            "sndarray" => "pygame.sndarray",
            "version" => "pygame.version",
            "transform" => "pygame.transform",
            "image" => "pygame.image",
            "key" => "pygame.key",
            "mouse" => "pygame.mouse",
            "gamepad" => "pygame.gamepad",
            "touch" => "pygame.touch",
            "io" => "pygame.io",
            "net" => "pygame.net",
            "dialog" => "pygame.dialog",
            "gfxdraw" => "pygame.gfxdraw",
            "mask" => "pygame.mask",
            "permission" => "pygame.permission",
            "video" => "pygame.video",
            "sprite" => "pygame.sprite",
            "egui" => "pygame.egui",
            "rapier" => "pygame.rapier",
            _ => "pygame.event",
        },
        def,
    }
}

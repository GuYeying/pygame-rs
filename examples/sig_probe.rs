//! sig_probe：绑定面签名契约锚点（批次 A，无头——零窗口/GPU 依赖）。
//!
//! 原生 pygame 模块路径的可执行规格（模块树/常量/Color/Rect/Key/math/
//! mixer 度量面等）在 `sig_probe.py` 锁定——签名回归先跑本探针，再上
//! 真机批次。draw 族 FromArgs 收参需真会话，由 binding_probe 覆盖。
//!
//! 运行：`cargo run -p pygame-rs --example sig_probe`
//! 预期：`[sig_probe] PASS`；Exit 1 = 契约破坏。

use pygame_rs::rpy;

fn main() {
    let interp = rpy::new_interpreter();
    let err = interp.enter(|vm| -> Option<String> {
        let scope = vm.new_scope_with_builtins();
        rpy::install_modules(vm); // 原生 pygame 模块注入（enter 内，脚本前）
        let code = vm
            .compile(
                include_str!("sig_probe.py"),
                rustpython_vm::compiler::Mode::Exec,
                "sig_probe.py".to_owned(),
            )
            .map_err(|e| format!("编译失败: {e}"))
            .ok()?;
        if let Err(exc) = vm.run_code_obj(code, scope) {
            let obj: rustpython_vm::PyObjectRef = exc.clone().into();
            let msg = obj
                .repr(vm)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("{exc:?}"));
            return Some(format!("脚本异常: {msg}"));
        }
        None
    });
    // 显式收口音频混音器（留到主线程 TLS 析构期 Drop cpal 流会崩）
    starfish_pygame::mixer::quit();
    match err {
        Some(msg) => {
            println!("[sig_probe] FAIL: {msg}");
            std::process::exit(1);
        }
        None => println!("[sig_probe] PASS"),
    }
}

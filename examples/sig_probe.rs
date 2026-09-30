//! sig_probe：绑定面签名契约锚点（批次 A，无头——零窗口/GPU 依赖）。
//!
//! draw 族 FromArgs 收参的可执行规格：正路径各形态（位置/kwargs/缺省值/
//! 序列整形）与负路径（缺参/越界/非 int/未知 kwarg）在 `sig_probe.py`
//! 锁定——签名回归先跑本探针，再上真机批次。
//!
//! 运行：`cargo run -p pygame-rs --example sig_probe`
//! 预期：`[sig_probe] PASS`；Exit 1 = 契约破坏。

use pygame_rs::rpy;

fn main() {
    let interp = rpy::new_interpreter();
    let err = interp.enter(|vm| -> Option<String> {
        let scope = vm.new_scope_with_builtins();
        rpy::bridge::register(vm, &scope);
        rpy::install_pygame(vm); // 原生 pygame 模块注入（enter 内，脚本前）
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
    rpy::bridge::mixer_quit();
    match err {
        Some(msg) => {
            println!("[sig_probe] FAIL: {msg}");
            std::process::exit(1);
        }
        None => println!("[sig_probe] PASS"),
    }
}

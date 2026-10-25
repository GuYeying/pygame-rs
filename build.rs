//! 构建脚本：Windows(MSVC) 示例可执行文件抬高主线程栈 Reserve。
//!
//! 根因（批次 A 收尾发现，详见 reference/安卓APK打包lib_name坑与真机诊断手册.md
//! §九-6）：RP 的 `with_recursion` 守卫（"while hashing" 等 RecursionError）
//! 不看 `recursion_limit`，而是量真实 C 栈指针——软限位 = 栈底 + margin
//! （debug 构建 margin = 256KB，vm/mod.rs `STACK_MARGIN_BYTES`）。Windows
//! 主线程栈 Reserve 默认仅 1MB（PE 头 /STACK），debug 构建引导冻结 importlib
//! 的原生帧深 >768KB 即撞线 → `essential initialization failed`。release
//! margin 仅 64KB 故无恙。
//!
//! 处置：link-arg 抬 Reserve 到 16MB（Reserve ≠ Commit，不用不占内存；
//! CPython 的 python.exe 同款做法）。仅 windows-msvc 示例目标；release 一并
//! 生效无害。Android（cdylib）与 wasm 不经此分支。

fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && env == "msvc" {
        // bins + examples 双覆盖（产品化批次：pypg/pygame-runner 为 bin
        // 目标——原 examples 专属链接参数不覆盖 bin，debug 栈撞线）
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
        println!("cargo:rustc-link-arg-examples=/STACK:16777216");
    }
}

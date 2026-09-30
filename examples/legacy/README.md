# 退役示例停放区（不在 cargo 发现路径，不参与编译）

- `rp_main_loop.rs` —— "主循环归 rpy" 形态（批次十七）。set_mode 调用
  缺失（编辑丢失，真机黑屏挂起，见手册 §六）+ Web get_screen panic；
  形态已被 binding_probe（宿主持循环）取代，2026-09-29 批次 E 收尾退役。
- `window_test.rs` —— 旧扁平别名写法演示（fill/rect/flip 全局）。
  依赖的 bridge 旧别名面已随本批退役同步移除；新写法 = binding_probe。

两文件为 **停用存档**（pygame-rs 无 git，防永久丢失）；复活需在
Cargo.toml 重挂 [[example]] 并恢复 bridge 旧别名面。

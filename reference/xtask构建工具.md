# xtask 构建工具介绍

> 2026-09-17 落地。位于 `xtask/` crate，经 cargo 别名调用：`cargo xtask <命令>`。
> Android 构建与部署的操作细节另见 `reference/android构建与运行指南.md`。

## 一、作用：为什么要有它

Starfish 的构建需求早已超出 `cargo build` 的范围：

| 需求 | 纯 cargo 做不到的事 |
|---|---|
| Android 出包 | 编译（cargo ndk）之后还要 **aapt2 打包 → 内嵌 .so/dex → 签名 → adb 部署** 四步手工流程 |
| 特性勾选 | 需要把 `--features` 与"哪些特性在目标平台真正可用"的知识结合 |
| 多模块扩展 | 每加一个模块/示例，不应要求开发者记一套新流程 |

传统做法是 shell/Python 脚本（项目曾用 `scripts/android_run_example.sh`），
但脚本方案有固有短板：平台依赖（bash 在 Windows 需 git-bash）、无法类型检查、
逻辑分散、不可复用。**xtask 模式**是 Rust 社区的主流答案：构建工具本身就是
一个 Rust crate——编译型路线的"自举"：**用 starfish 的构建工具开发 starfish**。

## 二、当前使用

```bash
# 列出全部示例 + Android 支持注册表
cargo xtask list

# Android 全链路：编译 → 打包 APK → 签名 → adb 部署（无设备则仅出包）
cargo xtask android 03_texture

# 仅构建出 APK，不装机（产物在 target/android-apk/）
cargo xtask android 16_dialog --build

# 模拟器用 x86_64（免 ARM 转译层）
cargo xtask android 15_empty_window --abi x86_64

# 横竖屏（默认 landscape；可选 portrait / fullSensor / sensorLandscape 等）
cargo xtask android 13_video_decode --orientation portrait

# 特性勾选：追加 / 从零勾选
cargo xtask android 15_empty_window --features dialog,gfx
cargo xtask android 15_empty_window --no-default-features --features dialog,gfx
```

智能行为：
- **示例名自动解析**：传 `03_texture` 自动命中双注册的 `03_texture_android`
  （Android 构建只认 cdylib 条目，选错会明确报错）
- **无设备自动降级**为仅出包，不会卡死在 adb
- 旧 bash 脚本 `scripts/android_run_example.sh` 兼容保留（功能等价）

## 三、内部机制（四步管线）

```
cargo xtask android <示例>
   │
   ├─ [1/4] cargo ndk --platform 26 -t <triple> build --release --example <名>
   │         （API 26 = cpal AAudio 硬性下限；产物 .so 自动落 jniLibs/<ABI>/）
   ├─ [2/4] aapt2 link（android/AndroidManifest.xml 模板，
   │         替换 __LIB_NAME__ / __ORIENTATION__ 占位符）
   │         → 内嵌 lib/<ABI>/*.so + robius classes.dex（dialog 模块，缺失仅告警）
   ├─ [3/4] apksigner 签名（debug.keystore，Windows 走 cmd /c 包 .bat）
   └─ [4/4] adb 部署 + 启动（检测到设备才执行）
```

工具路径全部自动发现：`ANDROID_HOME` → build-tools 取最高版本 →
platforms 取最高 API 的 android.jar（`ANDROID_JAR` 环境变量可覆盖）。

## 四、扩展指南

### 新增 Rust 模块（特性）

在 `xtask/src/main.rs` 的注册表加一行：

```rust
const FEATURE_ANDROID_SUPPORT: &[(&str, Support)] = &[
    ("gfx", Support::Ok),
    // ...
    ("你的新模块", Support::Ok),      // 或 Support::Stub 表示空实现占位
];
```

构建时自动产生勾选警示/提示，未登记的特性照常透传 cargo（附提醒）。

### 新增跨平台示例

1. 复制 `examples/basics/15_empty_window.rs`（同源双注册模板），
   在三个落点填模块代码；
2. `Cargo.toml` 注册两条（同一路径）：

```toml
[[example]]
name = "xx_yy"
path = "examples/xx/yy.rs"

[[example]]
name = "xx_yy_android"
path = "examples/xx/yy.rs"
crate-type = ["cdylib"]   # NativeActivity 需 dlopen 的共享库
```

3. `cargo xtask android xx_yy`。

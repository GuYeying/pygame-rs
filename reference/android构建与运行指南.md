# Android 构建与运行指南

> 2026-09-17 实测打通（wgpu 30 / winit 0.30 `android-native-activity` / cargo-ndk 4.1.2 / NDK r30）。
> 姊妹篇：`reference/wasm编译与运行指南.md`。

## 前置（一次性）

```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
```

- 环境变量：`ANDROID_HOME`（如 `D:\SDK\Android`）、`ANDROID_JAR`（robius/dialog 需要，如 `...\platforms\android-37.0\android.jar`）。
- `adb`：Android SDK platform-tools 在 PATH。
- 调试密钥库 `%USERPROFILE%\.android\debug.keystore`（装过 Android Studio 即有，口令 `android`）。没有则：
  `keytool -genkeypair -v -keystore "%USERPROFILE%\.android\debug.keystore" -storepass android -keypass android -alias androiddebugkey -dname "CN=Android Debug,O=Android,C=US" -keyalg RSA -validity 10000`

## 一键跑示例

**主工具（Rust 原生 xtask，跨平台、支持特性勾选）**：

```bash
cargo xtask list                                   # 全部示例 + Android 支持注册表
cargo xtask android 03_texture                     # 自动解析 03_texture_android，构建→打包→部署
cargo xtask android 16_dialog --build              # 仅出 APK 不装机
cargo xtask android 15_empty_window --abi x86_64   # 指定 ABI（模拟器）
cargo xtask android 13_video_decode --orientation portrait        # 横竖屏（默认 landscape）
cargo xtask android 15_empty_window --no-default-features --features dialog,gfx   # 特性勾选
adb logcat -s RustStdoutStderr RustLog RustPanic AndroidRuntime:E
```

- **特性勾选**：`--features` 追加、`--no-default-features` 剔除；Android 支持状态登记在
  `xtask/src/main.rs` 的 `FEATURE_ANDROID_SUPPORT` 表——**未来新增 Rust 模块在此加一行**。
- **示例解析**：`03_texture` 自动找双注册的 `03_texture_android`（cdylib）条目。
- dex（dialog）/ .so 并入、apksigner（Windows 走 `cmd /c` 包 .bat）全部内置。

**兼容保留**：`scripts/android_run_example.sh <示例名> [--build] [abi]`（bash 版，功能等价；
Windows 上依赖 git-bash）。新用法以 xtask 为准。

## 分步（等价于脚本内部）

1. **编译**（cargo-ndk 会把产物拷到 `jniLibs/arm64-v8a/`）：
   ```bash
   cargo ndk --platform 26 -t arm64-v8a -o ./jniLibs build --release --example 02_triangles_android
   ```
2. **打包**：`aapt2 link -I <android.jar> --manifest <替换过 __LIB_NAME__ 的清单> -o app.unsigned.apk`，
   再把 `.so` 以 ZIP_DEFLATED 压缩方式塞进 `lib/arm64-v8a/` 条目（脚本用 Python zipfile）。
3. **签名**：`apksigner sign --ks debug.keystore --ks-pass pass:android ...`。
4. **装机**：`adb install -r xxx.apk && adb shell am start -n com.starfish.test/android.app.NativeActivity`。

## 新增一个 Android 示例怎么接入

**推荐：直接复制跨平台空窗口模板 `examples/basics/15_empty_window.rs`**——单源文件双平台，
内置后端可视化（背景色=后端：深蓝 Vulkan / 深绿 GLES）、`FORCE_BACKEND` 强制后端开关、
Resized 尺寸自愈、模块代码落点注释。接入步骤：

1. 复制该文件为 `examples/xx/yy.rs`，在三个落点填模块代码；
2. `Cargo.toml` 同源文件双注册（**bin 不能与 cdylib 混用**，cargo 会拒绝
   `crate-type = ["cdylib","bin"]`——一个文件注册两条，桌面/Android 各取所需）：
   ```toml
   [[example]]
   name = "yy"
   path = "examples/xx/yy.rs"

   [[example]]
   name = "yy_android"
   path = "examples/xx/yy.rs"
   crate-type = ["cdylib"]     # cdylib 不可省（cargo-ndk 4.x 不再自动转换）
   ```
3. `./scripts/android_run_example.sh yy_android`。

> **2026-09-19 起（推荐）**：新示例用统一入口宏 `starfish::app_entry!(App::new(),
> WindowConfig::new(..))` 一行覆盖全平台——宏生成的 `android_main` 只做
> "捕获 AndroidApp 到 `base::platform` 全局槽 → 调 `main`"，而 **`run` 三平台
> 同名**（Android 分支从槽取句柄完成 android-activity 引导 + 私有目录自动
> 注入），应用代码零 cfg。probe 家族（`examples/probe/`）即此形态的范本；
> `run_android` 仅作旧示例手写 android_main 的兼容垫片。

安卓专属写法（旧模板，已被 app_entry! 吸收）：android 入口在 `#[cfg(target_os = "android")] mod entry` 内，
`#[unsafe(no_mangle)] fn android_main(app: AndroidApp)` → `run_android(...)`；
Android 返回键已由引擎翻译为 `CloseRequested`（app.rs translate，返回 = 退出），
应用按普通"关闭窗口"处理即可。

真机（鸿蒙 NEXT + 卓易通容器）已实测跑通：`target/android-apk/*.apk` 直接拷机、
"使用卓易通打开"安装。

## APK 模板（android/AndroidManifest.xml）

- **NativeActivity 无 Java 模板**三要素：`android:hasCode="false"`（无 dex）、
  activity 名 `android.app.NativeActivity` + `meta-data android.app.lib_name`（= 示例名，即 dlopen 的 `lib<名>.so`）、
  `android:exported="true"`（Android 12+ 带 intent-filter 的 activity 强制）。
- `extractNativeLibs="true"` + `.so` 压缩入包 → 安装时解出，**免 zipalign 页对齐**。
- `minSdk 26`：cpal 的 AAudio 硬性下限；`glEsVersion 0x00030000`：wgpu GLES 兜底（Vulkan 设备自动走 Vulkan）。

## 关键坑位（全是实测踩过的）

1. **cargo ndk 必须 `--platform 26`**：默认 21，链接报 `unable to find library -laaudio`（NDK 的
   `libaaudio.so` 仅 API 26+ 提供）。
2. **平台 flag 是大写 `-P` / `--platform`**：小写 `-p` 被 cargo-ndk 4.x 透传给 cargo 当 package 名，panic
   `unknown package: 26`。
3. **`-o` 目录会追加一层 ABI 子目录**：`-o ./jniLibs/arm64-v8a` 会产出
   `jniLibs/arm64-v8a/arm64-v8a/*.so`；给脚本/gradle 用应写 `-o ./jniLibs`。
4. **Android 示例必须 `[[example]] crate-type = ["cdylib"]`**：cargo-ndk 4.x 不再把 bin/example 自动转成
   cdylib（只拷贝 target 本身是 cdylib 的产物）；否则产出 PIE 可执行文件，NativeActivity 无法 dlopen。
   `-C crate-type=cdylib` 不是稳定 rustc flag；`cargo rustc --example X --crate-type cdylib` 会被 cargo 拒绝
   （bin 系目标恒为 bin）——只有 `[[example]]` 段的 `crate-type` 字段（cargo 1.61+）是正路。
5. **`#[no_mangle]` 在 edition 2024 报错**：必须写 `#[unsafe(no_mangle)]`。
6. **Windows build-tools 里 `apksigner` 是 `.bat`**：git-bash 直接调 `apksigner` 找不到（脚本已按 uname 分派）。
7. **Android 专属编译错误桌面查不出**（`#[cfg(android)]` 代码）：用
   `cargo check --target aarch64-linux-android --lib` 提前捕获。
8. Rust 的 stdout/stderr（含 panic 信息）经 android-activity 重定向到 logcat tag **`RustStdoutStderr`**；
   log crate 的日志（wgpu/hal 内部 `log::error!` 等）默认**完全不可见**——示例可装
   `android_logger` 桥（见 02_triangles_android.rs，⚠️ 仅 x86_64 原生环境，见下节）。
9. **`view_formats` 能力掩码（已修，settings.rs）**：Unorm↔Srgb 表面视图重解释需
   `DownlevelFlags::SURFACE_VIEW_FORMATS`，**GLES/WebGL 与 Android Vulkan 均不支持**（wgpu 明确标注）——
   configure 直接 panic。`SurfaceSettings::to_wgpu` 已按 `adapter.get_downlevel_capabilities()` 运行时掩码
   （Web 恒置空）。症状：`In Surface::configure / Downlevel flags ... not supported`。
10. **Android 启动门必须验句柄（已修，app.rs）**：winit 在 Android 的 `inner_size()` 返回**显示器尺寸**
    （恒非零，"等首个有效尺寸"的门形同虚设）；ANativeWindow 经 onNativeWindowCreated 异步送达，
    过早建 GPU 表面 → `configure: Invalid surface`。app.rs 启动门在 Android 上追加
    `HasWindowHandle::window_handle().is_ok()` 探测（winit 未就绪时返回 `Err(Unavailable)`）。
10b. **横竖屏**：manifest 占位 `__ORIENTATION__`，xtask `--orientation` 指定（默认 `landscape`；
     可选 portrait/fullSensor/sensorLandscape 等）。configChanges 已声明 → 旋转不重建
     Activity，表面随 Resized 自愈（模板/新示例已内置 surface.resize）。
10c. **GLES 采样驱动坑（已规避，04/05）**：部分 GLES 驱动对 **NPOT 纹理 + Repeat 寻址**
     采样异常（单面呈"单像素拉伸"涂抹）。立方体 UV 仅 0..1，Repeat 无意义——采样器一律
     `ClampToEdge`（桌面视觉无差，规避驱动病）。
11. **每个进程只允许一次 winit EventLoop（已修，app.rs）**：Android 系统在 activity 结束后保留缓存
    进程；桌面图标再进时同进程起第二个实例，二次 `android_main` → `RecreationAttempt` panic
    （症状：返回退出后再点图标闪退，后台划掉才正常）。`run_android` 已在事件循环结束后显式
    `std::process::exit(0)`（对齐桌面"run 返回即进程结束"语义；`singleInstance` 挡不住该场景，
    因为那是合法的新实例）。

## 示例 Android 覆盖与模块审计（2026-09-17）

**同源双注册约定**：一个示例文件注册两条 `[[example]]`（桌面 bin + `*_android` cdylib，
bin 不能与 cdylib 混用故拆两条）；资源装载 cfg 分家——桌面读 `resources/` 相对路径，
Android `include_str!/include_bytes!` 内嵌（渲染/字体）或内嵌后落盘应用私有目录
（`AndroidApp::internal_data_path()`，音频/视频——`SoundData`/`VideoModule` 仅有文件构造）。

| 示例 | Android 包 | 模块覆盖 | 说明 |
|---|---|---|---|
| 02_triangles_android | ✅ | 渲染/循环/窗口/返回退出 | 已实机验证（卓易通） |
| 15_empty_window_android | ✅ | 模板骨架 + 后端可视化（背景色=Vulkan蓝/GL绿）+ FORCE_BACKEND | 实机 Vulkan 验证载体 |
| 16_dialog_android | ✅ | **dialog**（robius 选择器/保存） | 需 classes.dex（脚本自动并入） |
| 03_texture_android | ✅ | 纹理管线 | 内嵌 wall.jpg + shader |
| 04_coord_system_android | ✅ | MVP/索引网格/纹理 | 内嵌 container.jpg |
| 05_storage_cube_android | ✅ | StorageBuffer 实例化/3D | 桌面 Esc/鼠标交互触屏无效 |
| 06_draw_gfx_android | ✅ | gfx 全形状（自包含，无资源） | |
| 07_draw_text_android | ✅ | **font**（Font::from_bytes 内嵌字体） | |
| 08_play_sound_android | ✅ | **audio**（SFX/BGM/效果器） | 窗口化（绿=播放中）；内嵌 wav→私有目录 |
| 09_play_music_stream_android | ✅ | **audio**（流式 BGM/排队/分组） | 窗口化；内嵌 mp3/wav≈10MB |
| 10_record_mic_android | ✅ | **audio** 录音（AudioRecorder） | 窗口化（橙=录音中/绿=已保存/红=失败不闪退）；⚠️ 需 RECORD_AUDIO 已授权 |
| Web 录音（无独立示例） | ⏳ | **audio** 采集自持后端（`audio/device_web.rs`，web-sys getUserMedia+ScriptProcessor） | cpal WebAudio 无输入实现（上游写死 Err）；公开 API 不变，cpal 支持后删单文件即回退 |
| 13_video_decode_android | ✅ 实机验证 | **video**（MediaCodec 硬解 + ndk_context 注入链路） | 内嵌 mp4→私有目录；视频播放/泵失败紫屏/结束自动退出 |
| 20_net_probe（Android） | ✅ | **net**（UDP 广播发现 + TCP/UDP echo，服务器见 examples/server/server.py） | 需手机与 PC 同局域网 |
| 01_hello_world / 11_web / 14_gamepad | ❌ 不打包 | — | 纯控制台 / wasm 专属 / gamepad Android 空实现占位 |

模块状态结论：渲染/窗口/循环/audio/font/gfx/video/dialog 九大件 Android 实现**代码级完整**
（video 依赖 ndk_context 由 android-activity 自动注入；dialog 依赖 APK 内 dex，管线已处理）；
**gamepad 为 Android 空实现占位**（文档既定）；net 走 std::socket（INTERNET 权限已入清单，
暂无专属示例——15 模板可填）。

**卓易通容器两项故障（2026-09-18 已修复，见批次 17/18 日志）：**
- **dialog / 录音权限同根因**：android-activity 往 ndk-context 存的是 **Application**
  （init.rs get_application），而 requestPermissions / getFragmentManager 是
  **Activity 独有方法**——.Application 上调用即抛异常（17 探针屏显抓到
  `no non-static method "Landroid/app/Application;.requestPermissions..."`）。
  修复：`run_android` 把 ndk-context 换成真实 NativeActivity（`activity_as_ptr()`）。
  修复后 dialog 文件选择实机 ✅。
- **录音权限**：`ensure_permission` 走同一修复路径，授权框在容器内待复测；
  未授权时红屏不闪退，可在容器设置手动授权。
- **SAF 期间卡顿**：阻塞式等待停摆事件泵——已改轮询式 Job API
  （`pick_file_start` / `save_bytes_start` + `try_result`），主线程全程保持事件循环。

## 模拟器已知限制（2026-09-17，Android 17 / sdk_gphone16k_x86_64 实测）

两条故障路径均**不是 starfish 代码问题**（根因均已定位到源码级），实机不受影响：

| 路径 | 故障 | 根因 |
|---|---|---|
| **arm64 .so + Berberis 转译** | 启动即 panic `Expected an exception after ExceptionCheck`（jni-0.22.4 env.rs:706） | Berberis 在 JNI 异常状态传递上丢状态：`find_class` 失败后 `ExceptionCheck` 报有异常挂起，紧接取异常却返回 null → jni 捕获断言 panic。android-activity 0.6 / jni 0.22 的类代理机制在转译层下必踩 |
| **x86_64 原生 .so** | `wgpu_hal::gles::egl: Error in create_window_surface: BadAlloc` | 模拟器 GLES 透传栈（"Android Emulator OpenGL ES Translator"）对有效 ANativeWindow 也 `eglCreateWindowSurface` 失败；模拟器未提供 Vulkan，wgpu 落到 Gl 后无路可走 |

**结论**：模拟器上用 **x86_64 原生包**（`./scripts/android_run_example.sh <名> --build x86_64`），
若仍 BadAlloc，把 AVD 的 Graphics 改为 **Software（SwiftShader）** 重启模拟器再试。
**最终验证以实机为准**（两故障均属模拟器栈，实机预计直接通过）。

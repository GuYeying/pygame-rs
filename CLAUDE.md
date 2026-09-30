# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 语言约定

项目文档、代码注释、更新日志全部使用**中文**（对齐上游 starfish 仓库惯例）。

## 项目概述

pygame-rs 是 **starfish 游戏引擎的 Python 绑定层**（独立 crate）：内嵌
RustPython 解释器 + pygame 绑定面。引擎本体在**同级目录** `../starfish/`，
本 crate 以 `path = "../starfish"` 单向引用。

- **依赖单向红线**：pygame-rs → starfish；starfish 永不感知本 crate，
  starfish 本体零 Python 资产。
- **三平台**：Windows 桌面 / Web（wasm32-unknown-unknown）/ Android（arm64 真机，
  鸿蒙 NEXT + 卓易通容器为实测环境）。
- **RustPython**：vendor 快照 0.6.0-dev（commit 23fcb8d9，2026-09-22）在
  `src/rpy/dependencies/RustPython-main/`，含三处 android 补丁（libffi 剔除 /
  ctypes 门控 / static_cell 全局单元）。Cargo.toml 的特性表（`default-features = false`
  + `freeze-stdlib` 等）是 wasm/android 可编译的前提，勿随意改回。
- 当前进度：M0（嵌入冒烟）已通；批次 A（draw 族 FromArgs 签名整形）、
  批次 B（原生 pygame 模块 + pyclass，`pygame_shim.py` 退役）、
  批次 C（Rect 直通句柄 + typed K_* + mouse）、批次 D（font 文字上屏 +
  绑定面拆分 pygame_impls）、批次 E（pygame.mixer + 旧示例退役）、
  批次 I（pygame.touch 多点触控查询）、批次 J（pygame.gamepad 手柄查询 +
  引擎侧 gamepad facade + gilrs TLS 泄漏修）、批次 K（math 3D：Vector3/
  Quaternion/Mat4 + quat_*/mat4_* 度数制入口，glam 直绑）、批次 L（draw.
  ellipse/arc + transform.smoothscale 别名 + Surface.set_alpha 表面级
  alpha）、批次 M（pygame.io 同步读写 + pygame.net TCP/UDP 轮询面 +
  pygame.dialog 桌面模态——pollster block_on，Web/移动边界报错）、
  批次 N（向量算术协议：Vector2/3 的 AsNumber 槽——`+ - * /` `+=` `-=`
  取负；加减仅向量、乘除标量/向量元素级 glam 语义、标量×向量对称分派）、
  批次 O（display flags 落地：set_mode 位集/序列 → apply_window_request
  **尺寸+旗标全权威**运行时生效；批次 33 扩：后端声明位 VULKAN/DIRECTX/
  METAL/OPENGLES/OPENGL→GpuSettings.backends + 不可用隐式退化回全后端、
  TRANSPARENT 创建期透明 + Screen 透明底清屏、set_opacity 窗口透明度
  ——桌宠形态 v1；创建期/运行期边界定约见 changelog 批次三十三）、
  批次 P（Quaternion/Mat4 算术：
  `*` 组合/标量缩放 + 取负，AsNumber 槽）、mouse/key 完整度补全
  （get_rel/可见态/聚焦态/get_mods/KMOD_*/name + set_pos·set_repeat
  垫底）、**gfxdraw 兼容签名面**（SDL_gfx 惯例：目标会话在前/坐标
  分离/颜色在后——aa* 为兼容别名，实现同一 batch；rounded* 垫底
  直角）已完成；**cursors 剪除**（2026-09-30 定案：光标图标桌面专属
  不跨平台，set_cursor_visible 保留于 base）。
- **逐模块独立测试阶段**（进行中）：共享宿主壳 `rpy::testhost::run(
  script, tag)`（set_mode/铺底/CJK 三平台预载/装配/事件泵/帧循环/收口
  固化），每模块一个 `examples/<模块>_test.rs + .py`（[模块-test] 行
  逐项断言 + 样张上屏；双注册 `<模块>_test` + `<模块>_test_android`，
  build_web.py REGISTRY 加窗口类条目）。已完成：font_test（11 项断言
  ——Font 构造**双参定约**：单参在参数层 TypeError，坏字节 ValueError
  需带 size）。binding_probe / sig_probe 保留不动。
  进度主线见 `src/lib.rs` 顶部注释与 `../starfish/doc/PROJECT_STATUS.md`。

## 常用命令

```bash
# 本 crate 无单元测试——验证方式 = 跑示例（Windows debug 已可用：build.rs
#   抬主线程栈 Reserve 修掉了引导 RecursionError；性能验收仍以 --release
#   为准，见 reference 手册 §九-6）：
cargo run --release -p pygame-rs --example rp_hello        # M0 冒烟（RP 创世纪/语言特性/冻结 stdlib）
cargo run --release -p pygame-rs --example sig_probe       # 签名契约锚点（无头零窗口，先于真机跑）
cargo run --release -p pygame-rs --example binding_probe   # M1 绑定面验收（标准 pygame 写法）

# Android APK 统一打包（唯一打包链；xtask 已退役）
python scripts/build_apk.py binding_probe                  # 本 crate 示例（默认 --dir 本目录）
python scripts/build_apk.py hello --dir ..                 # starfish 根 crate 示例共用本脚本
# 链路：cargo ndk（RP 恒 release 铁律）→ llvm-strip → aapt2 重造 base（lib_name 对齐示例名）
#   → 组装（.so 恒 ZIP_DEFLATED）→ zipalign → apksigner；产物 target/android-apk/<名>_android.apk
# 需 ANDROID_HOME / ANDROID_NDK_ROOT（未设时按常见路径探测，如 D:/SDK/Android）
# 模拟器冒烟必须 x86_64 原生包：--abi x86_64（arm64 包被 berberis 翻译层跑，结果不可信）

cargo check --target aarch64-linux-android --lib           # 提前捕获 Android 专属编译错误（桌面查不出）
```

Web：`wasm-bindgen-cli` 版本必须与 Cargo.lock 严格一致（当前 0.2.127，不一致报
"not compatible with version of the wasm-bindgen in the dependency tree"）。
**本 crate 示例的 Web 构建/测试统一走 `python scripts/build_web.py <名>`
（wasm32 构建 + 胶水 + HTML 加载页，产物落本仓库 `web/`；`--all` 全量；
页面带【pygame-rs】归属标识）**——归属规则：pygame-rs 的 demo 产物只进
pygame-rs/web/，starfish 示例的进 starfish/web/，两边不混放。
构建 + 胶水 + 本地服务流程详见 `reference/wasm编译与运行指南.md`。

## 架构

Python 触达 Rust 的分层链（自上而下，单向）：

```
用户 Python 脚本（标准 import pygame）
  → 原生 pygame 模块（src/rpy/native_pygame.rs：#[pymodule] 树
    display/event/time/key/mouse/gamepad/touch/font/mixer/transform/image/
    math/sndarray/version/io/net/dialog/gfxdraw + Screen/Session/Event/
    Rect/Key/Pressed/Font/Sound/Surface/Vector2/Vector3/Quaternion/Mat4/
    TCP/UDP/PickedFile pyclass + K_* 键表 + gamepad BTN_*/AXIS_* 整数
    常量（标准布局索引）；install_pygame() 注入 sys.modules——
    宿主壳在 interp.enter 内调用，勿放 init_hook；方法实现层拆分在
    src/rpy/pygame_impls.rs（impl 块与 struct 分离可行，同 crate 内合法）
    → sf_* / args（src/rpy/bridge.rs 引擎桥 + src/rpy/args.rs 收参层：
      draw 族 FromArgs 结构体收参 + 序列整形；bridge 全局 = sig_probe
      契约面）
      → starfish::pygame 兼容层（display/draw/event/time/rect/color/render）
        → starfish::base 引擎（wgpu + 自研 starfish-window）
```

- **RP 接触面收口**：与 RustPython 的一切交互只在 `src/rpy/`（`mod.rs` 的
  `new_interpreter()` = 解释器装配唯一入口；debug 构建须抬 recursion_limit=1000）。
- **生成器门**（主循环归调用方，pygbag 同款模式）：Python `game()` 生成器 =
  帧逻辑；Rust 宿主壳持主循环，每帧 `__next__` 步进一次，`yield` = 帧边界
  （Web 无阻塞模型下唯一可行形态）。定约：脚本只定义生成器，壳负责实例化
  （脚本内模块级自调用会报 `'generator' object is not callable`）。
- **ADR-5 渲染会话**：`with screen.render():` 由原生 Session pyclass 翻译为
  `bridge::session_begin` / `session_end`；RenderPass 存 thread-local，绘制
  方法路由到活跃会话。**不提供 `pygame.draw.*(screen, ...)` 兼容形态**——
  双形态在 Surface/MRT 场景产生二义性（定约，勿"补全"）。
- **加新 API**：直接在 `native_pygame` 加 `#[pyfunction]`/`#[pymethod]`
  （pymethod 同样收 FromArgs 结构体，kwargs/缺省值可用），参数整形复用
  `args` 层；会话/事件队列等引擎侧状态归 `bridge.rs`（thread-local）。
- **示例双注册惯例**：一个源文件注册两条 `[[example]]`——桌面 `<名>`（bin）+
  Android `<名>_android`（`crate-type = ["cdylib"]`，cargo-ndk 4.x 不自动转换）。
- 入口统一 `starfish::app_entry!(run());` 一宏覆盖三平台（Web 要求用户 crate
  可见 wasm-bindgen）。

### RP 原生绑定坑（批次 A/B 后；完整清单见 reference 手册 §九）

- **正式 Python 面 = 原生 pygame 模块**（native_pygame）：`#[pymodule]` 树 +
  pyclass（Screen/Session/Event）。注入必须走 `rpy::install_pygame(vm)`
  （宿主壳在 `interp.enter` 内、脚本前调用）——**init_hook 时点注入会递归
  爆栈**（解释器初始化深处勿动模块机制，批次 B 实测）；
- **pymodule 宏备忘**：`pyattr`/`pyfunction`/`pymethod`/`pygetset` 是内部
  标记（无需 import，也不存在于宏根）；**子模块必须嵌套声明且不带
  `(sub)`**——`(sub)` 是"属性并入父模块"形态，不产生独立子模块；
  `with(协议)` 挂 **impl** 的 `#[pyclass]` 上；struct 的 `#[pyclass]` 配
  `#[pyattr]`（注册进模块）或 `no_attr`（自由类型）；
- **draw 族 FromArgs 结构体收参**（`args.rs` 做序列整形）：kwargs、
  缺省值（`#[pyarg(any, optional)]`）、颜色 `(r,g,b[,a])`、嵌套点列、
  **`pygame.Rect` 实例直通**（SfRect downcast 分支）全原生支持。
  **原生函数/pymethod 不支持 `&str` 形参**——用 `PyStrRef` + `to_str()`。
  签名契约回归：`cargo run --release --example sig_probe`；
- **mixer（批次 E）**：`pygame.mixer.Sound()` 无参 = 内嵌 bleep.wav
  （Font(None) 惯例延续）/ 路径 / bytes 三构造（starfish
  `SoundData::from_bytes`）；`play(loops)/stop/set_volume/get_length`；
  `mixer.music.load/load_bytes/play/stop/...`（starfish
  `music_load_bytes`）；**混音器在 bridge thread-local 保活（Drop 即停）**；
  **Web 每帧须 `bridge::audio_pump()`**（无解码线程）；**Web 音频手势
  定约**（浏览器自动播放策略）：`pygame.mixer.needs_gesture()` 暴露平台
  差异（cfg 判定，Web=True）——原生平台直接创建播放，Web 由脚本延后到
  首次手势（keydown/mousedown 均 User Activation）；**手势前的 play 类
  调用一律跳过**（不排队、静默丢弃；挡在 with_mixer 前——不建混音器/
  AudioContext/不占通道，debug 构建打一条日志）；首个手势经
  `bridge::pump()` 置位并 resume 已存在的 suspended ctx（兜显式 init
  提前的场景）；通道索引停止后回收复用；
- **Surface/transform/image（批次 F+，GPU 直通定约）**：
  `pygame.Surface((w,h))` pyclass（starfish Surface owned payload；构造/
  使用须在 display init 后——GPU 槽约束，与真 pygame 差异）；
  `get_size/fill/blit/get_rect` + **`with surface.render() as t:` 离屏
  绘制会话**（fill/rect/circle/line/polygon/blit；`__exit__` flush 打包
  ——Surface 内部单一 batch 无通道拆分问题，与 Screen 会话同语法）；`pygame.transform.flip_x/flip_y/scale/
  rotate/rotozoom`（GPU 采样）；`pygame.image.load(桌面)/
  load_from_bytes(Web ✓)/save(桌面专用)/get_extended`；
  `Font.render(...) -> Surface` 文字表面一等化。**已知差异**：rotate/
  rotozoom 角度用 ArgIntoFloat 兼收 int/float（f32 形参严格拒 int）；
  get_at/set_at/tostring 等 CPU 回读类接口按用户定约出栈（GPU 直通
  优先，需要时与 starfish 协商 readback pub 包装）；
  **静态 Surface 必须创建一次、循环内复用**——每帧新建/丢弃会与
  ADR-5 延迟提交冲突（纹理先于命令执行销毁 → 内容丢失/黑白帧交替
  闪烁，批次 F 真机实测）；
- **font（批次 D/G）**：`pygame.font.Font(None|"cjk"|路径|bytes, size)`：
  None = 内嵌 Antonio-Regular 47KB（拉丁，跨仓库 include_bytes 单源）；
  **"cjk" = 宿主 io 预加载的 CJK 字体**（bridge 字节缓存；Windows 宿主
  预载系统 msyh.ttc，Web 预载 web/fonts/cjk.ttf fetch，Android 预载
  /system/fonts 系统候选链（NotoSansCJK/HarmonyOS_Sans_SC/miui 等 7 候选，
  2026-09-30 真机验收通过），未预载时构造报
  ValueError → 脚本回退拉丁）；路径 = 磁盘任意字体（wasm 走 bytes）。
  文字上屏走 `Session.text(font, text, dest, color) -> Rect`（render+blit
  融合）或 `Font.render(...) -> Surface`（一等化，可 blit/transform）；
  `SysFont` 占位为默认字体。**已知边界**：缺字形静默跳过（无回退链）；
  字形缓存无上限（demo 规模无碍）。
- **K_* 键值是 typed `Key` 实例（非整数）**——引擎无 pygame 键码表，
  typed 是 starfish locals 既定决策；`e.key == pygame.K_x` 用 Key 值相等
  （Comparable），`Event.key` 自批次 C 起也是 Key 实例。108 键表在
  native_pygame `KEY_TABLE`，经父模块用户版 `module_exec` 注册；
- **pyclass 可变 payload 用 `Cell<T>`**（RP pymethod 无 `&mut self` 形态，
  内置类型一律内部可变性；Rect 是 Copy 故 `Cell<Rect>` 即可）；
- 旧扁平别名（fill/rect/flip/quit_requested）已随 rp_main_loop /
  window_test 退役（存档 `examples/legacy/`）——`new_function` 直注册
  形态的限制（7 参上限、tuple 平铺消费、Rust tuple/`Vec<(i32,i32)>` 无
  TryFromObject）随之消失；存档复活需重挂 [[example]] 并恢复别名面；
- 生成器脚本只定义不实例化，壳负责 `call`——模块级自调用
  （`game = game()`）报 `'generator' object is not callable`；
- `&VirtualMachine` 必须在原生函数形参**末位**（两形态通约）。

### Android 实机关键坑

- **启动后立即 set_mode**：启动路径禁止睡眠/长阻塞——延迟装配 × 横屏旋转
  （ANativeWindow 销毁重建）= `Invalid surface`；
- **set_mode 后立即 fill+flip 铺一帧底色**（批次 D）：surface 配置完成到
  首帧 present 之间（RP genesis 约 1s），swapchain 未初始化 buffer 在部分
  设备显示为**白**——主题黑只覆盖窗口创建前，铺底消除中间白闪；
- **诊断 TCP 必须 `connect_timeout(300ms)` + 一次失败即拉黑**（AtomicBool）——
  无超时阻塞 connect 在主线程 = ANR；
- **APK 秒退先查 lib_name**：`aapt2 dump xmltree --file AndroidManifest.xml <apk> |
  grep -A2 lib_name`，value 必须 = `<名>_android`（dlopen 精确匹配）；
- **GL 纹理 ID 复用串内容（transform 源表面必须保活）**：wgpu-GL 后端
  无桌面 wgpu 的资源生命周期托管——临时纹理（如 `Font.render` 的产物）
  用完即弃后 GL ID 立即复用，**已提交未执行的绘制采到"新纹理"的内容**
  （安卓实测：后建的 SMOOTHSCALE 行显示了先前的旋转文字图；桌面不复现）。
  规则：**transform.rotate/smoothscale 的源、以及一切"用完即丢"的 GPU
  表面，保活到安全点**（ADR-5 红线延伸；font_test.py `_keep` 名单为
  参考写法）。治本（wgpu-GL 提交期资源保活）为 starfish render 独立待办；
- 卓易通日志通道不可靠，可信序：hilog `ANCO CREATE/DIED` 进程寿命 → 模拟器
  logcat（逻辑层）→ 屏幕阶段色 → TCP（需同网段）→ 文件。

## 文档指引

- 本仓库 `reference/`：`三端构建与运行指南.md`（**pygame-rs 一站式构建入口**）、
  `安卓APK打包lib_name坑与真机诊断手册.md`（**接手安卓问题先读**）、
  `android构建与运行指南.md`、`wasm编译与运行指南.md`、
  `wasm高DPI深度附件脱节问题详解.md`、`wasm运行时生命周期与尺寸竞态问题详解.md`、
  `pygame绑定层API设计稿.md`（绑定工程唯一契约）、`pygame兼容层模块架构.md`。
- 上游 `../starfish/`：`CLAUDE.md`（引擎侧全景）、`doc/PROJECT_STATUS.md`、
  `doc/log/`（每日批次日志）、`reference/rustpython改动备忘录.md`（RP 三处
  android 补丁的位置与缘由；独立 crate 后该文件仍在 starfish 仓库，本仓库
  Cargo.toml/rpy 注释里的 `reference/rustpython改动备忘录.md` 指它）。

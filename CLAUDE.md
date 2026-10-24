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
  直角）、**with_batch 逃逸舱补全**（vertices/geometry 直通——
  topo/kind 白名单校验、GLES 退化展开由 push_geometry 承载）、
  **Color 提升为 class**（批次三十九：PyColor pyclass 直通 starfish
  Color——r/g/b/a 属性 + normalize/premul_alpha/lerp/grayscale/invert/
  correct_gamma + Color==tuple 兼容旧契约 + SfColor 直通分支）已完成；
  **cursors 剪除**（2026-09-30 定案：光标图标桌面专属
  不跨平台，set_cursor_visible 保留于 base）。
- **批次 34（2026-10-12，Claude 接手后首战）：全体系对接收口**——
  starfish pygame 层 26 模块绑定面补齐：
  - **新模块**：mask（Mask 全套：new/set_at/get_at/count/fill/invert/
    overlap/overlap_area/overlap_mask/scale/to_surface + from_surface
    ——影子快路径三端同步零回读）；permission（ensure + MICROPHONE/
    INTERNET 常量；Android MICROPHONE 阻塞授权语义**透传**）；video
    （VideoSystem/Video：open/update/size/position/ended/音视频开关
    ——接口面级，呈现集成随引擎纹理桥）；time.Clock（**上限节流**
    帧时钟，tick 返 ms——vsync 已达标不补睡，三端同语义）；
  - **render 深水区**：Surface 构造 `depth_buffer=True`（架构 §九.2
    定约落地——`depth=` bpp 语义禁用）+ `has_depth()`；Session
    `set_clip`/`set_camera_ortho`/`set_camera_perspective`；Surface
    会话 `set_clip`；**`pygame.targets([a, b])`** MRT 会话（§3.1 预言
    形态——引擎侧 `render_targets_arc` 'static 收口 + bridge 目标
    Arc 保活表，本 crate 外唯一引擎增补）；
  - **既有补面**：event poll/clear、display set_caption/get_init、
    io write_text、KMOD_* 常量；
  - **sprite pyclass 桥**：Sprite = 配置对象（at/tag/z/rotation/scale/
    alpha/visible 链式 + 多帧动画 set_fps），Group.add 物化配置并
    **回写世代 id 进 Python 对象**（Copy 纯值跨 GC 零负担）/remove/
    alive/count/position/move_by/set_*/with_tag/tick/draw（路由活跃
    会话——单形态定约）；
  - **探针 selftest 扩至 23 项**（新模块全量），桌面全绿；
  - **坑（绑定面铁律新增）**：①RP 同类型仅一个 `#[pyclass]` impl
    块——跨文件第二个 E0119，方法并入既有块；②pymethod 直写
    `Option<T>` 形参不做缺省（可省参须 FromArgs 结构体
    `#[pyarg(optional)]`）；③**pymethod panic 穿透 Python 异常层
    直接带崩进程**（sprite 首版索引空 vec 实测——绑定面禁 panic，
    索引/unwrap 前自查）；④Constructor Args 用 tuple 无法变参
    （一参缺省形态须 derive struct）。
- **批次 34 续：with_batch 直通口**——`blit_area(src, dest, area,
  color=None)`（push_quad_uv：UV 子区域贴图=pygame blit 的 area 语义）
  + `quad(texture, corners, uvs, color)`（push_quad_corners：任意四角）
  三会话同挂（Screen/SurfaceSession/TargetsSession——同类型单 impl 块
  约束下方法面三处镜像）；args 新增 `SfFPoint/SfFPoints`（f32 宽收，
  ArgIntoFloat 路径——角点亚像素坐标）；`push_geometry` 暂缓（需
  Python 侧 Geometry 构建器，随 v2 静态网格立项）。
- **批次 41（2026-10-18）：bridge 瘦身七步——状态归属移交 starfish**：
  架构原则定约——**行为一致为准**（绑定层与引擎 Rust 用户同一套行为）、
  bridge 只留"提供状态 + debug"的 RP 薄壳（错误映射/收参/downcast）、
  **禁为兼容在 bridge 造新接口**（starfish 接口形态不满意就去 starfish
  改；screen 进程唯一是例外）、example 可重写。落地：①活跃会话 →
  starfish `render::session`（双槽 + MRT 保活表收回引擎 +
  `NoActiveSession`）；②混音器 → starfish `pygame::mixer`（生命周期 +
  Web 手势门 + `beep()` + pump）；③事件 → `pygame::event` 队列化
  （SDL 模型 `pump()` 灌队列返 Quit，get/poll/clear 消费同队列；
  **ESC 退出语义归脚本**——宿主壳只认窗口关闭）；④字体预载 →
  `font::preload/preloaded`；⑤`DrawTarget::blit_area/push_points/
  push_points_indexed` default method（引擎版 blit_area 补齐源面
  flush 保险）。`sf_draw_*` 七件全局 + register + bridge 全部
  thread-local 引擎状态退役——**bridge 691→293 行**。顺带修复
  PySurface.fill/blit 注释遗留（selftest 复活 2 项）；sig_probe 的
  draw 族收参断言迁入 binding_probe 真会话（draw.positive/negative）。
  **同批定约修订：Web 自动播放策略改部署约定**——运行时手势门整体
  撤除（API 面 `pygame.mixer.needs_gesture()` 移除），build_web.py
  模板内建"▶ 点击启动"覆盖层兜底，三平台同代码。
- **逐模块独立测试阶段**（进行中）：共享宿主壳 `rpy::testhost::run(
  script, tag, transparent)`（set_mode/铺底/CJK 三平台预载/装配/事件泵
  （`event::pump` 直调）/帧循环/收口固化），每模块一个
  `examples/<模块>_test.rs + .py`（[模块-test] 行
  逐项断言 + 样张上屏；双注册 `<模块>_test` + `<模块>_test_android`，
  build_web.py REGISTRY 加窗口类条目）。已完成：font_test（15 项断言
  ——Font 构造**双参定约**：单参在参数层 TypeError，坏字节 ValueError
  需带 size）、transparent_test（桌宠形态三端）、draw_test（七项含
  vertices/geometry/gfxdraw）。binding_probe / sig_probe 保留不动。
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
**Web 窗口大小控制**（页面即权威，开发者自行调整）：canvas 占多大
（CSS）渲染就是多大——位图 = CSS 布局 1:1，dpr 不进引擎。产物页默认
`800×600`（与桌面 `set_mode` 一致）；改产物 HTML 里 canvas 的 CSS
宽高（如 `92vw/86vh` 铺满）、运行时改 `canvas.style`，或启用产物
script 里注释好的 `setGameSize(w, h)` 助手——引擎经 ResizeObserver
感知布局变化自动同步位图 + rescale。浏览器缩放（Ctrl+滚轮）是纯视图
变换（canvas 与页面一起缩放，渲染零扰动）。`screen.get_size()` 查询
当前渲染尺寸。
构建 + 胶水 + 本地服务流程详见 `reference/wasm编译与运行指南.md`。

## 架构

Python 触达 Rust 的分层链（自上而下，单向）：

```
用户 Python 脚本（标准 import pygame）
  → 原生 pygame 模块（src/rpy/native_pygame.rs：#[pymodule] 树
    display/event/time(+Clock)/key/mouse/gamepad/touch/font/mixer/transform/
    image/math/sndarray/version/io(+write_text)/net/dialog/gfxdraw/
    **mask/permission/video**/pygame.targets(MRT 会话) + Screen/Session/
    Event/Rect/Key/Pressed/Font/Sound/Surface/**Mask/VideoSystem/Video/
    Clock/Sprite/Group**/Vector2/Vector3/Quaternion/Mat4/TCP/UDP/
    PickedFile pyclass + K_* 键表 + KMOD_* + gamepad BTN_*/AXIS_* 整数
    常量（标准布局索引）；install_pygame() 注入 sys.modules——
    宿主壳在 interp.enter 内调用，勿放 init_hook；方法实现层拆分在
    src/rpy/pygame_impls.rs（impl 块与 struct 分离可行，同 crate 内合法；
    ⚠️ 同类型仅允许一个 #[pyclass] impl 块——跨文件第二个会 E0119）
    → sf_* / args（src/rpy/bridge.rs 引擎桥——纯 RP 薄壳：会话/混音器
      错误映射 + with_batch 收参壳 + devlog，批次 41 后零引擎状态；
      src/rpy/args.rs 收参层：draw 族 FromArgs 结构体收参 + 序列整形）
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
  `music_load_bytes`）；**混音器生命周期在 starfish `pygame::mixer`
  （批次 41 收编）**——thread-local 保活（Drop 即停）+ `with` 惰性
  8 通道（bridge::with_mixer 只剩 AudioError→ValueError 映射）；
  **Web 每帧须 `starfish::pygame::mixer::pump()`**（无解码线程；宿主壳
  帧循环调用）；**Web 自动播放策略 = 部署约定**（2026-10-18 批次 41
  修订——原运行时手势门整体撤除：`needs_gesture()` API、play 类
  调用前跳过检查、事件泵手势标记全删）。浏览器要求 AudioContext 创建
  落在用户手势调用栈内，由**宿主页面保证首次手势先行**：build_web.py
  模板内建"▶ 点击启动"覆盖层（wasm 启动挂进 click 回调，产物天然
  合规）；嵌入页场景必然先点击；独立页自行用 html+js+css 挡一层。
  三平台行为一致：音频随时可播，无钩子、无跳过路径；通道索引停止后
  回收复用；
- **批次 42（2026-10-24）：外接库渲染对接 v1**——rapier-starfish-debug
  与 egui-starfish-we 挂上 pygame 屏幕。**分层定约（2026-10-24 用户立约）**：
  接口核心**先落 starfish 的 pygame 模块（通用情况，不针对特定库设计）**，
  本 crate 只做绑定对接；egui/rapier 是"现役消费者"而非接口设计目标。
  绑定面**不 pygame 化**（第三方库等价物，各自原生词汇 + Python 难度控制）：
  - **引擎侧通用能力**（starfish 批次 2026-10-24，依赖单向红线不破）：
    `Screen::with_overlay`（LoadOp::Load 叠加通道——本层 batch 延迟提交
    无活 pass 可借，外接者在游戏会话 pack 后、flip 前直绘）+
    `event::add_raw_listener`/`remove_raw_listener`（翻译前原始事件
    多消费者镜像表——pygame 翻译有损，egui 要修饰键/滚轮动量）+
    `render::gpu_info` / `display::with_window` pub / Screen 格式采样
    访问器；
  - **`pygame.rapier`**（`pyrapier.rs`，2D v1）：PhysicsWorld/RigidBodyBuilder/
    ColliderBuilder（构造 kwargs 替代原生链式）/DebugDraw/LineSink/
    DebugLinesRenderer + collect；句柄 = 不透明 int（generation<<32|index）；
    坐标契约 = 米制 y 向上原点居中（与 pygame 像素系不同系）；
  - **`pygame.egui`**（`pyegui.rs`）：init（幂等，CJK 复用 `font::preloaded`
    预载链）/frame(callback)/draw/wants_pointer/wants_keyboard/shutdown；
    回调收 PyUi（egui Ui 直绘包装——CURRENT_UI 指针模式：拷出即还锁 +
    horizontal 前置值恢复，跨帧持有干净报错不悬垂）；帧序拆两调用点：
    frame=输入→UI→细分（CPU 侧），draw=update_texture→update_buffers→
    叠加通道 render→free+clear→platform_output（0.37 帧序契约，错误路径
    弃置产物也须先 clear delta——drop panic 实测）；init 经
    `event::add_raw_listener` 登记消费者、shutdown 凭 id 撤除；
  - **帧序定约**：`event 泵 → step_frame → collect → egui.frame →
    游戏会话 → rapier 调试线叠加 → egui.draw 叠加 → flip`（叠加在
    会话后=画家序）；
  - **API 形态修订（同日第三轮，用户四点反馈落地）**：
    ①**事件捕获显式化**——init 不再自动挂监听，`egui.capture(enable=True)`
    手动开启/撤除（pygame 翻译有损：KeyDown 不带修饰键，egui 快捷键需
    完整原始词汇——原始镜像仍走引擎 `add_raw_listener`，脚本层可见）；
    ②**嵌套布局 with 语法**——`with ui.horizontal():`（无参返回 UiRegion
    区域对象：begin=`Ui::new_child` 建子 Ui 换 CURRENT_UI、exit=`advance_
    cursor_after_rect` 回填父光标；世代守卫防跨帧持有，frame 收尾兜底
    回收漏配对区；回调形态并存）；③**draw 会话内语义**——`egui.draw()`
    改走引擎 `defer_overlay` 延迟排程（flip 时在全部游戏内容 pack 之后
    统一落屏），放 `with screen.render():` 块内/外皆正确（egui=HUD 顶层
    语义由结构保证；**⑥会话形态终版（同日第四轮）——`egui.frame`/`egui.draw`
    模块函数移除，合并为 `t.draw_ui(panel)`**（t = `with screen.render()`
    的会话对象，PyScreenSession 新方法；"画到哪个目标"由会话对象表达）。
    引擎侧 = 会话级泛型 `RenderPass::defer_overlay`（Surface/MRT v1 提示
    不支持）；draw_ui 内部 = 会话探针先行（会话外调用先报错——不消耗
    输入不建 UI）→ 构建（OOP 校验→输入→UI→细分）→ 经活跃会话排程；
    产物不再暂存运行时（构建即排程，覆盖残留/对账路径消失，仅构建错误
    路径保留 discard_output）；④updates_* 通用方案定约——`draw_ui` 单
    调用承载 egui 0.37 全管线（update_texture→update_buffers→render→
    free+clear→platform_output 是**硬协议**不可拆：顺序错=panic 实测），
    DeferredFrame Drop 兜底清 delta（排程未执行也 drop 安全）；用户可见
    面等真需求出现再开（用户纹理上传 v2）；⑤**OOP 门面**（同日追加）——
    **⑥会话形态 + 事件逐条显式喂（同日第四/五轮定约）**：
    - `t.draw_ui(panel)` / `t.draw_debug(renderer, sink)`（PyScreenSession
      会话方法，"画到哪个目标"由会话对象表达；引擎侧 = 会话级泛型
      `RenderPass::defer_overlay`，Surface/MRT v1 提示不支持）；draw_ui
      内部 = 会话探针先行（会话外调用先报错——不消耗输入不建 UI）→
      构建 → 经活跃会话排程；产物不暂存运行时（构建即排程）；
    - **事件逐条显式喂**——`egui.capture()`（引擎原始镜像）退役，
      `egui.event(e)` 在 `pygame.event.get()` 循环内逐条调用（不喂则
      egui 无输入——捕获行为在数据流层完全可见）；配套 **PyEvent 全保真
      重构**（载荷 = 引擎 typed 枚举原样持有；原扁平五槽丢
      repeat/text/mod/宽高——textinput 塞 key 槽、resized 塌缩 "other"），
      新 getset：`mod`（KMOD 位，真 pygame `e.mod` parity）/repeat/text/
      w/h；引擎 `Event::KeyDown/KeyUp` 补 `modifiers` 字段 +
      `key::mods_to_bits/bits_to_mods` 双向换算（egui 重建完整原始事件
      ——快捷键可用）；egui 无关变体（quit/touch/user 等）喂入为 no-op；`egui.frame(panel)`
    收 `pygame.egui.UiBase` 子类实例（isinstance 强制，鸭子类型不放行），
    面板实现 `draw(self, ui)`（基类 NotImplementedError 兜底"必须实现"），
    经实例属性捕获状态与游戏对象。**UiBase 是 Python 层类**（egui 模块
    module_exec 注入 exec——Rust pyclass 做基类会被 Constructor Args 卡
    死）；**嵌套 module_exec 必须首行手动调 `__init_attributes(vm, module)`**
    （用户版 exec 顶替宏自动版，漏调 = no_attr 类静态类型未初始化 panic，
    pygame 顶层同款先例）；弃置产物（负路径/覆盖残留/shutdown）先与渲染
    器**对账**（`discard_output`：set 喂入 + free 释放——egui 端已记账，
    直接丢会让后续部分更新命中"未分配纹理"panic，实测）；
  - **feature 细化（同日定案）**：`physics` 改基座（拉依赖无维度），
    `physics2d`/`physics3d` 转发维度（可双开——crate dim2/dim3 命名空间
    天然并存）；default = ["gui", "physics2d"]；3D 绑定面
    （`pygame.rapier3d`）随 3D 批次落地；
  - 验收：`cargo run --release -p pygame-rs --example rapier_test`
    （15/15）+ `egui_test`（15/15，OOP 门面版含负路径×2）桌面全 PASS；
    双注册含 `_android`。
    v1 边界：rapier 仅 2D；egui 事件不拦截/无 IME；Web/Android 编译门
    未开（arboard 与 rapier-parallel 的 wasm 问题——批次 43）。
- **批次 43（2026-10-24）：绑定层多文件拆分落地 + 事件帧尾清空**：
  - **native_pygame.rs（4347 行）退役** → `mods/` 每子模块一文件（24 个）
    + `pygame.rs`（顶层类型壳 pymodule）+ `registry.rs`（手工挂载序，
    替代父模块自动 submodule_inits）；pygame_impls.rs 退役：五类
    （PyKey/PySound/PySurface/PyRect/PyColor）的 `#[pyclass] impl` 块
    **搬回各自宿主 pymodule 块内**。RP 宏分工（拆分批次根因）：
    struct 上的 `#[pyclass]` 只生成 `PyClassDef`+`StaticType`，
    **`PyClassImpl`（方法/槽位半）只由 `#[pyclass] impl` 块生成**——
    struct+impl 同宿 pymodule 合法（pymodule 宏跳过 Item::Impl 不吃
    其属性；font.rs 先例），历史 E0119 是混合态重复块所致；
  - 拆分文件定约：文件头 `use rustpython_derive::{pymodule, pyclass,
    FromArgs};` + 尾部 `pub(crate) use xxx::module_def;` + 跨文件引用
    类型 re-export（event::PyEvent/PyKey、mixer::PySound、font::PyFont）；
    args.rs 收拢 FromArgs（HorizontalArgs 等）+ bridge.rs 收拢 exc_msg
    等辅助（RP 薄壳）；
  - **两个提取事故教训**：①`no_attr` 标记丢失（video.rs
    PyTargetsSession）→ pymodule 宏对该项 bail → 错误路径原样 re-emit
    后续属性泄漏 → "cannot find attribute pyfunction"（真实病灶不在
    报错行）；②`targets`/`PyTargetsSession` 段在 HEAD 里位于 **pygame
    父模块直接作用域**（video mod 闭合后、sprite mod 前），归宿
    pygame.rs 非 video.rs；
  - **registry 挂载序实测修正**：event 先于 key——key 的 K_* 挂载
    钩子（module_exec）取 PyKey 的 static type，PyKey 在 event 的
    `__init_attributes` 里创建（"event 最后"旧定约作废；首次运行即
    "static type has not been initialized" panic）；
  - **事件帧尾清空（用户定约，落引擎侧）**：`event::pump()` 起点排空
    上帧未消费事件——不管宿主用没用（不消费积压只会让陈旧 QUIT 延迟
    爆发）；`post_user` 改走 USER_PENDING 列 pump 时补发（**保一帧**：
    post 后下一帧 get/poll 可见；同帧 post→get 不可见——契约变更，
    引擎测试同步改写）；`clear()` 连带清 pending；
  - 验收：sig_probe 134/134 + binding_probe（ quit 驱动正常收尾）+
    rapier_test SUMMARY PASS（90 帧落体全过，**断言收口后保持窗续演
    到用户关窗/ESC——视觉验收期**）+ egui_test 300 帧烟测
    无异常（交互验收留人工）；
  - **验收轮补丁（用户实测反馈）**：①模块案例脚本 `pygame.event.K_*`
    落点错（K_* 全表在 `pygame.key`——`pygame.event.K_*` 不存在，短路
    之下平时不炸、**一按任意键即 AttributeError**）——rapier/draw/
    font/transparent 四脚本改 `pygame.key.K_ESCAPE`（egui/binding_probe
    原本就对）；②testhost 生成器耗尽（StopIteration）改记"脚本完成"，
    不再误报"脚本异常"。
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
  `mods/key.rs` `KEY_TABLE`，经 key 模块用户版 `module_exec` 注册到
  `pygame.key`（挂载序 event 先于 key——批次 43）；
- **pyclass 可变 payload 用 `Cell<T>`**（RP pymethod 无 `&mut self` 形态，
  内置类型一律内部可变性；Rect 是 Copy 故 `Cell<Rect>` 即可）；
- 旧扁平别名（fill/rect/flip/quit_requested）已随 rp_main_loop /
  window_test 退役并**剔除**（2026-10-24 清理——文件曾存档
  `examples/legacy/`，git 历史提交 cdf143e 仍可查；原留档理由"无 git
  防丢失"已失效）——`new_function` 直注册形态的限制（7 参上限、
  tuple 平铺消费、Rust tuple/`Vec<(i32,i32)>` 无 TryFromObject）随之消失；
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
- **批次 44（2026-10-24）：Rect 类型退役（引擎+绑定两侧联动）**——
  用户定约：GPU 渲染下 Rect 未达原版 pygame"描述 surface 信息"的等价
  能力，且**碰撞/触发器今后统一走物理引擎**：
  - starfish 侧（用户手工删 rect.rs + gui/physics 两 crate 后收尾）：
    **draw.rs 重写**——全部绘制函数改**无返回值**签名（"受影响 Rect"
    = CPU 脏矩形时代产物，GPU 全帧 present 无意义）；
    **`IRect = [i32; 4]` 数组别名**（`[x, y, w, h]`，用户定约：struct
    载体也不要）承载 clip/scissor/blit_area 内部几何；`DrawTarget`
    trait 的 fill/blit/blit_centered/blit_area 返回 `()`；
  - pygame-rs 侧：**`pygame.Rect` pyclass 删除**（struct + `#[pyclass]
    impl` + Constructor/Comparable/Representable + collidelist* 的
    rect_list 助手——碰撞查询面随之消失）；`Surface.blit/fill` 与三会话
    （Screen/Surface/Targets）的 fill/blit/blit_area/text 全部改无返回
    值；`Surface.get_rect` 删除；`args::SfRect` 去 Rect 直通分支
    （纯序列收参 `(x, y, w, h)`）；绘制调用点 `IRect::new(...)` →
    数组字面量 `[x, y, w, h]`；mods prelude 去 PyRect；
  - 脚本侧：sig_probe Rect 断言段删除（134→124 项）；binding_probe
    Rect 实例参数改元组（`r.rect(color, (1,2,3,4))`、`box = (x,270,60,60)`）；
  - 验收：sig_probe 124/124 + binding_probe selftest 全 PASS（0 FAIL）
    + rapier_test SUMMARY PASS + draw_test/egui_test 烟测零 panic；
    starfish workspace check 绿 + starfish-pygame 测试 49/51
    （唯一失败为存量字体路径环境问题）。

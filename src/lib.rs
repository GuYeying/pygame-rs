//! pygame-rs：starfish 的 Python 绑定层（RustPython 内嵌壳 + 绑定面）。
//!
//! 分层红线：starfish 本体**零 Python 资产**；本 crate 单向依赖
//! `starfish`（path = "../starfish"），RP（RustPython）接触面全部收在 [`rpy`]。
//!
//! 当前进度：M0（嵌入冒烟）已通；**批次 A**（draw 族 FromArgs 签名整形，
//! `args` 层序列整形）、**批次 B**（原生 `pygame` 模块 + Screen/Session/
//! Event pyclass，`pygame_shim.py` 退役）与**批次 C**（Rect 直通句柄 +
//! typed K_* 键集 + mouse）、**批次 D**（pygame.font：内嵌默认字体 +
//! Session.text 文字上屏；绑定面拆分 native_pygame/pygame_impls）与
//! **批次 E**（pygame.mixer：Sound 音效 + music 流式 BGM；starfish 侧补
//! SoundData::from_bytes / music_load_bytes 字节接口）、**批次 F**
//!（pygame.Surface 直通句柄 + transform + image + Font.render 一等化）与
//! **批次 G**（CJK 字体：bridge 字节缓存 + 宿主 io 预加载 + Font("cjk")
//! 构造解析链）、**批次 H**（sndarray：Sound ↔ 嵌套 list 互转）已完成。
//! **Web 音频手势定约**（2026-09-30）：`pygame.mixer.needs_gesture()`
//! 平台查询 + 手势前 play 跳过（不排队不占通道）+ starfish 侧
//! AudioContext 经 cpal 0.18 包装层解包捕获。
//! **批次 I**（pygame.touch 多点触控查询）、**批次 J**（pygame.gamepad
//! 手柄查询：整数标准布局常量 + starfish gamepad facade 宿主泵排水 +
//! gilrs TLS 泄漏修）、**批次 K**（math 3D：Vector3/Quaternion/Mat4 +
//! quat_*/mat4_* 度数制入口，glam 直绑）、**批次 L**（draw.ellipse/arc
//! 三路会话 + transform.smoothscale 别名 + Surface.set_alpha 表面级
//! alpha）已完成；binding_probe 含批次 I–L 运行时自测段（帧 1 一次，
//! [selftest] 行三端同源）。**批次 M**（pygame.io 同步读写——原生
//! pollster block_on / Web 边界报错；pygame.net TCP 消息分帧连接三平台
//! 统一轮询面 + UDP 原生专属；pygame.dialog 桌面模态 open_file/
//! save_bytes——移动/Web 异步 job 为后续批次）已完成。**批次 N**
//! （向量算术协议：Vector2/3 的 `+ - * /` `+=` `-=` 取负——AsNumber
//! 槽 + PyNumberMethods 静态表；加减仅向量、乘除标量/向量元素级
//! glam 语义、标量×向量经对称分派）已完成。**批次 O**（display flags
//! 落地：set_mode 位集/序列翻译 → starfish display::apply_window_flags
//! 运行时生效，OPENGL/创建期旗标垫底）与**批次 P**（Quaternion/Mat4
//! 算术：`*` 组合/标量缩放 + 取负，AsNumber 槽）已完成。**mouse/key
//! 完整度补全**（mouse.get_rel/set_visible/get_visible/get_focused/
//! set_pos 垫底 + key.get_mods/KMOD_*/name/get_focused/set_repeat·
//! get_repeat 垫底——事件层接线 Modifiers 与聚焦态）与 **cursors
//! 剪除定案**（光标图标桌面专属不跨平台）已记录。**批次 30**
//! （pygame.gfxdraw SDL_gfx 兼容签名面：目标会话在前/坐标分离/颜色
//! 在后——aa* 为兼容别名，实现与 f.circle 同一 batch 零额外成本；
//! rounded* 垫底直角）已完成。**批次 33**（display 后端声明位
//! VULKAN/DIRECTX/METAL/OPENGLES/OPENGL→GpuSettings.backends +
//! 不可用隐式退化回全后端；TRANSPARENT 创建期透明 + Screen 透明底
//! 清屏 + set_opacity 窗口透明度——桌宠形态 v1）已完成。**逐模块独立测试阶段启动**：共享测试宿主
//! `rpy::testhost::run(script, tag, transparent)`（装配/字体预载/事件泵/帧循环/收口
//! 固化）+ 每模块一个 `examples/<模块>_test` 案例（[模块-test] 行逐项
//! 断言 + 样张上屏）；首例 `font_test`（font 模块 15 项断言含旋转/缩放/
//! 半透明）已三端跑通。
//! 后续批次：mask/surfarray/PixelArray/BufferProxy（受像素回读桥约束）、
//! sprite（纯 Python）、
//! dialog 轮询式 job API（Android）、面 B（starfish 直通模块）、
//! .pyi 桩体系、行为回归集（旧仓库 M2 的 71 例迁移）、get_at/tostring
//! 像素回读（需 starfish 出 pub 包装）。

pub mod rpy;

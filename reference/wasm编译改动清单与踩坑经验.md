# wasm32-unknown-unknown 编译改动清单与踩坑经验（2026-09-08）

> 回答一个问题：**为了让 starfish 跑在浏览器里，代码被改了多少？**
> 以及本次 Web 迁移踩过的全部有效坑（附伪代码）与经验总结。
> 配套文档：`SDL退役与平台迁移设计笔记.md`（决策链）、`doc/log/starfish_changelog_2026-09-08.md`（三个实施批次）。

---

## 〇、结论速览

全库 **82 个 rust 文件 / 约 13,400 行**。为 Web 目标真正需要动的代码：

| 类别 | 文件数 | 说明 |
|---|---|---|
| **Web 专属适配（本批新增/修改）** | **6** | time 的时钟源分支、app.rs 的 Web 入口/节奏/canvas、render_entry 的错误处理器、settings.rs 的掩码、示例 11 的异步初始化、Cargo.toml target 段 |
| 属于 SDL3→winit/cpal 迁移（与 Web 无关，同期发生） | ~10 | 设备层胶水替换、事件枚举、示例迁移 |
| 完全未动（纯逻辑层） | **~66** | 渲染对象/mesh/管线/bind group/字体/几何/音频纯逻辑/解码器…… |

**核心结论：wasm 化的代码代价 ≈ 6 个文件的局部改动 + 0 行核心逻辑改动。**
这是两笔旧账的回报：①"cfg 只写在驱动边界、共用核心不假设平台"的架构纪律；
②DecoderPump（无线程解码泵）在 emscripten 时期就已就位。

同时是**净减法**：vendor/ 四个补丁目录、emsdk 工具链、emcc 链接配方、
egl_stub.js 全部删除——Web 工具链只剩 `wasm-bindgen-cli` 一项。

---

## 一、改动清单（按"暴露时机"分类）

> 分类标准很重要：**编译期就能抓到的坑是廉价坑，运行期才暴露的坑是昂贵坑**。
> 下表按此排列。

### 1.1 编译期暴露（改了编译不过/警告，成本最低）

| 文件 | 改动 | 触发方式 |
|---|---|---|
| `Cargo.toml` | wgpu 加 `webgl`；cpal 加 `wasm-bindgen`（target-gated）；新增 wasm-bindgen / wasm-bindgen-futures / web-sys（target-gated）；删 vendor 补丁与 sdl3 | 目标切换直接暴露 |
| `src/base/render/render_entry.rs` | **修正畸形 cfg**：`target_arch = "wasm32-unknown-unknown"`（永不成立的写法）→ `not(target_arch = "wasm32")` | check-cfg warning |
| `src/base/render/render_surface.rs` | 遮挡查询 cfg：`target_os = "emscripten"` → `target_arch = "wasm32"`（一行） | 目标切换 |
| `src/base/time/mod.rs` | `now()` 增加 Web 分支（std `Instant` 在该目标不可用）；`sleep_until` Web 分支 no-op | 预留位直接生效 |

### 1.2 运行期才暴露（编译全绿但行为不对，成本最高）

| 文件 | 改动 | 症状 |
|---|---|---|
| `src/base/render/settings.rs` | `to_wgpu` 做"许愿→掩码"：usage/present_mode 与 caps 取交集；**view_formats（Unorm↔Srgb 重解释）Web 上置空** | `Surface::configure` 校验错误 → panic |
| `src/base/app.rs` | Web 用 `ControlFlow::Wait`（Poll 的调度策略是"尽快跑"，非 vsync）；resumed 末尾补初始 `request_redraw`；`run()` Web 变体 `spawn_local` + 返回 `()`；`with_web_canvas_id` | 页面 CPU 全速空转卡死；画布全透明 |
| `src/base/render/render_surface.rs` | 新增 `size()` 读取器（供帧循环尺寸自愈对比） | 见坑 4b |
| `examples/11_web_triangles.rs` | **`Resized` → `surface.resize()`** + **帧循环尺寸自愈**（见坑 4b）；资源异步初始化共享槽位；`with_web_canvas_id` | 1×1 帧缓冲被 CSS 拉伸 = "全屏纯色"假象 |

### 1.3 诊断基建（不是修 bug，是让 bug 可见——第一天就该有）

| 位置 | 内容 |
|---|---|
| `render_entry.rs`（wasm 分支） | 设备级未捕获错误处理器：沿 `source` 链展开完整原因后抛出（默认处理器只打印顶层 "Validation Error"，细节全丢） |
| `examples/11` | `console_error_panic_hook`（panic 落控制台）；`console_log` extern（wasm 无 stdout，println 无处可去） |

### 1.4 完全未动（架构红利清单）

`render/`（mesh、pipeline、bind_group、shader_module、render_pass、command_encoder、features、settings 结构）· `gfx/` · `font/` · `audio/` 纯逻辑（ring、DecoderPump、Resampler、music、sfx、sound_data、symphonia 解码）· `pygame/` · `color` · `error` · `resources`。
**这些文件里的任何一行都不知道自己跑在浏览器里。**

---

## 二、踩坑记录（伪代码标注版）

> 每坑四段：现象 → 根因 → 修复（**伪代码**）→ 教训。

### 坑 1：畸形 cfg —— `target_arch` 不认完整 triple ⭐潜伏最深的坑

**现象**：check-cfg warning "unexpected cfg condition value"，易被当噪音忽略。
**根因**：`target_arch` 的合法值只有 `wasm32`/`x86_64` 这类架构名；完整 triple 不是合法 cfg 值，**比较结果恒为 false**——于是 `not(...)` 恒为 true，"排除 wasm"的分支在 wasm 上照样编译进去。

```rust
// 伪代码 ❌ 恒为 true（wasm32-unknown-unknown 也会编进去 → 运行期 panic）
#[cfg(not(target_arch = "wasm32-unknown-unknown"))]
fn blocking_init() { pollster::block_on(async_init()) }

// 伪代码 ✅ 架构名才是合法值
#[cfg(not(target_arch = "wasm32"))]
fn blocking_init() { pollster::block_on(async_init()) }
```

**教训**：cfg 拼错的代码不会编译失败，只会**静默地把错误的分支带上每个平台**。check-cfg warning 必须清零，不能容忍。

### 坑 2：Surface::configure 三连校验 ⭐错误信息最吝啬的坑

**现象**：`wgpu error: Validation Error` panic，无任何细节。
**根因**：三个字段在 WebGL2 上与桌面语义不同——① usage 硬请求（桌面全支持，WebGL 交换链只有 `RENDER_ATTACHMENT|TEXTURE_BINDING`）；② `view_formats`（Unorm↔Srgb 视图重解释）WebGL2 不支持；③ present_mode 非 Fifo 不支持。
**修复**：在 `to_wgpu` 里统一做"许愿→掩码"（与项目许愿系统同哲学）：

```rust
// 伪代码：表面配置一律与 caps 能力取交集，请求超出自动降级
let usage        = (wanted.usage & caps.usages).empty() ? RENDER_ATTACHMENT
                 : wanted.usage & caps.usages;
let present_mode = caps.present_modes.contains(&wanted.mode) ? wanted.mode : Fifo;
let view_formats = is_web ? [] : srgb_reinterpret(wanted.format);  // WebGL2 无重解释
```

**教训**：桌面能跑 ≠ 配置合法，只是后端容忍度高。**所有"请求类"配置都该有 caps 交集降级**，错误处理器负责兜底暴露。

### 坑 3：`ControlFlow::Poll` 在 Web = CPU 全速空转 ⭐卡顿元凶

**现象**：页面"非常卡顿"，渲染每秒数百次。
**根因**：winit web 的 Poll 调度策略是 Scheduler.yield/setTimeout（官方注释 "runs as fast as possible"，**非 vsync**）——事件循环 + 我们的每帧渲染以 CPU 极限空转。

```rust
// 伪代码：Web 的帧节奏必须由 rAF 驱动
match platform {
    Desktop => set_control_flow(Poll),      // 事件即处理 + 连续帧，无此问题
    Web     => set_control_flow(Wait),      // 空闲让路给浏览器
}
// Wait 下：frame() 末尾 request_redraw() → canvas rAF → 下一帧（每 vsync 一帧）
// 别忘了 resumed 末尾补一次 request_redraw() 启动帧链，否则一帧都不渲染
```

**教训**：桌面语义照搬到 Web 会以性能方式爆炸。**"谁来驱动帧"是平台差异**，必须进 cfg 边界（和音频泵一样）。

### 坑 4：接管 canvas 后 `inner_size` 初始 0×0 ⭐最隐蔽的坑

> 为什么初始是 0×0、"异步到达"具体指什么？本文是清单式记录；**事件循环/异步时序的
> 完整推导见 `wasm运行时生命周期与尺寸竞态问题详解.md`**（零 Web 经验友好版）。

**现象**：画布呈现"全屏均匀纯色"（粉 188,137,137），时而伴随尺寸抖动风暴。
**根因**：winit 接管 canvas 后 `inner_size` 初始为 0×0，真实尺寸经 **ResizeObserver 异步**到达；不处理 `Resized` 则表面永远停在初始尺寸（被 `max(1)` 变成 1×1），1×1 帧缓冲被 CSS 拉伸成 800×600。

```
伪代码（数学闭合过程，也是排查时用到的反推法）：
观察像素 (188,137,137)
→ sRGB 反解 = 线性 (0.5, 0.25, 0.25)
→ = 0.5×红 + 0.25×绿 + 0.25×蓝
→ = 三角形 NDC 中心 (0,0) 的重心插值权重 (0.5, 0.25, 0.25)
→ 结论：三角形渲染完全正确，只是画在 1×1 帧缓冲上被拉伸了！
```

```rust
// 伪代码 ✅ Resized 是"异步到达的真实尺寸"，必须同步给表面
fn event(&mut self, e: &WindowEvent, _ctx) {
    if let Resized { width, height } = e {
        surface.resize(width, height);   // 缺这行 = 表面永远停在初始 1×1
    }
}
```

**教训**：①Web 上"窗口尺寸"是**异步事实**，初始化时读到的值不可信；②"全屏纯色"不一定是渲染错误——先用像素值反推（一个 1×1 帧缓冲拉伸后，任意采样点颜色相同；真实渐变则各点不同）；③桌面示例能省略 Resized 处理，Web 不能。

### 坑 4b：Resized 与异步资源初始化的竞态 ⭐坑 4 的狡猾变体

**现象**：Resized 处理明明写了（坑 4 的修复），切换 WebGPU 后端后"深红色画布、无三角形"依旧——颜色从 (188,137,137) 变成 (128,64,64)。
**根因**：**事件与异步初始化的竞态**。`Resized` 事件在 `build_gpu` 的 `spawn_local` 完成之前到达——那一刻共享槽位是 `None`，resize 被跳过；此后再无 Resized 事件 → 表面永久卡在 1×1。颜色不同的原因：WebGPU 画布**没有 Srgb 格式**（规范事实），存的是线性原始字节 (0.5,0.25,0.25)→(128,64,64)，而非 GL 的 Srgb 编码 (188,137,137)。

```rust
// 伪代码 ❌ 只靠事件：事件与异步初始化的顺序不可控
fn event(e) { if let Resized(w, h) = e { surface.resize(w, h) } }  // 早于 gpu 就绪 → 丢失

// 伪代码 ✅ 帧循环尺寸自愈：不管顺序如何交错都能收敛
fn frame(&mut self, ctx) {
    let Some(gpu) = ... else { return };
    if gpu.surface.size() != ctx.size() {          // 每帧对比
        gpu.surface.resize(ctx.size());            // 不一致即对齐
    }
    /* 渲染 */
}
```

**教训**：①异步初始化 + 事件驱动共存时，**任何"事件到达即处理"的一次性逻辑都可能被竞态吞掉**——需要状态自愈兜底（每帧对比、不一致即修复）；②同一 bug 在不同后端上"颜色不同"——WebGL 的 Srgb 编码与 WebGPU 的线性原始字节，反推时先确认当前后端再对数学；③伴随坑：**WebGPU 画布没有 Srgb 格式**，同场景比 WebGL 路径"更暗更饱和"是格式事实而非 bug，跨后端观感统一需在上层做 gamma 补偿。

### 坑 10：Web 双后端自动降级（WebGPU 优先 / WebGL 兜底）——能力而非代码

wgpu 同时编译 `webgl` + `webgpu` 特性时，`Backends::all()` 按 `navigator.gpu` 可用性自动枚举：存在选 WebGPU，缺失自动只剩 GL 适配器（兜底）。**应用层零分叉，无需自己写 fallback**。验证矩阵（无头浏览器可全测）：

```
默认配置        → 后端=BrowserWebGpu（渲染正确，非 Srgb 编码）
强制 GL 配置    → 后端=Gl            （渲染正确，Srgb 编码）
```

注意：WebGPU 路径 requestAdapter 会报 `powerPreference ignored on Windows`（Chromium 已知限制，无害）；`adapter_info().backend` 打进控制台可让用户直接确认所选后端。

### 坑 5：winit Web 默认自建 canvas，且不入 DOM

**现象**：页面上只有自己的静态 `<canvas>`（黑的），渲染内容消失。
**根因**：winit web `create_window` 在未显式传 canvas 时**新建**一个元素，且 `append` 默认 false（连 DOM 都不进）——渲染发生在内存里的孤儿 canvas 上。

```rust
// 伪代码 ✅ 嵌入网页必须显式接管
let canvas = document.get_element_by_id(cfg.web_canvas_id)?;
attrs = attrs.with_canvas(Some(canvas.into_html_canvas_element()));
// 配套：WindowConfig::with_web_canvas_id("canvas") 暴露给用户，桌面忽略
```

**教训**：Web 后端的"窗口"默认不是页面上的任何元素。嵌入模型必须显式声明。

### 坑 6：无阻塞模型 + 'static —— 异步初始化的结构模式

**现象**：桌面同步初始化代码在 wasm 上无法照搬（不能 block_on）。
**根因**：wasm 主线程不能阻塞；`spawn_local` 要求 `'static`；异步回调无法直接借用 `&mut self`。

```rust
// 伪代码 ✅ 共享槽位模式：两平台一套结构
struct App { gpu: Rc<RefCell<Option<Gpu>>> }        // 槽位两平台一致

fn start(&mut self, ctx) {
    let slot = self.gpu.clone();
    let window = ctx.window().clone();              // Window: Clone，'static 化
    web: spawn_local(async move { *slot.borrow_mut() = Some(build(window).await) });
    desktop: *slot.borrow_mut() = Some(block_on(build(&window)));
}
fn frame(&mut self, _) {
    let Some(gpu) = self.gpu.borrow_mut().as_mut() else { return };  // 未就绪跳过
    /* 渲染代码两平台完全相同 */
}
```

**教训**：把"资源未就绪"设计成帧回调的正常状态，而不是启动障碍——Web 的初始化是**并发的**，桌面是**顺序的**，槽位模式让两者共享同一套渲染代码。

### 坑 7：wasm 无 stdout —— 诊断通道必须自建 ⭐决定排查效率的坑

**现象**：黑盒调试期 println 全部失踪，wgpu 校验错误只报顶层 "Validation Error"。
**根因**：wasm 无 stdout/stderr；wgpu 默认未捕获处理器只 Display 顶层错误，`caused by` 链被吞。

```rust
// 伪代码 ✅ 三件套（第一天就装，不是出错才装）
std::panic::set_hook(console_error_panic_hook::hook);        // ① panic → 控制台
device.on_uncaptured_error(|e| {                             // ② wgpu 错误 → 展开整条链
    let mut msg = format!("{e}");
    while let Some(s) = e.source() { msg += format!("\n caused by: {s}"); }
    panic!("{msg}");                                          //   借 panic hook 落地
});
#[wasm_bindgen] extern "C" { fn log(s: &str); }              // ③ console.log 替代 println
```

**教训**：本次排查的最大转折点就是 ② 装上后立刻拿到完整错误链。**Web 开发的第一等基础设施是错误可见性**，价值高于任何单个功能。

### 坑 8：wasm-bindgen-cli 版本必须与依赖树严格一致

**现象**：胶水生成后运行时符号对不上/直接不可用。
**根因**：wasm-bindgen 的 JS 胶水与 wasm 内运行时宏是版本配对的。

```bash
grep 'name = "wasm-bindgen"' Cargo.lock   # 查依赖树版本（如 0.2.126）
cargo install wasm-bindgen-cli --version 0.2.126   # CLI 必须同版本
# 升级 wasm-bindgen 依赖时，同步升级 CLI——写进 CI/文档
```

### 坑 9：线程分界是旧决策的红利兑现

Web 上没有 `std::thread`——但音频的 DecoderPump（解码泵）+ SPSC 环 + `pump_streams(budget)` 在 emscripten 时期就按"无线程可用"设计好了，本次**零改动**直接在浏览器可用。native 线程路径照常编译。
**教训**：平台差异只写在"谁来驱动"的边界——这条旧纪律在本次迁移中回收了最大的一笔红利。

---

## 三、经验总结（提炼为原则）

1. **架构纪律是迁移成本的唯一决定因素**。cfg 只在驱动边界 + 纯逻辑不假设平台 → 82 个文件只有 6 个需要 Web 适配。反过来说：任何"顺手在共用代码里加个平台判断"的妥协，都会在下次平台迁移时变成利息。
2. **坑分两等，等价不同**：编译期坑（依赖、cfg warning）廉价；运行期坑（configure 校验、调度策略、异步尺寸）昂贵且隐蔽。对运行期坑的唯一防御是**错误可见性基建**（坑 7 三件套）先行。
3. **渲染问题的隔离实验法**（本次闭环）：
   ① 纯色清屏（验管线）→ ② 去掉 draw（验 clear）→ ③ 硬编码位置/颜色（分离几何与数据链路）→ ④ 截图像素反推（数学闭合）。每轮只改一个变量；**像素值会说出真相**（粉色的 sRGB 反解直接暴露 1×1 拉伸）。
4. **无头浏览器 + PNG 裸解析 = 渲染问题的自动化闭环**：`msedge --headless=new --enable-logging=stderr --virtual-time-budget=6000 --screenshot` + Python zlib 解 PNG，不依赖人眼盯屏，可进 CI。
5. **生态事实查源码，别信记忆**：winit 的 Poll 调度策略、canvas 自建行为、pump_events 平台面——本次三个关键结论都来自读 registry 源码（十几分钟），事先的推测全部有偏差。
6. **"全屏纯色"是 Web 渲染的特征性症状**：优先怀疑帧缓冲尺寸 ≠ CSS 尺寸（拉伸假象），其次才是着色器/混合。用多采样点颜色相同/渐变即可区分。
7. **文档化的良性噪音**：winit web 启动时一次 "Using exceptions for control flow"（退出同步栈的机制）是预期行为——写进文档，避免每次都当 bug 查。

## 附：验证工具箱

```bash
# 构建 + 胶水 + 部署
cargo build --release --target wasm32-unknown-unknown --example 11_web_triangles
wasm-bindgen --out-dir web --target web \
  target/wasm32-unknown-unknown/release/examples/11_web_triangles.wasm
cd web && python -m http.server 8000        # http://localhost:8000/11.html

# 无头验证（控制台 + 截图）
msedge --headless=new --user-data-dir=<临时目录> --enable-logging=stderr --v=1 \
  --virtual-time-budget=6000 --screenshot=shot.png --window-size=900,700 \
  http://localhost:8000/11.html 2>&1 | grep CONSOLE
```

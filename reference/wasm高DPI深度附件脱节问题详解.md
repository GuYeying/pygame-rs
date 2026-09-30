# wasm 高 DPI 深度/颜色附件脱节问题详解

> 本文回答：**Web 端报 `BeginRenderPass` 校验失败、`depth ... does not match ... base plane` 的报错，根子在哪、为什么难查、怎么修对的。**
> 阅读前提：了解 pygame-rs 的 set_mode / Screen / swapchain 基本概念即可；不要求浏览器/前端知识。
> 姊妹篇：`wasm运行时生命周期与尺寸竞态问题详解.md`（wasm 执行周期）、`wasm编译改动清单与踩坑经验.md`（坑清单）。
> 对应 changelog：`starfish_changelog_2026-09-30.md` 批次三十七。

---

## 一、报错长什么样

Web 控制台（F12）出现：

```
The depth stencil attachment [TextureView of Texture "depth_texture"]
size (width: 1400, height: 1050) does not match the size of the other
attachments' base plane (width: 800, height: 600).
 - While validating depthStencilAttachment.
 - While encoding BeginRenderPass(["clear_screen"]).
 - While finishing [CommandEncoder].
[Invalid CommandBuffer] is invalid due to a previous error.
```

关键信息：**深度纹理 1400×1050，颜色附件 800×600**——一个 RenderPass 里
两个附件尺寸不一致，wgpu 校验层直接拒收。但诡异的是：**画面其实还在渲染**
（"确实正常运行了"），只是验证层持续刷错误。

---

## 二、三个尺寸数字的由来（先记住它们）

| 尺寸 | 谁产生的 | 语义 |
|---|---|---|
| `1260×945` | 建窗时 `sync_canvas_size` | canvas 初始 CSS 布局 × dpr（这台机器 dpr=1.75） |
| `800×600` | `set_size`（set_mode 参数） | 直接写进 canvas `width/height` 属性 |
| `1400×1050` | resize 监听的 `sync_canvas_size` | 800 CSS × 1.75 dpr 重算 |

一个数字对应一条**独立且互相冲突的 resize 路径**——这正是病根。

---

## 三、两条 resize 路径：对"窗口尺寸"的定义不一致

Web 上没有"窗口"实体，只有 `<canvas>` 元素。它的尺寸有两个正交概念：

- **CSS 布局尺寸**（`get_bounding_client_rect` / `client_width`）：页面
  排版里 canvas 占多大，单位是 CSS 像素。
- **物理分辨率**（canvas `width/height` 属性）：绘制缓冲有多大，单位是
  物理像素。

高 DPI 屏上，正确关系是 **物理 = CSS × devicePixelRatio(dpr)**。

starfish-window 的 web 后端有**两条路径**会改 canvas 尺寸，它们的哲学相反：

### 路径 A：`set_size(width, height)`（脚本 `set_mode` 触发）

旧实现把 `width/height` 当**物理像素**：

```rust
// 旧实现（有 bug）
style.set_property("width", format!("{}px", width / dpr)); // CSS = 物理/dpr
canvas.set_width(width);                                   // 物理 = width
```

即：`set_mode(800,600)` → canvas 物理 = 800，CSS = 800/1.75 ≈ 457px。

### 路径 B：`sync_canvas_size`（浏览器 resize 监听触发）

读**当前 CSS 布局 × dpr**，反推物理：

```rust
let css_w = canvas.get_bounding_client_rect().width(); // CSS 布局
let width = css_w * dpr;                               // 物理 = CSS × dpr
canvas.set_width(width);
```

即：如果 CSS 布局还是 800px，物理 = 800 × 1.75 = 1400。

### 冲突点

`set_size` 写 canvas 物理 = 800（路径 A 的结果），但 CSS 布局没立刻变成
457px（style 写入后浏览器还没回流，或 HTML 模板 canvas 无 CSS 尺寸导致
布局仍跟属性走）——于是 `sync_canvas_size` 读到的仍是 800 CSS，算出
1400 物理，把 canvas **覆盖回 1400**。

两条路径来回覆盖，最终 canvas 物理属性停在某个值，而引擎侧重建深度用的
又是另一个值。

---

## 四、致命组合：wgpu 的 swapchain 是"跟随 canvas 属性"的硬事实

wgpu Web 后端（WebGL2）的 swapchain 尺寸 **由 canvas `width/height`
属性决定**，`surface.configure(config)` 里的 `config.width/height` **被
忽略**（浏览器 canvas 绘制缓冲大小只能靠改属性，不能靠 configure 改）。

于是引擎 `Screen::resize(width, height)` 里两件事的尺寸来源**不同**：

```rust
// RenderSurface::resize(width, height)
self.config.width = width;
self.surface.configure(&device, &self.config);   // ← Web 上被忽略，swapchain 跟 canvas 属性
self.depth_texture = create_texture(width);      // ← 深度用 width（Resized 事件的尺寸）
```

- **swapchain** = canvas 属性（`sync_canvas_size` 最后写成的 800 或 1400）
- **深度纹理** = `Resized` 事件携带的尺寸

只要这两者不一致，`BeginRenderPass` 就报错。这正是"深度 1400 vs 颜色 800"
的完整链条。

---

## 五、为什么 dpr = 1 的机器一直没暴露

dpr = 1 时，路径 A 和路径 B 的结果**恰好相等**：

- `set_size(800,600)`：物理 = 800，CSS = 800/1 = 800
- `sync_canvas_size`：CSS 800 × dpr 1 = 800

两条路径都算出 800，canvas 属性和 Resized 尺寸永远一致，bug 静默潜伏。
dpr ≠ 1（Retina / 125% / 150% 缩放屏）才让两条路径分道扬镳。

这也解释了为什么这个 bug 在"flags 尺寸跟随（批次 32）"补齐后才出现——
在那之前 Web 上 `set_mode` 的尺寸根本不生效，canvas 永远停在建窗尺寸，
`set_size` 从未被触发，冲突无从谈起。

---

## 六、修复过程（两轮，第二轮才对）

### 第一轮：补发 Resized 事件（治标，仍脱节）

最初以为只是"Web 端 `set_size` 不发 Resized 事件，引擎不重建"。于是给
`set_size` 补发 `Event::Resized(参数尺寸)`。

结果：resize 链跑起来了，但深度变成 1400×1050、颜色仍是 800×600——
**还是脱节**，只是数字换了。因为补发的 Resized 用的是 `set_size` 参数
（800），而 swapchain 被 `sync_canvas_size` 覆盖成了 1400。

### 第二轮：统一尺寸语义（治本）

根子不是"少了哪个事件"，而是**两条路径对"窗口尺寸"的定义不一致**。
修复 = 让 `set_size` 与 `sync_canvas_size` **同源**：

```rust
// 新实现：width/height = 逻辑尺寸（CSS px），物理 = 逻辑 × dpr
let dpr = self.dpi_scale() as f64;
style.set_property("width", format!("{}px", width));   // CSS = 逻辑
let pw = (width as f64 * dpr).round() as u32;
canvas.set_width(pw);                                   // 物理 = 逻辑 × dpr
// Resized 用 canvas 实际物理属性（= swapchain 实际尺寸），严格一致
```

此后：`set_mode(800,600)` → canvas 物理 = 1400×1050，`sync_canvas_size`
算出的也是 1400×1050（一致，不再互相覆盖），swapchain 与深度都 = 1400。

**附带收益**：高 DPI 屏上 canvas 渲染分辨率从 800 提升到 1400×1050，
画面更清晰（旧实现把 canvas 降级到了低分辨率）。

---

## 七、结论（一句话 + 检查清单）

**一句话**：Web 上 swapchain 尺寸 = canvas 物理属性（硬事实），深度纹理
必须与它严格同源；任何"物理语义 vs CSS×dpr 语义"的混用，都会在 dpr≠1
的屏上引爆深度/颜色脱节。

**同类问题自查**：
1. 改 canvas 尺寸的代码有几处？它们的"物理 vs 逻辑"语义是否一致？
2. Resized 事件携带的尺寸，是否 = canvas `width/height` 属性（而非重算）？
3. 在 dpr ≠ 1 的屏（浏览器缩放 125%/150%）上验证过一次吗？

> 本例的教训是通用的：**跨 DPI 尺寸，永远从"唯一权威源"（这里 = canvas
> 物理属性）派生，而不是在多处各自换算。**

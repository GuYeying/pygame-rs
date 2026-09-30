# wasm 运行时生命周期与"尺寸竞态"问题详解（零 Web 经验友好版）

> 本文回答：**这次"粉色/深红画布"的运行时问题，在 wasm 的执行周期里到底是怎么发生的？**
> 阅读前提：你会 Rust、了解 starfish 的桌面示例。不要求任何浏览器/前端知识。
> 姊妹篇：`wasm编译改动清单与踩坑经验.md`（坑的清单与清单化结论，本文负责"为什么"）。

---

## 一、先立框架：桌面世界 vs 浏览器世界的三条根本差异

你熟悉的桌面程序和浏览器里的 wasm，是两种完全不同的"生存方式"：

| | 桌面（exe） | 浏览器（wasm） |
|---|---|---|
| **谁拥有主线程** | 你的 main()，从头跑到尾 | 浏览器。你的 wasm 是"房客"，只在浏览器叫你时才执行 |
| **能阻塞吗** | 能。`block_on`/`sleep` 随便用 | **永远不能**。主线程一睡，整个页面（含渲染、输入）全部假死 |
| **"窗口多大"何时知道** | 创建窗口时 OS **同步**告诉你 | 创建后是 0×0，真实尺寸要等浏览器**量完布局后异步通知**（ResizeObserver） |
| **事件从哪来** | OS 事件队列，`poll` 就有 | 浏览器事件循环，注册监听器后**它主动喂你** |
| **println 去哪了** | 终端 | **无处可去**（wasm 没有 stdout）——诊断必须走浏览器控制台 |

一句话：**桌面世界是"我主导、同步、可等待"；浏览器世界是"浏览器主导、异步、只靠回调"。**
本次所有坑都是这三条差异的直接后果。

---

## 二、wasm 应用的完整运行周期（我们的 11 示例逐步走一遍）

### 2.1 浏览器侧：加载与实例化（Rust 代码执行之前）

```text
① 浏览器打开 11.html
② <script type="module"> 执行 import init from './11_web_triangles.js'
③ init() fetch 同目录的 11_web_triangles_bg.wasm（编译产物）
④ 浏览器把 wasm 编译成机器码并实例化（此刻 Rust 的任何代码都还没跑）
⑤ 实例化完成 → 浏览器自动调用导出函数 _start
        ↑ 这就是示例里 #[wasm_bindgen(start)] 标注的 web_main()
```

**关键认知 1**：`fn main()` 在 Web 上不是程序起点，而是"_start 回调里的一行"。

### 2.2 Rust 侧：从 main 到"把控制权还给浏览器"

```text
⑥ web_main() → main() → run(app, config)
⑦ run() 的 Web 变体：spawn_local(异步任务)   ← 只是【排队】，一行都没执行！
⑧ main() 返回 → run() 返回 → web_main() 返回
⑨ 控制权回到浏览器 ——【此刻：窗口不存在、GPU 未初始化、一帧都没渲染】
⑩ 浏览器处理完手头事务后，轮到微任务队列里我们排的那个异步任务：
   → EventLoop::new()          （winit 向 canvas 注册键盘/鼠标/尺寸监听）
   → run_app()：
      → resumed 回调 → create_window（接管 <canvas id="canvas">）
                     → app.start() → 又 spawn_local(build_gpu) ← 再次排队！
                     → request_redraw()（踢第一脚帧链）
      → winit 把控制流异常抛出 → Rust 调用栈退回浏览器（这是它的既定机制）
⑪ 浏览器恢复控制权
⑫ 排队的 build_gpu 开始执行：
   → await 请求适配器   → 【让出】浏览器做别的事 → GPU 回答 → 恢复
   → await 请求设备     → 【让出】→ 恢复
   → 建管线/网格 → 填入共享槽位 ——【GPU 资源就绪】
⑬ requestAnimationFrame（rAF，浏览器每 vsync 一次的回调）→ winit 派发事件
   → about_to_wait → 我们的 frame() 渲染 → request_redraw → 下一个 rAF
   → 无限循环（这就是 Web 的"主循环"）
```

**关键认知 2**：`spawn_local` ≠ 执行，是"往浏览器的任务队列排队"。队列前面的
还有浏览器自己的布局、样式计算等事务——**你的代码什么时候跑、跑一半在哪让出，
全由浏览器调度**。

**关键认知 3（本次问题的舞台）**：从 ⑨ 到 ⑬，至少有**三条独立时间线在赛跑**：

```text
T1（GPU 线）  ：排队 → 询问 GPU（多次让出等待）→ 资源就绪，填槽位
T2（尺寸线）  ：浏览器布局完成 → ResizeObserver 测量 canvas → 派发 Resized 事件
T3（帧线）    ：rAF 每 vsync 一次 → 派发事件 → 调 frame()
```

**桌面世界这三件事是顺序确定的**（先有窗口尺寸 → 再初始化 GPU → 再渲染）；
**Web 世界它们并发赛跑，先后顺序由浏览器决定，没有任何保证**。

---

## 三、本次 bug 的逐帧时间线（对照看完就懂了）

### 3.1 桌面时间线（为什么桌面从来没问题）

```text
t0  Window::new(...)        → OS 同步返回，尺寸立即 = 800×600
t1  RenderEntry::new(...)   → block_on 阻塞初始化 GPU（阻塞也没关系）
t2  第一帧                  → 表面 800×600，渲染正确
```

### 3.2 Web 时间线（出问题的顺序）

```text
t0  create_window（接管 canvas）
     winit 内部记录的尺寸 = 0×0（真实尺寸要等 ResizeObserver，见 2.1⑩）
t1  start() → spawn_local(build_gpu) 排队          【T1 启动，但没执行】
t2  ◆ T2 赢得了赛跑：ResizeObserver 触发！
     → Resized(800, 600) 派发到我们的 event()
     → event() 里：gpu 槽位还是 None（build_gpu 没跑完！）
     → resize 被跳过 ✗✗✗                            【唯一的修复机会溜走了】
t3  ◆ T1 完成：build_gpu 填入槽位
     → 表面按 start 时拿到的尺寸 (0,0)→max(1) 配置成 1×1
     → Resized 不会再来了（尺寸没再变过）
t4  ◆ T3 启动：frame()
     → 清屏 + 三角形画进 1×1 的帧缓冲
     → NDC (0,0) 处的插值色 = 0.5×红+0.25×绿+0.25×蓝 = (0.5,0.25,0.25)
t5  浏览器合成页面：1×1 的画布被 CSS 拉伸到 800px 宽
     → 整个画布 = 那一个像素 = 全屏纯色
       · GL 路径（Srgb 格式，自动编码）：(188,137,137) 粉
       · WebGPU 路径（无 Srgb 格式，原始字节）：(128,64,64) 深红
```

### 3.3 修复后的 Web 时间线（尺寸自愈）

```text
t4  frame():
     对比 ctx.size()=(800,600) 与 surface.size()=(1,1)
     → 不一致 → surface.resize(800,600) 自愈
t5  之后的帧全部按 800×600 正确渲染（三角形 + navy 底）
```

自愈能收敛的原因：**winit 知道真实尺寸**（ResizeObserver 已经告诉过它，
Resized 事件就是它发的），所以 `ctx.size()` 迟早变成 (800,600)——
"没赶上就下一帧补"在 Web 是完全合法的修复模式。

---

## 四、为什么颜色不一样：Srgb 编码的两种命运

同一个 (0.5, 0.25, 0.25)（三角形中心的线性插值色），存进两种格式：

| 后端 | 表面格式 | 写入行为 | 画布字节 | 视觉 |
|---|---|---|---|---|
| WebGL2（Gl） | Rgba8Unorm**Srgb** | 自动做线性→sRGB 编码 | (188,137,137) | 粉（亮） |
| WebGPU | Rgba8/Bgra8Unorm（**WebGPU 画布没有 Srgb 格式**，规范如此） | 原始字节直接存 | (128,64,64) | 深红（暗） |

**两个都是"正确渲染"**——只是帧缓冲格式不同导致亮度不同。
跨后端观感统一需要在上层（pygame 层）做 gamma 补偿，属于后续可选项。

---

## 五、浏览器名词速查（本文用到为止）

| 名词 | 是什么 | 在 starfish 里的对应物 |
|---|---|---|
| **事件循环** | 浏览器的主循环：执行任务→渲染→等事件→循环。主线程只有这一个 | 桌面的"main 函数 + 消息循环"合体，但归浏览器管 |
| **微任务/任务队列** | `spawn_local` 排队的地方；浏览器处理完当前事务后依次执行 | 我们的 `run()`、`build_gpu` 都从这开始 |
| **rAF**（requestAnimationFrame） | 浏览器每 vsync（约 16.7ms）调一次的回调 | Web 版的"帧心跳"，`request_redraw` 就是排一个 rAF |
| **ResizeObserver** | DOM 元素尺寸变化的监听器（异步触发） | winit 的 `Resized` 事件的真实来源 |
| **navigator.gpu** | WebGPU API 的入口对象 | 不存在 = 浏览器不支持 WebGPU → wgpu 自动落 WebGL2 |
| **canvas 双尺寸** | CSS 显示尺寸 vs `width/height` 属性（绘制缓冲尺寸）。两者可被独立拉伸 | "全屏纯色"假象的直接来源：缓冲 1×1，显示 800px |
| **devicePixelRatio (dpr)** | 物理像素 / CSS 像素比（Windows 常为 1.25/1.5） | winit 用它把逻辑尺寸换算成缓冲尺寸 |
| **控制流异常** | winit web 用抛 JS 异常的方式从同步 Rust 栈退回浏览器 | 启动时控制台那条 "don't mind me" 错误，**预期行为** |

---

## 六、wasm 运行期的四条"物理定律"（写 Web 代码前默念）

1. **永不阻塞主线程**。`block_on`、长 `sleep`、死循环 = 页面假死。
   → 所以 `run()` Web 变体用 spawn_local；`sleep_until` 在 Web 是 no-op。
2. **一切信息异步到达**：窗口尺寸、GPU 资源、输入事件——"到不了就下一帧再试"。
   → 所以帧循环里有尺寸自愈；资源未就绪时 frame 静默跳过是**正常状态**而非错误。
3. **事件与异步初始化会赛跑**：任何"事件到达即一次性处理"的逻辑都可能被竞态吞掉。
   → 所以需要状态自愈兜底（每帧对比、不一致即修复）。
4. **帧节奏归浏览器**（rAF/vsync），不归 CPU 速度。
   → 所以 Web 用 `ControlFlow::Wait`（Poll 的调度策略是 CPU 全速，实测卡死级卡顿）。

---

## 七、验证这个理解的最小实验（可自己动手）

1. 打开 11.html，F12 控制台看 `build_gpu: 后端=...`（确认后端）。
2. 在控制台输入 `document.getElementById('canvas').width`——这就是绘制缓冲尺寸；
   与窗口尺寸对比，能直观看到"双尺寸"的存在。
3. 把 11.html 里 `<canvas>` 的 CSS 加一行 `style="border: 4px solid red"` 刷新——
   观察缓冲尺寸与 CSS 盒子的独立性。
4. （进阶）在 DevTools 的 Performance 面板录制 3 秒，能看到 rAF 回调的节奏
   ——这就是"Web 主循环"的实物。

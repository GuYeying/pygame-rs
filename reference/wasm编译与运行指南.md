# starfish Web（wasm32-unknown-unknown）编译与运行指南

> 面向零 wasm 经验的开发者。照抄第二节的命令即可跑通示例 11；
> 第三节讲怎么把自己的项目搬上 Web；第四节是排错 FAQ。
> 背景原理（为什么）见 `wasm运行时生命周期与尺寸竞态问题详解.md`；
> 坑的清单见 `wasm编译改动清单与踩坑经验.md`。

---

## 一、前置准备（每台机器一次性）

### 1. 安装 wasm 编译目标

```bash
rustup target add wasm32-unknown-unknown
```

### 2. 安装 wasm-bindgen-cli（生成浏览器加载用的 JS 胶水）

**版本必须与项目依赖树里的 wasm-bindgen 完全一致**，否则运行时符号对不上。

```bash
# 第一步：查当前依赖树的版本（在 starfish 项目根目录）
grep 'name = "wasm-bindgen"' -A 1 Cargo.lock | head -2
#   输出形如：name = "wasm-bindgen" / version = "0.2.126"

# 第二步：安装同版本 CLI（编译安装，约几分钟）
cargo install wasm-bindgen-cli --version 0.2.128
```

> 升级项目的 wasm-bindgen 依赖后（Cargo.toml / cargo update），**必须重装匹配版本的 CLI**。
> 验证：`wasm-bindgen --version`。

### 3. 本地静态服务器

任选其一：Python（`python -m http.server`）、VSCode Live Server 等。
**不能用 `file://` 直接双击 html**——浏览器禁止 file 协议 fetch wasm。

### 4. 浏览器

- WebGL2：所有现代浏览器 ✓（渲染兜底路径）
- WebGPU（可选，自动优先）：Edge/Chrome 113+；不支持的浏览器自动落到 WebGL2，无需配置

---

## 二、标准流程：编译并运行示例 11（可直接照抄）

```bash
# ① 编译（release：debug 的 wasm 体积大且慢，不推荐）
cargo build --release --target wasm32-unknown-unknown --example 11_web_triangles

# ② 生成 JS 胶水（输出到 web/ 目录，--target web = 供 ES Module 导入）
wasm-bindgen --out-dir web --target web \
  target/wasm32-unknown-unknown/release/examples/11_web_triangles.wasm

# ③ 启动本地服务器
cd web && python -m http.server 8000

# ④ 浏览器打开 http://localhost:8000/11.html
```

产物说明（web/ 目录）：

| 文件 | 作用 |
|---|---|
| `11_web_triangles.js` | JS 胶水：负责加载 wasm、内存管理、JS/WASM 边界转换 |
| `11_web_triangles_bg.wasm` | 你的 Rust 代码本体 |
| `11_web_triangles.d.ts` 等 | TypeScript 类型提示（可忽略） |

打开页面后，F12 控制台应看到：

```text
build_gpu: 后端=BrowserWebGpu   ← WebGPU 可用（否则自动显示 Gl，均为正常）
Uncaught Error: Using exceptions for control flow...  ← winit 既定机制，预期噪音，非错误
```

---

## 三、把自己的项目搬上 Web

### 3.1 平台差异已封装——应用代码零 cfg

三个封装件吸收了全部平台差异（示例 11 即零 cfg 范本）：

| 封装件 | 吸收的差异 | 用法 |
|---|---|---|
| `starfish::base::app::InitSlot<T>` | Web 异步初始化 / 桌面同步阻塞 | `slot.init(async { ... })`；`frame` 里 `get_mut()` 未就绪跳过 |
| `starfish::web_entry!()` 宏 | `#[wasm_bindgen(start)]` 入口 + panic→控制台 | `main` 后一行 `starfish::web_entry!();`（桌面展开为空） |
| `starfish::base::web::console_log` | wasm→浏览器控制台 / 桌面→stdout | `console_log("...")`（别用 println，Web 上无处可去） |

零 cfg 的应用骨架（完整版见 `examples/11_web_triangles.rs`）：

```rust
use starfish::base::app::{run, Application, Ctx, InitSlot, WindowConfig};
use starfish::base::window::{Window, WindowEvent};

struct App { gpu: InitSlot<Gpu> }

fn start(&mut self, ctx: &mut Ctx) {
    let slot = self.gpu.clone();
    let window = ctx.window().clone();
    slot.init(async move { build_gpu(&window).await }); // 桌面阻塞跑完；Web 排队
}

fn event(&mut self, e: &WindowEvent, _ctx: &mut Ctx) {
    // Web 关键：真实尺寸经 ResizeObserver 异步到达，必须同步给表面
    if let WindowEvent::Resized { width, height } = e {
        if let Some(mut gpu) = self.gpu.get_mut() { gpu.surface.resize(*width, *height); }
    }
}

fn frame(&mut self, ctx: &mut Ctx) {
    let Some(mut gpu) = self.gpu.get_mut() else { return }; // 未就绪：跳过（正常状态）
    // 尺寸自愈：覆盖 Resized 与异步初始化竞态
    let size = ctx.size();
    if gpu.surface.size() != size { gpu.surface.resize(size.0, size.1); }
    /* 以下渲染代码与桌面完全相同 */
}

fn main() { run(App::default(), WindowConfig::new("demo", 800, 600)); }
starfish::web_entry!();
```

Web 构建时用户 crate 需在 dev-dependencies 加 `wasm-bindgen = "0.2"`（宏的属性路径）。

### 3.2 资源文件（重要差异）

Web 上**没有文件系统**：`fs::read_to_string`、`SymphoniaDecoder::from_file` 这类路径读取在浏览器不可用。两种方案：

- **编译期内嵌**（推荐，示例 11 的做法）：`include_str!("../resources/shaders/triangle.wgsl")`、`include_bytes!`——小资源直接打进 wasm。
- **运行时 fetch**：大资源（纹理/音频）用 `web_sys` 的 fetch + `Response::array_buffer` 异步加载后走 `from_interleaved_f32` / 内存接口。后续版本可能提供 `AssetManager` 统一封装（见 reference 既有笔记）。

### 3.3 建 crate 项目（而非 example）时的额外一步

```toml
# 你的 Cargo.toml 需要
[lib]
crate-type = ["cdylib", "rlib"]
```

入口同样用 `#[wasm_bindgen(start)]` 标注（或导出显式 init 函数）。

---

## 四、FAQ / 排错速查

| 症状 | 原因 | 处置 |
|---|---|---|
| 白屏/透明画布 | F12 看 panic；多数是**没处理 Resized**（表面停在 1×1 被拉伸） | 照 3.1 ③④ 补齐；帧循环加尺寸自愈（示例 11 已示范） |
| 页面卡死级卡顿 | Web 上用了 `ControlFlow::Poll`（调度策略 = CPU 全速空转） | base 已内置 Wait（app.rs cfg），**不要改回 Poll** |
| 控制台 "Using exceptions for control flow" | winit 退出同步栈的既定机制 | 预期噪音，忽略 |
| `powerPreference ignored on Windows` | Chromium 已知限制 | 忽略 |
| `Requested format X is not in list of supported formats` | 请求了 WebGL 不支持的表面格式（如 Bgra8；WebGL2 仅支持 Rgba8 系） | 用默认 SurfaceSettings（已内置 caps 掩码），不要手写格式 |
| `bindgen format ... must exactly match`（CLI 0.2.126 vs schema 0.2.128） | 加新特性/升级依赖使 lock 浮动，CLI 没跟上 | 实测踩过：`cargo install -f wasm-bindgen-cli --version <lock 版本>` 后重新生成胶水 |
| 画面全屏纯色（粉/深红） | 1×1 帧缓冲被拉伸（尺寸竞态） | 同白屏条目；数学详解见生命周期文档第四章 |
| 音频无声 | 浏览器 autoplay 政策：AudioContext 需要用户手势后 resume | 已知事项：首次点击/按键时调用恢复（后续版本计划内建） |
| 渲染偏暗（WebGPU vs WebGL 对比） | WebGPU 画布无 Srgb 格式（规范），线性字节直接显示 | 格式事实非 bug；跨后端观感统一留待上层 gamma 补偿 |
| 想强制用某个后端 | — | `GpuSettings::default().with_backends(wgpu::Backends::GL)`（或 `BACKENDS_GPU`） |

---

## 五、命令速查卡

```bash
# ── 一次性 ────────────────────────────────────────────
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version <Cargo.lock 里的 wasm-bindgen 版本>

# ── 每次构建 ──────────────────────────────────────────
cargo build --release --target wasm32-unknown-unknown --example 11_web_triangles
wasm-bindgen --out-dir web --target web \
  target/wasm32-unknown-unknown/release/examples/11_web_triangles.wasm
cd web && python -m http.server 8000        # → http://localhost:8000/11.html

# ── 查 CLI 应配版本 ───────────────────────────────────
grep 'name = "wasm-bindgen"' -A 1 Cargo.lock | head -2

# ── 无头验证（不打开浏览器）──────────────────────────
msedge --headless=new --user-data-dir=<任意空目录> \
  --enable-logging=stderr --v=1 --virtual-time-budget=6000 \
  --screenshot=shot.png --window-size=900,700 \
  http://localhost:8000/11.html 2>&1 | grep CONSOLE
```

> 不需要任何特殊 HTTP 响应头（无线程/SharedArrayBuffer，COOP/COEP 均不需要）——
> 这是相对旧 emscripten pthread 方案的又一个简化。

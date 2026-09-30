# pygame 兼容层模块架构（基于用户分析优化，最终设计）

> 状态：**设计定稿 · 未实现**——本文是 pygame 兼容层的模块映射与 Surface
> 统一架构契约，实现排期按 Tier 0→3 依赖序推进。
> 上游：`pygame绑定层API设计稿.md`（绑定架构 / 生成器门 / 性能分层原则）。
> 来源：用户对 pygame 官方模块族的逐模块分析（2026-09-20），经三轮优化定案。

---

## 一、四条设计原则

1. **1:1 接口 + 兼容垫底**：pygame 公开 API 逐一存在；确实不适用的
   （GL 属性等旧约定）→ 保留名字、返回安全默认值（不报错），同时提供
   starfish 等价接口（如 `display.wgpu_info()`）并文档标注差异。
2. **热路径下沉 Rust**：每帧/每像素/每资源调用的实现住 Rust 原生面；
   Python 只留装配胶水（设计稿 §六 性能分层原则）。
3. **一个实现，多个兼容名**：合并的是**实现**，保留的是**名字**——
   `pygame.mixer` / `pygame.mixer.music` / `pygame.event` 等兼容名全部
   存在，实现统一指向唯一宿主；删除的是重复实现（fastevent 第二套事件
   循环、gfxdraw 独立光栅）与不支持的模块。
4. **entry 锁定 + 类型互通**：`pygame.run_script`（pygame 形态）与
   `base::app::run`（纯 Rust 形态）各自固定运行形态；类型经 pygame 层的
   `from_base()/to_base()` 双向互通（设计稿 §一 类型互通单向化）。

---

## 二、模块映射总表（pygame 官方模块族 → starfish 宿主）

### Tier 0 · 类型与常量（零依赖，最先做）

| 模块 | 宿主 | 状态/说明 |
|---|---|---|
| `Color` | Rust（`src/pygame/color.rs`） | ✅ 已实现；含 base HDR 色双向互通 |
| `Rect` | Rust（`src/pygame/rect.rs`） | ✅ 已实现 |
| `locals` | Rust 常量 + Python 组织 | **定案**：包含**全部常量**，按源模块分组（keys/events/display/gfx/…）——调试定位与“重导出到对应模块常量”两便；顶层保留 flat `import *` 兼容；位值自定，只承诺名字与语义 |
| `math` | Rust（glam 直绑） | Vector2/Vector3 + **Quaternion/Mat4（3D 优先，见 §四）** |
| `version` | Python 常量 | trivial |

### Tier 1 · base 直绑（本体已有，只做映射）

| 模块 | 宿主 | 状态/说明 |
|---|---|---|
| `display` | base::app + window | set_mode 记账/flip/窗口 flags（已落地）；GL 属性类接口 → 兼容垫底 + `wgpu_info()` 替代接口 |
| `key` / `mouse` | base::window 状态表 | 轮询+事件双轨已有；**无新增维护对象**（状态表 base 已有，纯映射） |
| `event` | base 事件缓冲 | 批灌已定（§shell_rpy）；**吸收 fastevent**（其本质=快轮询，我们的 get 本就是直接排空） |
| `time` | base::time | Clock/FixedTimestep 映射 |
| `image` | base/resources（image crate） | load/save/save_extended 映射 |
| `font` | base/font | 图集+光栅化已有；**SysFont 不再支持（定案）**——系统字体跨平台支持度差；`SysFont(...)` 保留为兼容 shim（路由到标准内嵌字体，文档明确标注）；质量目标 = 内嵌字体**多国语言覆盖** |
| `mixer` / `mixer.music` | base/audio | **实现统一为 base/audio**（SFX 通道池=Sound；music 流=MusicStream）——名字保留、实现唯一（§一 原则 3） |

### Tier 2 · 渲染核心（Surface 统一架构，见 §三）

| 模块 | 宿主 | 状态/说明 |
|---|---|---|
| `Surface` | Rust 原生面（**三态后端**，§三） | **本兼容层的核心交付** |
| `draw` | base/gfx + Surface pass | 16 形状光栅化已有；**吸收 gfxdraw**（其独立光栅与 AA 语义已归 wgpu 采样） |
| `cursors` | winit 光标图标接口 | **可行**（winit 0.30 `Icon::from_rgba` 实测存在）——系统光标映射 + 自定义 RGBA 光标 |

### Tier 3 · 扩展与数组视图

| 模块 | 宿主 | 状态/说明 |
|---|---|---|
| `mask` | Surface 渲染管线 | 遮罩=管线化纹理处理（用户分析定案）：阈值/颜色键 → mask 纹理输出 |
| `PixelArray` | Surface 像素桥 | CPU 视图句柄（§三 像素回读桥） |
| `sndarray` | base/audio SoundData | 纯 Rust 数据，numpy-free 纯 Python 数组协议 |
| `surfarray` | Surface 像素桥 | 同 PixelArray（GPU 后端受回读桥约束，§三.5） |
| `BufferProxy` | Rust Buffer 语义 | 对齐 base buffer 体系 |
| `sprite` | 纯 Python（**保留 pygame 架构**） | **不做 ECS（定案）**：性能无增益 + 开发难度↑；现代化 = 保留 Group/Sprite 架构，叠加开发者便利接口 |
| `touch` | base 触摸事件 | winit 支持（含 Android） |
| `gamepad`（新名） | base::gamepad（gilrs） | **替代 joystick+controller 两个名字**（gilrs 语义天然对齐 controller；文档标注对齐关系）——自研设计、API 风格对齐 |
| `dialog` / `io` / `net` | base 已有 | 直接映射 |

### 剪除（不做，文档注明原因）

| 模块 | 原因 |
|---|---|
| `freetype` | base/font 自研光栅化已覆盖，无 freetype 依赖 |
| `midi` | 游戏库边界外 |
| `camera` | 用户定案不支持（与设备接口第二批决策一致） |
| `fastevent` | 实现并入 event（名字可留兼容别名） |
| `joystick` / `controller`（名） | 统一为 `gamepad`，文档标注对齐 controller 语义 |
| `scrap` | **不实现（2026-09-20 定案）**：移动端剪贴板行为无法收拢（Android=ClipboardManager JNI+IME 交互）；剪贴板需求由未来 egui UI 层零成本继承（egui-winit 内置 arboard） |
| `cursors` | **不实现（2026-09-30 定案）**：光标图标为桌面专属概念，不跨平台（移动端无光标、Web CSS 光标价值/成本比低）；窗口级 `set_cursor_visible` 仍保留于 base/starfish-window |
| GL 属性类接口 | wgpu 语义替代，垫底原则处理 |

---

## 三、Surface/Screen 统一架构（核心交付）

### 3.0 类型分立：Screen 与 Surface（2026-09-20 定案）

**两个类型，一套附件协议**：

| | `Screen`（display.set_mode 返回） | `Surface`（离屏面） |
|---|---|---|
| 本体 | starfish 默认 RenderTarget(Swapchain) 的**间接包装** | 自有 wgpu Texture |
| color attachment | **每帧从交换链 acquire**(常变) | 恒定(自有纹理) |
| depth attachment | 声明即有(base RenderSurface 默认带) | 构造时声明，尺寸跟随 |
| 可采样(被画到别处) | **不可**（swapchain 纹理无 TEXTURE_BINDING，物理事实） | **可**（TEXTURE_BINDING + RENDER_ATTACHMENT） |
| present 语义 | 有（flip） | 无（blit 桥到目标面） |

- **不合并类型**：可采样性与附件生命周期根本不同——合并会迫使 swapchain
  面伪装成普通面，语义混乱；保留 Swapchain 思路，但把"每帧获取附件"抽象
  成二者的**共同协议**。
- **共同底层**：二者都实现 RenderTarget 语义（产出本帧附件组：colors +
  可选 depth）；差异只在附件**获取方式**（screen 每帧 acquire / surface
  恒定自有）。

### 3.1 with 语法：RenderTarget 协议 + 深度声明

```python
with screen:                     # 主面：swapchain 附件(每帧 acquire) + present 语义
    pygame.draw.line(...)

with surface:                    # 离屏面：自有纹理附件
    pygame.draw.circle(...)

with surface(depth=True):        # 深度缓冲：按声明带出(离屏面构造时已声明)
    pygame.draw.cube(...)
```

- **深度缓冲三问的解答**：① 主面——base RenderSurface **默认带深度**
  （get_current_depth_attachment 已有），with 自动带出；② 离屏面——
  `Surface(size, depth=True)` 构造时创建 depth texture（尺寸随面，resize
  跟随）；③ with 块**继承声明**，逐块临时开关 = v2 显式参数。
- **多颜色目标（MRT）**：Rust pass 层原生支持附件数组（≥2 色 + 深度）；
  Python with 语法形状 = surface-group（`with pygame.targets([a, b])`），
  **v2 形状待定**——架构不设障碍。

### 3.2 Rust 分层：RenderTarget 协议（pygame 层持有，rpy 层消费）

```rust
// src/pygame(语义层,RP-free;wgpu 类型经 base::render)
pub trait RenderTarget {
    /// with 块开启时产出本帧附件组
    fn attachments(&mut self) -> PassAttachments;   // colors + 可选 depth
}
// Screen：每帧从 swapchain acquire；
// Surface：恒定纹理——两实现，一套 with 协议。
```

- 原生面注册（rpy 层）只消费 `PassAttachments`——Screen 与 Surface 在 VM
  注册面上是**同一类对象**（都产附件组），脚本无感。

### 3.3 像素数据驻留策略（surfarray 方向修订，用户定案）

**像素数据默认 GPU 驻留**；像素操作 = **管线族**（一份像素处理着色器 +
uniform 条件：亮度/色键/翻转/缩放/卷积…），输入/输出纹理由管线填入：

- 接口收敛：`pygame.transform.apply(surface, effect, params)` 形状，
  而非逐操作 CPU 代码——着色器多设几个 uniform 即是条件分支；
- 收益：数据**不上传下载**，管线复用；与 base 渲染同一 encoder 提交；
- CPU 读取仅经回读桥（延迟 1 帧）；CPUBuffer 后端 = 兼容垫底（v2，
  基于 image 基层），供 legacy 逐像素脚本。

（修订说明：初稿曾建议 surfarray 走 CPU 全速路径——用户定案 GPU 驻留
更优：管线 + uniform 的接口面恒定，数据不落盘。）

### 3.4 像素回读桥（仅 CPU 读取场景）

- GPU 后端的 `get_at` / `get_view` 读取 = copy texture→buffer→map，
  **延迟 1 帧语义**（提交回读，下帧可取；意图记忆模式复用）；
- `set_at` / 少量像素写 = write_texture 局部更新（即时，小开销）；
- 性能特征**不承诺与 CPU 画布一致**，文档标注（pygame 老脚本重度
  逐像素操作在 GPU 后端会慢——这是 GPU 化的固有代价，垫底不报错）。

---

## 四、剪贴板策略：不做 scrap，委托未来 egui UI 层（2026-09-20 定案）

- **scrap 不实现**(Tier 2 移除,与 camera 同列剪除)。理由:①pygame.scrap
  官方即 experimental,生态使用极少,弃之无损失;②**移动端剪贴板行为无法
  收拢**——Android 依赖 ClipboardManager JNI + 输入框/IME 长按交互,程序化
  API 在移动端无真实消费场景(用户定案);③wasm 的 navigator.clipboard
  带权限交互,价值/成本比低。
- **剪贴板需求的承接路径 = 未来 egui UI 层**:egui-winit 0.36 默认特性
  含 clipboard(内部 arboard,桌面三平台 text+image);egui TextEdit 控件
  自带 Ctrl+C/V 全套交互——引入 egui 时**零成本继承**,无需我们写任何
  剪贴板代码(已核实:egui-winit 0.36.2 / arboard 3.6.1)。
- **移动端定位**:剪贴板语义归输入法/输入框的平台原生交互,不走库 API
  (行为收拢不可能,文档标注);wasm 同理归浏览器 clipboard API 的页面级
  权限交互。
- 影响范围:base/clipboard 模块不再新建;Tier 2 相应瘦身;pygame.scrap
  兼容名标记"不支持"(与 camera 同列)。

---

## 五、实现排期（依赖序）

| Tier | 内容 | 前置 |
|---|---|---|
| **Tier 0** | Color✓ / Rect✓ / locals 重组 / version / math(glam) | 无 |
| **Tier 1** | display / key / mouse / event / time✓ / image / font / mixer+music(audio) | M1 闭环 ✓ |
| **Tier 2** | **Surface 三态统一** + draw(吸收 gfxdraw) + cursors | Tier 1 |
| **Tier 3** | mask / PixelArray / sndarray / surfarray / BufferProxy / sprite / touch / gamepad / dialog·io·net 映射 | Tier 2 |

- 排期原则（用户定案）：**先基本类型 → 低依赖独立模块 → 高层模块**；
- M6 导出批次与 Tier 对齐：每落地一个 Tier 段，对应面 A/B 注册 + pyi 桩
  同步冻结（四者同键）。

---

## 六、开放问题（实现期决策）

1. MRT 单 pass 多附件的 surface-group API 形状（v2）
2. GPU 后端像素回读的延迟语义细节（1 帧承诺 vs 轮询就绪）
3. ~~SysFont 的系统字体枚举~~（**已决**：不再支持系统字体，shim 路由标准内嵌字体；质量目标 = 内嵌字体多国语言覆盖）
4. ~~sprite 现代化的形态~~（**已决**：保留 pygame Group/Sprite 架构，不做 ECS；现代化 = 叠加开发者便利接口）
5. locals 位值是否对齐 SDL 数值（当前定案：自定值只承诺名字与语义）
6. ~~clipboard wasm 的 read 权限提示交互~~（随 scrap 不实现决策一并移除；egui UI 层若落地由其继承）

---

## 七、与绑定设计稿的关系

- 本文 = pygame 兼容层的**模块架构与 Surface 契约**；
- `pygame绑定层API设计稿.md` = **绑定架构**（feature 拓扑 / 生成器门 /
  原生面注册 / launcher）；
- 交汇点：面 A/B 注册表消费本文的模块清单；性能分层原则（§六）决定各
  模块的实现宿主列；行为回归测试集（M2）按本文 Tier 扩展脚本集。

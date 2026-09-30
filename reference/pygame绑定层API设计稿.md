# pygame 绑定层设计稿（最终版）

> 兄弟文档:`pygame兼容层模块架构.md`(pygame 兼容层的模块映射总表、
> Surface 统一架构、Tier 排期)——两文档互为犄角:本文管"怎么绑",
> 那篇管"绑出来的是什么"。

> 状态：**设计定稿 · 实现中**（M0~M4 已完成；M5 未动；M6 进行中——
> display/event/draw/time/render 已出，演进见 doc/log/starfish_changelog_2026-09-25.md
> 批次 28~32）。
> 本文是绑定工程的唯一契约。
> 上游依赖：base 的 B 门模型（`base::app`）、启动门、窗口 flags（见
> `architecture/` 各模块文档）。
> 决策推理链的完整历史在 `doc/log/starfish_changelog_2026-09-22.md`
> 批次 9~16；本文只陈述现状，文末附决策速查表与待定项。

---

## 一、定位与目标

- **产品形态**：`pygame` 兼容层——Python 开发者以 pygame 的心智模型书写，
  底层由 starfish base（winit + wgpu + cpal）驱动。
- **分层守恒**：
  - **base（Rust，恒编译）**：引擎零件（WindowConfig / RenderEntry /
    AudioMixer / Font / …），Rust 用户显式组装——**零 pygame 依赖，零
    Python 感知**；
  - **pygame 语义层（Rust，feature = "pygame"，默认开）**：Color/Rect 类型、
    绑定核心状态机（manifest/事件缓冲/帧步进）——解释器无关，Rust 用户
    可独立启用；
  - **内嵌壳 / launcher（Rust，feature = "rpy" / "launcher"）**：把 base 与
    pygame 两个映射模块注册进对应解释器（RP 内嵌 / CPython bootstrap）；
  - **pygame 兼容层（纯 Python）**：pygame 风格的模块函数与装配胶水，
    双解释器零分叉。
- **类型互通单向化（2026-09-20 定案）**：
  - base **零 pygame 依赖**（依赖方向严格单向：base ← pygame ← rpy/launcher）；
  - 双向类型转换**全部住在 pygame 层**：pygame→base 直接转换
    （`to_base()`），base→pygame 经高层类型接口（`Color::from_base()` /
    `From`，`pygame.XXX.from` 风格）——base/color.rs 的字节色桥接已迁入
    pygame 层，base 反向引用归零；
  - **entry 锁定**：使用谁的入口（`base::app::run` 纯 Rust 应用 /
    `pygame.run_script` pygame 脚本）即固定运行形态，但类型互通不受限。
- **运行时格局（2026-09-20 四修定案）**：
  - **主运行时 = 内嵌 RustPython**——引擎 + 解释器同一份二进制，桌面 /
    Web / Android 统一形态；
  - **CPython = 启动器（launcher）分发形态**——PyO3 API 绑定层已删除，
    CPython 只负责 `python -m starfish game.py` 把脚本转交内嵌壳；
  - **兼容边界 = 纯 Python + RustPython stdlib 白名单**。C 扩展（numpy 等）
    明确不支持——**这是特性不是缺陷**：强制生态收敛到纯 Python + Rust，
    换取绑定层维护量趋近于零。
  - **stdlib 白名单策略（2026-09-20 定案）**：CPython stdlib 并非绝对跨平台
    （wasm/mobile 本就残缺），**只冻结 native + wasm 双平台实测可用的常用
    子集**（dataclasses/enum/typing/collections/re/json/abc/functools/
    itertools/copy/weakref/types/math/time/random/struct/base64/zlib 级别，
    清单随 M2 行为回归逐个确认）——维护裁剪版 Lib 目录，freeze 指向它；
    缺失的原生模块（math 等）以 `add_native_module` 薄 shim 按需自补。
    二进制体积与兼容边界同时收紧，"支持哪些 stdlib"由我们的白名单定义，
    不由上游冻结范围定义。
- **核心设计目标**：
  1. Python 拿到的每个对象就是 Rust 对象的直通句柄（无拷贝、无 id 间接层）；
  2. 每个方法调用就是一次被 Rust 完整校验的接口调用（"保证在方法里，
     不在架构里"）；
  3. 声明顺序 ≠ 执行顺序：用户按 pygame 习惯声明（模板期记账），引擎按
     固定顺序执行（事件先行 → 尺寸就位 → 服务原子实例化 → 资源惰性加载 → 帧）。

---

## 二、生命周期即脚本顺序（非 class 线性形态，主推）

**原则**：时序保证已全部消化在引擎机制里（启动门等真实尺寸 / 服务原子
实例化 / 资源惰性句柄 / require 闸门）——**脚本不需要任何"结构"（类/方法表）
承载生命周期，线性阅读顺序 = 执行顺序**。这正是 pygame 灵魂的回归。

| 生命阶段 | 脚本位置 | 引擎保证 |
|---|---|---|
| 模板期（init + 窗口请求 + 资源声明） | `run` 之前的模块级代码 | 纯记账，零副作用；`run()` 封账：全量校验 + 依赖定序 |
| 底层对象实例化 | `pygame.run(game)` 内部（启动门） | 原子：事件先行 → 真实尺寸 → 服务以正确尺寸创建，失败整体退出 |
| 资源排水 | 资源构造即发起加载（桌面同步完成；Web 并行 fetch + 预算排水解码） | 恰好一次；发起序 = 声明序（完成序 Web 不承诺）；加载中调用 = 意图记忆，就绪自动执行（§5） |
| 帧循环 + 事件 | 生成器循环体（`while True:` 与 `while ctx.open` 皆可） | 每 vsync 步进一次（rAF / Poll） |

- 生成器形态：`def game(ctx)` 为生成器函数，`run` 逐帧 `next()` 步进；
  `yield` 是帧边界，`return` 是退出。类钩子形态
  （configure/load/event/update）保留为等价结构化选项。
- 退出兜底：用户漏处理 QUIT 时，窗口关闭由引擎对生成器 `GeneratorExit`
  强制收口，不会挂死。

---

## 三、最终 Python 形态（定稿脚本）

```python
import pygame

pygame.init()                                    # 全量初始化（经典语义：引擎按依赖定序，
                                                 #   调用顺序无关；pygame.mixer.init(channels=N)
                                                 #   仍可显式覆盖配置）
screen = pygame.display.set_mode((800, 600), pygame.RESIZABLE)   # 窗口请求（boot 时以真实尺寸物化）

# ── 资源（惰性句柄 → 引擎启动后自动加载）──
sound = pygame.mixer.Sound("shoot.wav")
music = pygame.mixer.music
music.load("bgm.ogg")

# ── 主循环（一个生成器 = 整个"主函数"）──
def game(ctx):
    music.play(-1)                               # 未就绪 → 就绪后自动播

    # 加载屏相位：声明的资源全部就绪前,循环绘制 LOADING(句柄 .ready 直查)
    while not (sound.ready and music.ready):
        ctx.gfx.clear((26, 26, 38))
        ctx.font.draw("LOADING", (10, 10))
        ctx.flip()
        yield

    while True:                                  # 经典写法完全可用
        for e in pygame.event.get():             # 事件对象支持 match 解构
            match e:
                case pygame.Event(type=pygame.QUIT):
                    return
                case pygame.Event(key=pygame.K_ESCAPE, down=True):
                    return

        ctx.gfx.clear((26, 26, 38))
        ctx.font.draw("HELLO", (10, 10))
        ctx.flip()
        yield                                    # 帧边界：交还引擎等下一个 vsync

pygame.run(game)
```

配套的 Python 语言特性：

- **PEP 562 模块级 `__getattr__`**：未 init 就访问 `pygame.mixer.Sound` →
  报错直接指引"请先 pygame.init()"，定位到调用行；
- **事件对象**：`dataclass(frozen=True, slots=True)` + 结构化 `match` 解构；
- 惰性句柄：`__bool__`（`if sound:` = 就绪态）与 `.ready` 属性——**加载屏
  模式**：未就绪循环画 LOADING,全真后进入正式渲染(两相位生成器,见上);
- `pygame.running` 模块属性：`ctx.open` 的镜像。

### 与经典 pygame 的差异清单（两条，均为机制必然）

| 经典 pygame | 本层 | 为什么消不掉 |
|---|---|---|
| 循环体自然执行到下一帧 | 循环末尾 `yield`（帧边界） | 引擎持循环：yield 是控制权交还的唯一通道（Web 无阻塞模型下唯一可行形态） |
| Python 拥有 main | 末尾 `pygame.run(game)` | 同上——脚本作为协程被引擎逐帧步进 |

`init_finish` 已删除（封账并入 `pygame.run`，见 §4.2）；`pygame.init()`
回归经典全量语义；`while True:` 完全可用。

---

## 四、绑定层内部设计（Rust 侧，用户不可见）

### 4.1 薄 App 类型 + 每模块实例槽 + 三时刻检测

winit 的事件循环与窗口只能被 Rust 结构体拥有——Python 不可能"拥有" winit。
因此 Rust 侧封装薄 App 类型（引擎锚点），每个子系统一个实例槽位；
Python 侧 `init()` 负责记账并缓存句柄——**实例存在性 = 正确初始化的检测**。

```rust
pub struct AppCore {
    manifest: Manifest,        // init 阶梯的声明记录
    services: Services,        // 各模块实例槽位
    pending: PendingLoads,     // 资源加载登记（弱引用，§5）
    // 不含任何 pygame 语义——语义全在 Python 层（保持"薄"）
}

pub struct Services {
    display: Slot<DisplayModule>,   // set_mode 请求 → boot 物化
    render:  Slot<RenderModule>,
    font:    Slot<FontModule>,
    gfx:     Slot<GfxModule>,
    mixer:   Slot<MixerModule>,
}
```

**双层存放（定稿决策）**：

| 层 | 存什么 | 为什么 |
|---|---|---|
| Rust 侧（services 槽位） | 服务实例**本体**（Arc） | 帧管线在 Rust（排水/灌事件/present 需直达服务）；生命周期由引擎逆序收口——服务不能被用户 `del` 掉 |
| Python 侧（模块变量缓存） | 句柄 + 存在性标记 | 存在性检查在 Python 层完成（友好报错、定位到脚本行） |

**检测的三时刻（三层防线）**：

| 时刻 | 检测 | 层 |
|---|---|---|
| `xxx.init()` / `set_mode` 调用时 | 记账 + 幂等 + 句柄缓存写入 | Python（友好报错定位到行） |
| `run()` 封账 + 启动门 | 全量校验；服务物化（真实尺寸就绪才创建） | 引擎（顺序保证） |
| 运行期任意调用 | require：App 槽位查询（None → pygame.error） | Rust 方法内（不可绕过的强保证） |

App 槽位与 Python 句柄指向同一实例（Arc）；槽位可置空（quit 后清 None、
拒绝后续调用）。

### 4.2 init 阶梯与 run 封账（模板期契约）

```text
pygame.init()          → 全量初始化声明（经典语义）：核心 + 全部子系统入清单，默认配置；
                         依赖定序由引擎保证（boot 按构造器链 AppCore → Mixer → Render →
                         Font/Gfx 实例化），用户调用顺序无关
pygame.mixer.init(…)   → 显式覆盖：同服务以参数覆盖默认
set_mode(...)          → 记录窗口请求（flags 随行）
pygame.run(game)       → 封账：
                           ① 全量依赖校验（font↔render 等），错误在 run 行报告
                             （仍在启动前、窗口出现前，fail-fast）
                           ② 模板期结束：此后 init 类调用 → 报错并提示"移到 run 之前"（v1 严格）
                           ③ 资源工厂调用不受影响（任意时刻可声明，惰性加载）
```

**check 的有效边界（运行期机制的适用域）**：运行期 API 内部同步自检前置
状态（require 闸门 / 数据槽查询）**全部成立且零仪式**——两帧之间引擎推进
状态，同步 check 便宜且准确。但该机制**仅适用于引擎运行后**：run 之前线程
只有一条且被 Python 占用，同步 check 的前置等待只有三种结局——阻塞（桌面
死锁；Web 禁止阻塞主线程）/ 返回未就绪（线性脚本无处可退，脚本即死）/
当场补做（winit 未醒、无 GPU、无真实尺寸，做不到）。"前置初始化是否完成"
在 run 之前没有真答案——解法不是 check 而是清单（先记账，boot 在有上下文
处一次做完）。**两阶段的存在理由即此，非设计偏好。**

**性质分界**：
- **服务配置 = 可预见** → run 封账，boot 原子实例化，失败整体退出；
- **资源 = 不可预见** → 不进清单、不进原子段，资源对象自带槽位 + 惰性
  加载承担，错误在使用点确定性报错。

### 4.4 统一渲染管线与标准渲染约定（2D≡3D，批次 26 定案方向）

**定位声明**：pygame 层不对齐经典 pygame 的 CPU 像素世界（SDL 式光栅），
而是以 **wgpu 统一渲染管线**承载全部绘制——2D 与 3D 不分家：
**2D = 正交相机 + 贴图四边形 + 关深度写入的 3D**（Unity 同构，被验证的
收拢路径）。代价与收益都已定案：放弃逐像素兼容（get/set_at 降级为显式
CPU 回读慢路径），换取 GPU 直绘、着色器能力与 3D 天花板。

#### 四支柱（统一模型的全部约定）

```text
① 统一顶点布局   position(3) + uv(2) + color(4) [+ normal(3) 3D 光照启用]
② 统一绑定布局   @group(0) 引擎 uniform(相机/对象变换)
                  @group(1) 材质: @binding(0) 纹理 + @binding(1) sampler
                                + @binding(2) 材质参数 uniform
③ 统一材质模型   ColorMaterial / TextureMaterial (+v2: 自定义 WGSL 遵循布局)
④ 统一绘制入口   with screen.render(...) as r:
                     r.draw(mesh, material, transform)   # 网格/精灵/形状同入口
```

**模式参数（2D/3D 唯一差异）**：相机 ortho/persp、坐标 y 向下(2D,对齐
pygame)/y 向上(3D)、深度策略 关闭+画家算法(2D)/深度测试(3D)。

**实现真相**：统一的是接口与约定;wgpu 混合状态属于管线对象,内部实现为
**小规模管线变体矩阵**(混合模式 × 是否贴图 × 图元类型,量级十几个),
绘制入口按参数自动选择并缓存——用户只面对一个 draw 入口。

#### 三层能力模型（BindGroup 的取舍消解）

BindGroup 的详细配置不该由用户写——它由"标准布局约定 + 材质参数"推导:

| 层 | 用户准备 | BindGroup |
|---|---|---|
| L0 预设材质 | Mesh + ColorMaterial/TextureMaterial | 材质内部自动构建(无纹理/纹理绑定) |
| L1 自定义 WGSL + 标准布局 | Shader(from_wgsl) + 材质参数 | 材质内部自动构建(group(1) 遵循标准布局,文档给模板) |
| L2 完全控制 | 显式 BindGroup/RenderPipeline(面 B 下沉) | 用户手写——自定义布局/storage/多纹理全可达 |

着色器编写难度**不由封装降低**(Shader 资源直通,`from_wgsl` 即用);
封装降低的是**资源绑定的仪式感**。

#### pygame 渲染能力映射清单（对齐验收表）

| pygame 能力 | 统一管线对应物 | 难度 |
|---|---|---|
| blit(位置/alpha) | 贴图四边形 draw + alpha 混合 | 低 |
| set_alpha(表面透明) | 材质参数 uniform | 低 |
| BLEND_ADD/MULT/MIN | 混合状态变体 | 低 |
| transform.rotate/scale/rotozoom | 顶点变换(连续坐标+线性采样,质量优于 CPU 旋转) | 低 |
| smoothscale vs scale | 采样器 linear vs nearest | 低 |
| draw.rect/circle/ellipse/polygon | GPU 图元 / fragment SDF(SDF 顺带 = aaline 抗锯齿) | 中 |
| draw.arc | SDF 扇形判定 | 中 |
| fill | pass clear 或纯色 quad | 低 |
| font.render | 字形图集(glyph atlas)+ quad 批渲染 | 中 |
| 离屏渲染(Surface 作目标) | render pass 指向 Texture-backed Surface | 中 |
| get_at/set_at/pixelarray | 显式 CPU 回读慢路径(标注性能悬崖) | 已声明 |
| 8-bit 调色板 surface | 放弃(历史遗留) | — |

#### 类型归属（双面分工）

- **面 A(pygame)**:Surface(统一资源抽象:贴图/渲染目标)、render() 作用
  域、draw 入口、事件/时间——**绘制入口与便利函数,无 GPU 资源类型包装**;
- **面 B(starfish)**:GPU 资源类型本体——`Mesh`/`BindGroup`/
  `RenderPipeline`/`ShaderModule`/`Texture`,高级与自定义场景直通
  (面 A 绘制入口直接收面 B 的 Mesh;自定义布局走面 B 显式构建);
- **互通**:Surface→纹理绑定、面 B Mesh 直入面 A 绘制(层级规则允许)。

### 4.3 对象直绑模型（无注册表）

```rust
#[pyclass]
struct PySound {
    path: String,
    data: RefCell<Option<SoundData>>,   // None = 加载中（仅 Web 异步）；Some = 就绪
    err: RefCell<Option<AudioError>>,   // 失败态（调用点确定性报错）
    pending_play: RefCell<Option<i32>>, // 加载中的 play 意图记忆（就绪自动执行）
}

#[pymethods]
impl PySound {
    #[new]
    fn new(path: String) -> Self {
        // 桌面：同步读+解码填入（pygame 原味，零队列零延迟）
        // Web：记录 path，spawn_local fetch；解码进 App pending 集合，
        //      每帧预算排水（防一帧集中解码卡帧）
    }
    fn play(&self, loops: i32) -> PyResult<()> {
        let mixer = app_require_mixer()?;
        if let Some(e) = &*self.err.borrow() { return Err(into_pygame_error(e)); }
        match &*self.data.borrow() {
            Some(d) => { mixer.play(d.clone(), loops)?; }        // 就绪：直达
            None => { *self.pending_play.borrow_mut() = Some(loops); } // 加载中：意图记忆
        }
        Ok(())
    }
}
// 加载完成写回（spawn_local 回调 / 排水晋升）时：检查 pending_play → 自动执行
```

**三层机制（"惰性句柄 → 自动加载"的准确形态）**：

- **对象层（自治）**：数据、方法、加载状态、待执行意图全部住对象自己
  ——没有中央行为队列。加载中的 `play()` = 意图记忆，就绪写回时自动执行；
  失败态在使用点确定性报错。
- **App 层（登记，非注册表）**：加载中的对象以弱引用登记进 App pending
  集合——只为三件事服务：每帧预算排水、就绪判定（排水完成 = 全部就绪，
  供句柄 `.ready` 翻转）、错误传播。**不是** id→数据的中央映射——数据
  永远住对象里；聚合计数不上 Python 面（句柄直查,见定稿脚本加载屏）。
- **平台层（差异摊平）**：桌面同步加载，pending 集合瞬时清空；Web 并行
  fetch，**发起序 = 声明序**（构造即发起），**完成序不承诺**。

- **Web 异步加载写回**：加载任务持 `Py<PySelf>`，完成后写回槽位
  （冲突时 try_borrow 重试）。

> **实施修订（2026-09-25，批次 32）**：Texture 类逐帧 `draw` 与 Sound 的
> 一次性 `play` 语义不同——未就绪帧跳过、就绪自动出现，**意图记忆与
> App 层登记对纹理整层可省**（对象层三态句柄 + 平台流水线即足）。
> 统一装载方案与 Web 单 Device 收敛见
> `reference/资源异步装载统一方案.md`；Device/窗口生命周期解绑见
> doc/log/starfish_changelog_2026-09-25.md 批次 31。

### 4.4 HookApp：全库唯一的 Application 实现

winit 回调只能被 Rust 结构体接收——base 提供钩子槽类型（**base 唯一新增
的 pygame 导向基座**；starfish 本就是"次世代 pygame"，此类基座进本体
名正言顺）：

```rust
// base::app（纯 Rust，零 pygame 语义；Rust 用户亦可直接用）
pub struct HookApp {
    pub on_start: Option<Box<dyn FnMut(&mut Ctx)>>,
    pub on_event: Option<Box<dyn FnMut(&Window, &WindowEvent, &mut Ctx)>>,
    pub on_frame: Option<Box<dyn FnMut(&mut Ctx)>>,
}
impl Application for HookApp { /* starfish 内唯一实现，逐钩子分发 */ }
```

pygame-core 组装状态机后填入 HookApp 钩子；壳只负责"钩子里如何进入 VM"
（内嵌壳 = `vm.enter`）。同类问题的处置：`AudioEffect`（DSP）与
`Connection`（自定义传输）trait → **v1 Rust-only**，Python 侧使用内建
实现（文档标注能力边界）。

### 4.5 线程契约

GIL 单线程模型：绑定层全部调用落在主线程（GIL 协程与 RP 单 VM 天然同构）。
base 线程契约保留为 Rust 层纪律（主线程：run/窗口/事件/present；任意线程：
AudioMixer/MusicPlayer/SFX）——多线程归编译型语言用户自理。

---

## 五、工程拓扑（独立 crate：pygame-rs；2026-09-23 六修，反转"不建新 crate"）

**Python 绑定整体抽离为独立 crate `pygame-rs/`**（starfish 仓库根下，独立
lockfile/target，非 workspace 成员）——starfish 回归零 Python 资产的纯引擎。
原"单 crate + feature 组件化"方案（五修）随抽离作废：feature 门控不再需要
（资产物理移出后，starfish 默认图**天然**零 Python 依赖）。

### 5.1 crate 表（六修后）

| crate | 内容 | 外部依赖 | 分发 |
|---|---|---|---|
| `starfish` | base 引擎本体（渲染/窗口/循环/audio/time/…），**零 Python 资产** | 与 pygame/rpy 无关 | 纯 Rust 用户直用 |
| `pygame-rs` | 绑定层整体：`src/rpy`（内嵌 RustPython 壳）→ `src/pygame`（语义层，RP-free）→ starfish（path 依赖） | rustpython-vm/pylib = **vendor main 本地路径**（0.6.0-dev 快照,M0+M3 锁定,去 host_env;见 §九.6） | 引擎 wasm/APK/桌面二进制 |
| `launcher`（M5，落 pygame-rs） | CPython 微型 bootstrap：`python -m starfish game.py` | pyo3（折入 `extension-module`） | maturin wheel（附 `pygame-rs/python/` 资产，建议 abi3） |

- **依赖方向**：`pygame-rs → starfish` 单向；crate 内部 `rpy → pygame →
  starfish::base`——pygame 语义层不背解释器（解释器仅存在于 rpy 模块，
  wasm 门控 `#[cfg(not(target_arch = "wasm32"))]`，M3 落地时调整）。
- **纯 Rust demo 保证**：starfish 源码树内零 pygame/rpy/python 路径——
  隔离从"cfg 纪律 + CI 检查"升级为**物理隔离**（编译器无需参与执法）。
- base↔pygame 颜色桥接住在 pygame-rs 的 `pygame/color.rs`（base 本体不反向
  依赖 pygame 的既有纪律不变）。
- **双绑定模块（2026-09-23 定案）**：Python 侧两种使用形式——① `pygame`
  绑定 = 状态机形式（`pygame.run` 生成器门,现有兼容层）；② `starfish`
  绑定 = 纯引擎原生接口形式（无内置状态机,M6+）。层级规则：pygame 模式
  下可用 starfish;**纯 starfish 模式下不推荐再用 pygame 类型**。
- **绑定入口标准 + 实现区（批次 26/28 定案）**：`bindings.rs` 只定义
  **入口标准**——`BindingModule` trait（`name()` + `register(vm)`）+
  `BINDINGS` 静态表 + `register_all` 总入口;具体绑定实现按包下沉
  `src/rpy/dependencies/<包名>/`(pygame 绑定 = `dependencies/pygame/`)。
  脚本侧接口 = crate 顶层 `pygame/`、`starfish/` 的**纯 .pyi 桩**
  (批次 26 全原生化后零 .py)。"Rust 在 src、接口看顶层桩、实现看
  rpy/dependencies"一眼可读;新增绑定包 = 加目录 + 实现 trait + 表加一项,
  装载器零改动。

### 5.2 源码落位（含 2026-09-23 抽离后形态）

```
starfish/（仓库根：引擎本体 + 独立绑定 crate）
├── src/                   # base 全模块 + HookApp + app_entry!        [已有 ✓]
│                          #   零 pygame/rpy/python 路径(物理隔离)
├── pygame-rs/             # ★ Python 绑定层（独立 crate，cd 操作）
│   ├── pygame/            #   绑定① 接口面（纯 .pyi,零 .py）         [已有 ✓]
│   │   ├── __init__.pyi   #     包根契约:常量/Event/init 阶梯/run     [已有 ✓]
│   │   ├── display/event/draw/time.pyi    #   子模块契约              [已有 ✓]
│   │   └── （_native.pyi 已删——批次 26 全原生化,原生面即公开面）
│   ├── starfish/          #   绑定② 纯引擎原生接口（面 B）           [M6+]
│   │   └── __init__.pyi   #     占位标归属
│   ├── examples/          #   pygame_hello + stack_probe(T9)         [已有 ✓]
│   ├── web/               #   web 测试包(html/js/wasm)               [已有 ✓]
│   └── src/
│       ├── pygame/        #   语义层（RP-free）                      [已有 ✓]
│       │   ├── core.rs    #     状态机：manifest/事件缓冲/帧步进       [已有 ✓]
│       │   └── color.rs · rect.rs                                    [已有 ✓]
│       └── rpy/           #   壳:engine_main/槽/统一帧管线           [已有 ✓]
│           ├── mod.rs     #     引擎主程/槽/GPU 服务(pub(crate))     [已有 ✓]
│           ├── bindings.rs  #   入口标准:BindingModule+register_all [随批次 28 ✓]
│           └── dependencies/
│               ├── pygame/   # 绑定① 实现:register+常量+Event+函数   [随批次 28 ✓]
│               │   ├── mod.rs    # PygameBinding+注册+f_*            [已有 ✓]
│               │   ├── event.rs  # PyEvent 原生类(pyclass 静态类型)  [已有 ✓]
│               │   ├── screen.rs # Screen 原生类(set_mode 返回)      [已有 ✓]
│               │   └── render/   # pygame.render 绑定(Texture 三态   [已有 ✓]
│               │           #   异步句柄+帧批+材质注册表)
│               └── RustPython-main/  # 第三方解释器源码快照          [已有 ✓]
├── examples/probe/        # 现有探针 + 未来 probe_python（验收通道）
├── spike/                 # M0 探针(独立项目,不进构建;M5 后删除,结论在 REPORT.md)
└── xtask/                 # 现有工具（--dir 服务兄弟 crate）         [已有 ✓]
```

> 进度口径：`[已有 ✓]` = 已生长;`[Mx]` = 归属里程碑,未到不动——
> 目录树是**终态**,里程碑负责到达,不存在偏离。
> **分层红线**：`rpy/` 是 pygame 的**兄弟**而非子模块——解释器永不进入
> pygame 语义层;starfish(base) 反向引用 pygame 亦为零(类型互通住
> pygame 层,§一)。

- **绑定全原生化（批次 26）**：常量/Event 类/函数/子模块全部由 Rust
  构建注册(`BindingModule::register`),运行时不加载任何 Python 源码;
  顶层 `<包名>/*.pyi` 仅为接口契约桩(IDE/文档)。
- **构建关系**：独立 crate、自带 lockfile/target——根 Cargo.toml 保持单包；
  依赖经 `starfish = { path = ".." }` 单向（`wgpu`/`serde` 系以同版本需求
  与 starfish 合并为同一实例，特性并集不受扰动）。
- **xtask**：M4 的 Android 入口与 M3 的 wasm 打包落 pygame-rs 侧再议
  （`FEATURE_ANDROID_SUPPORT` 的 `rpy` 行随 M4 处理）。
- ** launcher 产物**：maturin wheel = 微型 bootstrap 扩展（`starfish/_launcher`
  或经 `python -m` 入口）+ `pygame/` + `starfish/` stub 包；abi3 一份通吃。
  `extension-module` 与 `cargo test` 已知互斥 → launcher 特性验证走
  maturin develop + 冒烟，不走 cargo test。

---

## 六、原生面与 .pyi 桩契约

**双面注册表，一次注册（仅存在于内嵌壳的 RP 世界）**：

| 面 | 模块名 | 内容 | 受众 |
|---|---|---|---|
| **面 A** | `pygame` 模块本体 | 常量/Event 类/init 阶梯/display·event·draw·time 函数/run 生成器门——批次 26 全原生化(`_native` 桥消失,原生面直接挂公开模块) | pygame 脚本(原纯 Python 兼容层的原生替代) |
| **面 B** | `starfish` | **base 直绑（pygame 性能层的实现宿主，见性能分层原则）**：Texture / Mesh / AudioMixer / StreamVoice / Sound / Music / Video / Rect / Color…（§一 直通句柄的分批导出） | 穿底用户；pygame 层自身也建于面 B 之上 |

- 共享同一批类型对象（pygame 的 Surface 包装的就是 `starfish` 面里的
  Texture，同一 `#[pyclass]` 实例，零转换）。
- 注册机制（rpy 壳）：`vm.ctx.new_function` / `PyClassImpl` + `extend_class`；
  原生函数签名惯例 `Fn(&VirtualMachine, FuncArgs) -> PyResult`；issue #3556
  以"单 Interpreter 长持有"规避。
- **性能分层原则（2026-09-20 修订，替代初稿"表面最小化"）**：单壳内嵌后
  原生调用 = 同进程直接 Rust 函数（无动态库边界/无 GIL 管理），且 RP 解释
  器慢于 CPython → **热路径下沉 Rust，冷路径上浮 Python**：
  - 下沉 Rust（性能敏感，多为 base 既有本体的直绑）：Rect/Color
    （src/pygame/{color,rect}.rs Phase 4 已有）、Surface 像素操作、
    draw.* 光栅化（base/gfx 16 形状）、font（base/font）、mixer/audio、
    time、image、transform（待建）
  - 上浮 Python（冷路径胶水，写一次）：init 阶梯记账、模块组织、
    事件路由封装、友好报错、装配模式
  - 判据：一段逻辑若在每帧/每资源调用 → Rust；若每游戏只执行一次 → Python

### 窗口 flags 映射（`set_mode` 的 flags → base WindowConfig）

base `WindowConfig` flag 集（已落地）：`resizable` / `maximized` /
`decorations`（无边框）/ `transparent`（**创建期一次性**）/ `always_on_top` /
`fullscreen`（Borderless）/ `visible` / `cursor_visible`；运行时对应
`Window::set_*` 方法族。pygame flag 翻译（纯 Python 层一个映射函数；
**常量位值实现期自定，只承诺名字与语义**）：

| pygame flag | base 字段 |
|---|---|
| `FULLSCREEN` | `fullscreen: true` |
| `NOFRAME` / `WINDOW_BORDERLESS` | `decorations: false` |
| `RESIZABLE` | `resizable: true` |
| `WINDOW_TRANSPARENT` | `transparent: true`（创建期一次性） |
| `WINDOW_ALWAYS_ON_TOP` | `always_on_top: true` |
| `WINDOW_HIDDEN` | `visible: false` |
| `OPENGL` | —（base 恒 wgpu，pygame 层显式报错） |

鼠标类（`WINDOW_MOUSE_GRABBED` 等）不走创建 flag——运行时经
`Window::set_cursor_grab`（Confined 仍可见）/ `set_relative_mouse`
（Locked + 隐藏）/ `set_cursor_visible`。平台降级矩阵见
`architecture/window.md`。

### .pyi 桩体系（模块化多文件）

**面 B = stub 包，镜像 `src/base/` 一比一**（命名空间与 Rust 同构）：
`starfish/render.pyi`、`starfish/audio.pyi`、`starfish/video.pyi`…；
面 A 桩 = `pygame/__init__.pyi` + 各子模块 .pyi(surface/image/mesh/
material 等随实现批次增长),直接描述公开模块面(批次 26 全原生化后即
完整契约,无 `_native` 中间层);starfish 桩随 M6 按同模式镜像。

**四者同键**：Rust 模块 ↔ pyi 文件 ↔ 注册函数 ↔ M6 导出批次——改哪个
模块的 API 就动哪个 pyi/哪个注册。**桩 = 契约本体**：手写为源（接口定稿
文化），rpy 注册向桩实现，行为回归测试执法；`pyo3-stub-gen` 类生成作为
漂移缩减选项（待定项），不作初始形态。

---

## 七、里程碑

| 阶段 | 内容 | 性质 |
|---|---|---|
| **M0** | RustPython 嵌入 spike：三语言特性（生成器帧边界 / `match` 解构事件 / dataclass slots + PEP 562）+ stdlib 子集（dataclasses/enum/typing/collections）+ **sys.modules 注入式装载** + **RP-in-CPython 进程共存** | ✅ **完成**（单点决策门放行,报告 `spike/m0/REPORT.md`） |
| **M1** | 绑定核心 + 内嵌壳桌面 hello（cargo 入口；主运行时的开发主循环） | ✅ **完成**（全链路实测） |
| **M2** | 行为回归测试集（金标准，内嵌壳；单运行时——launcher 冒烟并入 M5） | ✅ **完成**（71 passed） |
| **M3** | 内嵌壳 → wasm（xtask web 打包子命令 + probe 无头判读） | 🚧 **编译已通**（vendor main + 去 host_env）；剩余 web 入口 + 浏览器实测 |
| **M4** | 内嵌壳 → Android（xtask APK） | 🚧 编译已通（psm 需 NDK CC 环境变量,APK 流程自动注入）；入口待做 |
| **M5** | launcher wheel（maturin + abi3 + 启动器冒烟）→ PyPI | |
| **M6+** | 模块导出批次：display/event/time/key → mixer → font/gfx → draw/image → video/gamepad/net/dialog | 与 pyi 桩逐批冻结同步 |

---

## 八、决策记录（速查）

| # | 决策 | 被否备选 | 核心理由 |
|---|---|---|---|
| 1 | 主运行时 = 内嵌 RustPython | Pyodide JS 桥 / 引擎 emscripten 化 | CPython wasm = emscripten 工具链与引擎 unknown-unknown 二进制不相交；Android libpython 交叉编译地狱 |
| 2 | CPython = 启动器，PyO3 API 面删除 | PyO3 全量双壳 | 纯 Python 边界下双解释器执行冗余；省整个绑定层维护量；强制生态收敛纯 Python + Rust |
| 3 | 单 crate + feature 组件化 | 多 crate workspace | 与既有 7 特性同构；xtask/示例/CI 零改动 |
| 4 | `init_finish` 删除，run 封账 | 显式封账调用 | 职责等价迁移，少一个用户必记概念 |
| 5 | 生成器门（yield） | init-yield 闭环 / async 门 / run 前 check 等待 | 模块体不能 yield；sync 调用不能隐式挂起；run 前无上下文且单线程不可等待——各自推理链见 09-22 日志批次 9~11 |
| 6 | include_str! 模块图 | rustpython freeze | 免工具链；对齐 kit 内嵌字节哲学 |
| 7 | GIL 单线程 | free-threaded 3.14t | wasm 稳定运行；多线程归编译型语言用户 |
| 8 | 不支持 CPython C 扩展生态 | —（用户定案） | 特性非缺陷：强制纯 Python + Rust 生态，维护量趋零 |
| 9 | **类型互通单向化**：base 零 pygame 依赖，双向转换住 pygame 层 | base 内建桥接方法（base/color.rs 曾挂 from_byte_color，已迁出） | 依赖严格单向 = base 永不被 pygame 语义污染；entry 锁定下类型互通不受限；转换住 pygame 层符合孤儿规则自然形态 |
| 10 | **RP 依赖 = vendor main 本地路径**（0.6.0-dev,去 host_env） | crates.io 0.5.0 | 0.5.0 对 wasm 编译损坏（_io/os 无门控引用 crt_fd）+ malachite 打包 bug；vendor 使 wasm 打通且 RP main 可用（用户供源）；升级 = 重下源码包 |

---

## 九、待定项（实现期决策）

1. ~~`pygame.run` 签名细节~~ **已定**：fps/vsync 归窗口侧（WindowConfig.fps_cap 既有通道），run 只收生成器——帧率是显示阻塞策略，不进 run 签名
2. ~~`ctx.assets.ready/loading` 粒度~~ **已决:不设 ctx.assets**(2026-09-20)——
   句柄各持 `.ready`/`__bool__`,脚本直查(加载屏模式见 §三 定稿脚本);
   引擎侧聚合计数仅为排水判据,不上 Python 面
3. 事件钩子先于 load 到达的缓存策略（现定义：句柄未就绪跳过,事件丢失——
   与 pygame 桌面行为一致）
4. Web 资源路径语义与预加载 API 形状
5. WebGPU 无 Srgb 格式的观感差异 → 上层 gamma 补偿（远期）
6. ~~RustPython 版本锁定策略~~ **已定（M0+M3 实测,2026-09-20）**：**vendor
   main 本地路径**（vendor/RustPython-main,0.6.0-dev 快照,非 crates.io
   0.5.0——后者 wasm `_io`/`os` crt_fd 编译损坏 + malachite 打包 bug 双缺陷,
   均为上游发布问题;main 修复了 wasm 且 match 可用）。vm 特性 =
   default-features=false + [compiler, wasmbind, gc, stdio, importlib,
   encodings, freeze-stdlib]（**去 host_env,wasm 必需**）。
   ⚠ 0.5.0 的 rustpython-stdlib 存在 malachite 打包 bug 不可用 → 原生 stdlib
   模块(math/_opcode…)缺失记入兼容边界;dataclasses 不可用 → pygame 层
   Event 用普通类自实现。详见 `spike/m0/REPORT.md` 与 09-22 日志批次 16。
   **零魔改承诺**:vendor 装载脚本只做"解压原样 + symlink 材料化",RP 源码
   零修改(wasm 修复全在 features 声明侧);切回 crates.io = 依赖行改版本号,
   一行完成。切回操作步骤见 `reference/vendor与crates切换指南.md`。
7. ~~PyPI 发行名~~ **已定：`pygame-rs`**（用户定案；import 名恒 pygame，`pygame` 被原版占用）
8. `.pyi` 维护演进（契约优先手写；stub-gen 为漂移缩减选项）
9. 面 B 导出批次表（随 M6 逐批冻结进桩）
10. ~~CPython 入口形态~~ **已定：`python -m starfish game.py`**（稳健路线；首行接管 `import starfish` + `os._exit` 作为进阶用法后补）
11. launcher 期间 GIL 释放细节（allow_threads 包裹 bootstrap 全程）

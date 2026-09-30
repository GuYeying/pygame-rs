# pygame sprite 的 Godot 式重设计（细案 v0.1）

> 2026-09-27 立项（用户定案）：pygame 原版 sprite（类继承 + 信号式
> Group）使用者极少且形态陈旧，**参照 Godot 概念重新设计**，不做
> 1:1 兼容。本细案为批次十四实施依据。

## 一、概念对照（取 Godot 之骨，去解释器之形）

| Godot 概念 | 采纳 | 本层对应物 |
|---|---|---|
| Node2D（transform/visible/z） | ✅ | `Sprite` 数据字段：position / rotation(度) / scale / visible / z |
| Sprite2D（texture/centered/offset/flip/modulate） | ✅ | texture（Surface 句柄）/ centered（默认 true）/ offset / flip_x·y / alpha |
| AnimatedSprite2D（frames + fps） | ✅ | `frames: Vec<Surface>` + `fps` + 内部帧计数（`tick(delta)` 推进） |
| z_index / Y-sort | ✅ | `z: i32`；同 z 按 y 升序（Y-sort 可选开关）——绘制排序双键 |
| Groups（Godot 分组标签制） | ✅ | 标签集 `tags: Vec<&str>`（按标签过滤遍历） |
| 场景树父子层级 | ❌ v2 | transform 层级（父子跟随） pygame 用户无此习惯；v2 按需立 `attach` |
| 信号系统 / _process 回调 | ❌ | **违反 poll 哲学**（调用方持循环）：用户在自己循环里 `group.tick(delta)` + 查询遍历 |
| hecs/ECS | ❌ | 手写池化容器足够（sprite 量级 ≤ 千；ECS 依赖与心智不值） |

## 二、Rust 形态（签名级草案）

```rust
let mut g = sprite::Group::new();
let id = g.insert(Sprite::new(&face).at(100, 100).scale(0.5).z(1).tag("enemy"));
g.set_position(id, (200, 100));
g.move_by(id, (-4, 0));            // 或逐字段
g.tick(delta);                      // 推进动画帧计数

{ let f = screen.render();
  g.draw(&f);                       // 按 (z, y) 排序批量入 batch（ADR-5 会话）
  f.end(); }
display::flip();

for id in g.with_tag("enemy") {     // 分组遍历（查询制，非回调）
    if screen_rect.contains(g.position(id)) { /* ... */ }
}
g.remove(id);                       // 世代 id：remove 后旧 id 自动失效
```

- **存储**：`Vec<Option<SpriteData>>` 池 + `(usize, 世代)` 句柄——
  remove 稳定、遍历缓存友好、零依赖（不引 ECS）；
- **绘制**：`Group::draw(&impl DrawTarget)` 按 (z, y) 排序 → 逐 sprite
  `push_quad_corners`（rotation/scale/flip 全走仿射四角，transform 的
  GPU 采样路径复用）；alpha = 顶点色乘制；
- **碰撞**：不内置信号——`g.rect(id) -> Rect` / `mask::Mask` 组合由
  用户判断（保持分层：sprite 管组织，Rect/Mask 管判定）。

## 三、排期与验收

- 单批次实施（估 ~400 行 + 单测 + 探针扩展一节），排 P5 收官批之后；
- 验收 = 精灵群 demo（动画 + z 排序 + 分组过滤 + 碰撞计数）三平台。

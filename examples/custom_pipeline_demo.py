# custom_pipeline_demo（渲染架构批次 C/D）：自建 WGSL 管线 + raw_render
# 直绘会话——底层定制化全链路验证。
# 演示形态：**fragment shader 程序化波纹圆盘**（SDF 圆形边界 + time
# 驱动同心波纹扩散——引擎内置绘制[fill/blit/draw]无法实现的逐像素
# 效果，自定义 WGSL 的价值所在）。
# ① access() 资源链：Shader/Mesh/BindGroup/Pipeline（安全收参）
# ② raw_render 相机 = **完全自管**（与全局相机零耦合）：各会话持
#    独立 buf + 独立 bind（布局同构共享 pipeline），矩阵经
#    `ac.write_buffer(cam_buf, cam.mvp())` 直写（write_buffer 特判
#    收 Mat4）——不 set_camera / 不 write_camera_mvp。
# ③ 同帧双会话：大圆盘（自建正交原位）+ 中央小圆盘（自建透视微缩）。
import pygame

WGSL = """
struct Camera { mvp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> u_time: f32;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
};
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world: vec2<f32>,
};

@vertex
fn vs_main(in: VIn) -> VOut {
    var o: VOut;
    o.pos = camera.mvp * vec4<f32>(in.pos, 1.0);
    o.color = in.color;
    o.world = in.pos.xy;
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // SDF 圆盘 + 同心波纹（引擎内置绘制无法实现的逐像素程序化效果）
    let c = vec2<f32>(400.0, 340.0);
    let d = distance(in.world, c);
    let r = 235.0;
    let edge = smoothstep(r, r - 5.0, d);
    let ring = 0.5 + 0.5 * sin(d * 0.13 - u_time * 5.0);
    let glow = 1.0 - smoothstep(0.0, r, d) * 0.55;
    let col = mix(vec3<f32>(0.05, 0.1, 0.35), vec3<f32>(0.15, 0.95, 1.0), ring) * glow;
    return vec4<f32>(col, edge);
}
"""


async def main():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 17)
    title = cjk.render(
        "下=2D 正交对照（恒定）；中央=3D 透视动态盘（摆动+呼吸，2D 做不出的投影变形）",
        True, (205, 205, 205))
    _fail = []

    def st(name, cond):
        if cond:
            print("[cp-demo] PASS", name)
        else:
            _fail.append(name)
            print("[cp-demo] FAIL", name)

    # ── 资源链（一次性构建；须 set_mode 后）──
    ac = pygame.render.access()
    shader = ac.shader(WGSL, label="demo_shader")
    # 500×500 quad（pos3color4——颜色基色，圆盘形态/波纹全在 fragment）
    x0, y0, x1, y1 = 150.0, 90.0, 650.0, 590.0
    verts = [
        x0, y0, 0.0, 1.0, 1.0, 1.0, 1.0,
        x1, y0, 0.0, 1.0, 1.0, 1.0, 1.0,
        x1, y1, 0.0, 1.0, 1.0, 1.0, 1.0,
        x0, y0, 0.0, 1.0, 1.0, 1.0, 1.0,
        x1, y1, 0.0, 1.0, 1.0, 1.0, 1.0,
        x0, y1, 0.0, 1.0, 1.0, 1.0, 1.0,
    ]
    mesh = ac.mesh("pos3color4", verts, label="demo_mesh")
    # 双会话各持独立 buf + 独立 bind（布局同构——同一 pipeline 复用）
    cam_buf = ac.buffer(64, label="cam_2d")
    cam_buf3 = ac.buffer(64, label="cam_3d")
    time_buf = ac.buffer(16, label="demo_time")
    # indirect 绘制参数 buffer（u32×4：6 顶点 ×1 实例 从 0 起）——
    # 段 2 经 draw_indirect 消费（与 draw_mesh 全量等价）
    ind_buf = ac.buffer(16, label="draw_indirect", usage="indirect")
    ac.write_buffer_u32(ind_buf, [6, 1, 0, 0])
    cam_bind = ac.bind_group(
        [(0, "uniform", cam_buf), (1, "uniform", time_buf)],
        label="bind_2d")
    cam_bind3 = ac.bind_group(
        [(0, "uniform", cam_buf3), (1, "uniform", time_buf)],
        label="bind_3d")
    pipe = ac.pipeline(shader, mesh, [cam_bind], label="demo_pipeline")

    st("mesh 顶点数 6", mesh.vertex_count() == 6)
    st("mesh 无索引", mesh.index_count() == 0 and not mesh.has_index_buffer())
    st("screen_format 查询可用", len(pygame.render.screen_format()) > 0)

    frame = 0
    asserted = False
    # 两个自建相机（与全局相机零耦合）：
    # - 正交（2D 相机对照）：恒定——盘原位 1:1，波纹世界波长直显
    # - 透视（3D 相机验证）：**动态**——绕 Y 摆动 ±25° + 距离呼吸
    #   900↔650 → 中央盘呈现透视梯形倾斜 + 尺寸呼吸（近大远小——
    #   2D 正交绝对做不出的 3D 投影特征，即"3D 相机在工作"的可视证明）
    M = pygame.math
    cam_ortho = pygame.render.camera_ortho(800.0, 600.0)
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        t_sec = frame / 60.0
        # 动态透视 view：T(后移) × R_y(摆动) × T(物体中心→原点)
        dist = 900.0 - 250.0 * (0.5 + 0.5 * (t_sec * 0.8 % 2.0 - 1.0))
        yaw = 25.0 * (t_sec * 0.7 % 2.0 - 1.0)
        view3d = (
            M.mat4_translation(0.0, 0.0, -dist)
            .mul_mat4(M.mat4_from_quat(
                M.quat_from_axis_angle(0.0, 1.0, 0.0, yaw)))
            .mul_mat4(M.mat4_translation(-400.0, -340.0, 0.0))
        )
        cam3d = pygame.render.camera_perspective(
            0.9, 800.0 / 600.0, 0.1, 2000.0).with_view(view3d)

        ac.write_buffer(time_buf, [t_sec])
        # 动态几何写入路径验证（update_mesh 每帧就地更新——内容不变
        # 零视觉风险；容量契约：顶点数 ≤ 建时数量）
        ac.update_mesh(mesh, verts)
        # 自管相机：矩阵直写各自 buf（Mat4 特判；零全局相机参与）
        ac.write_buffer(cam_buf, cam_ortho.mvp())
        ac.write_buffer(cam_buf3, cam3d.mvp())

        with screen.render() as t:
            if frame == 0:
                st("screen_attachment 帧内可取",
                   pygame.render.screen_attachment() is not None)
            t.fill((14, 15, 22))
            t.blit(title, (14, 10))
            # ── 段 1：2D 正交（对照恒定）——大波纹圆盘（draw_mesh 全量，
            #    免手填顶点范围）+ debug 标注 + 动态状态命令 ──
            with t.raw_render() as p:
                p.push_debug_group("ortho_disc")
                p.set_pipeline(pipe)
                p.set_bind_group(0, cam_bind)
                p.set_blend_constant(0.0, 0.0, 0.0, 0.0)
                p.set_stencil_reference(0)
                p.draw_mesh(mesh)
                p.insert_debug_marker("ortho_disc_done")
                p.pop_debug_group()
            # ── 段 2：3D 透视（动态验证）——同一 mesh，独立 buf3/bind3；
            #    draw_indirect 路径（范围参数在 u32 buffer 内）──
            with t.raw_render() as p:
                p.set_pipeline(pipe)
                p.set_bind_group(0, cam_bind3)
                p.set_mesh(mesh)
                p.draw_indirect(ind_buf, 0)

        pygame.display.flip()
        frame += 1

        if not asserted and frame == 60:
            st("raw_render 60 帧排程不抛", True)
            st("全程未 set_camera 状态恒 auto2d",
               pygame.render.camera_state() == "auto2d")
            asserted = True
            print("[cp-demo] SUMMARY",
                  "PASS" if not _fail else "FAIL " + str(_fail))
        await pygame.next_frame()

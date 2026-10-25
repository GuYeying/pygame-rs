# custom_pipeline_demo（渲染架构批次 C/D）：自建 WGSL 管线 + raw_render
# 直绘会话——底层定制化全链路验证。
# ① pygame.render.access() 两入口取资源工厂 → 自建 Shader/Mesh/
#    BindGroup/Pipeline（安全收参：布局预设/枚举字符串）
# ② 全局相机定约：group0/b0 = 64B 列主 mat4 MVP——raw_render(camera=
#    buf) 会话托管：收口快照、回放时写 buf（同帧 2D/3D 相机来回切、
#    多 raw 会话共享同一 buf 各自配对——即时写+延迟回放覆盖根治）
# ③ with t.raw_render(camera=cam_buf) as p: 命令记录 → 收口排程 →
#    目标提交时点回放（恒在游戏内容之上）
# 上半：正交 quad（原尺寸）；下半：透视 quad（同 mesh 同 buf，微缩
# 偏移——两会话各自配对的可视证明）。
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
    @location(1) center_dist: f32,
};

@vertex
fn vs_main(in: VIn) -> VOut {
    var o: VOut;
    o.pos = camera.mvp * vec4<f32>(in.pos, 1.0);
    o.color = in.color;
    o.center_dist = (in.pos.x - 400.0) / 250.0;
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let breathe = 0.5 + 0.5 * sin(u_time * 2.0);
    let c = mix(in.color.rgb, vec3<f32>(breathe, 1.0 - breathe, 0.5 + 0.5 * breathe), 0.6);
    return vec4<f32>(c, in.color.a);
}
"""


def game():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 17)
    title = cjk.render(
        "红框内=正交原位 quad；中央微缩=同帧透视段（同 cam_buf，会话快照隔离）",
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
    # 渐变 quad：pos3color4（两三角 6 顶点，像素坐标——Auto2d 正交解释）
    x0, y0, x1, y1 = 150.0, 330.0, 650.0, 560.0
    verts = [
        x0, y0, 0.0, 1.0, 0.25, 0.2, 1.0,
        x1, y0, 0.0, 0.1, 0.9, 1.0, 1.0,
        x1, y1, 0.0, 0.1, 0.9, 1.0, 0.4,
        x0, y0, 0.0, 1.0, 0.25, 0.2, 1.0,
        x1, y1, 0.0, 0.1, 0.9, 1.0, 0.4,
        x0, y1, 0.0, 1.0, 0.25, 0.2, 0.4,
    ]
    mesh = ac.mesh("pos3color4", verts, label="demo_mesh")
    cam_buf = ac.buffer(64, label="demo_camera")
    time_buf = ac.buffer(16, label="demo_time")
    cam_bind = ac.bind_group(
        [(0, "uniform", cam_buf), (1, "uniform", time_buf)],
        label="demo_bind")
    pipe = ac.pipeline(shader, mesh, [cam_bind], label="demo_pipeline")

    st("mesh 顶点数 6", mesh.vertex_count() == 6)
    st("screen_format 查询可用", len(pygame.render.screen_format()) > 0)

    frame = 0
    asserted = False
    # 透视相机（3D 段）：世界居中后移 900（quad → 屏幕中央微缩）
    M = pygame.math
    cam3d = pygame.render.camera_perspective(0.9, 800.0 / 600.0, 0.1, 2000.0) \
        .with_view(M.mat4_translation(-400.0, -445.0, -900.0))
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        ac.write_buffer(time_buf, [frame / 60.0])

        with screen.render() as t:
            if frame == 0:
                st("screen_attachment 帧内可取",
                   pygame.render.screen_attachment() is not None)
            t.fill((16, 17, 24))
            t.blit(title, (14, 10))
            t.rect(color=(120, 60, 60), rect=(140, 60, 520, 250), width=2)
            t.rect(color=(60, 120, 160), rect=(140, 330, 520, 250), width=2)
            # ── 段 1：Auto2d 正交（上半）——camera=cam_buf 会话托管 ──
            with t.raw_render(camera=cam_buf) as p:
                p.set_pipeline(pipe)
                p.set_bind_group(0, cam_bind)
                p.set_mesh(mesh)
                p.draw(0, 6)
            # ── 段 2：切 3D 透视（下半）——同一 mesh 同一 cam_buf，
            #    会话快照隔离：回放各自写入配对矩阵 ──
            pygame.render.set_camera(cam3d)
            with t.raw_render(camera=cam_buf) as p:
                p.set_pipeline(pipe)
                p.set_bind_group(0, cam_bind)
                p.set_mesh(mesh)
                p.draw(0, 6)
            pygame.render.reset_camera()

        pygame.display.flip()
        frame += 1

        if not asserted and frame == 60:
            st("raw_render 60 帧排程不抛", True)
            st("帧尾相机状态回 auto2d", pygame.render.camera_state() == "auto2d")
            asserted = True
            print("[cp-demo] SUMMARY",
                  "PASS" if not _fail else "FAIL " + str(_fail))
        yield

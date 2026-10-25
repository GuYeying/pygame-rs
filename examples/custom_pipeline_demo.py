# custom_pipeline_demo（渲染架构批次 C/D）：自建 WGSL 管线 + raw_render
# 直绘会话——底层定制化全链路验证。
# ① pygame.render.access() 两入口取资源工厂 → 自建 Shader/Mesh/
#    BindGroup/Pipeline（安全收参：布局预设/枚举字符串）
# ② 全局相机定约：group0/b0 = 64B 列主 mat4 MVP（write_camera_mvp
#    帧内写入——与引擎管线同一约定）+ b1 time uniform 每帧更新
#    （颜色呼吸动画 = 自定义 uniform 全链路生效证明）
# ③ with t.raw_render() as p: 命令记录 → present 收口回放（恒在游戏
#    内容之上）
# 下半红框：自定义管线渐变 quad（红↔青呼吸）。
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
        "下半红框：自定义 WGSL 管线（raw_render + 相机定约 group0/b0=64B MVP + b1 time 呼吸）",
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
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        # 相机定约（Auto2d 全局正交——draw 时点快照）+ time uniform 更新
        pygame.render.write_camera_mvp(cam_buf)
        ac.write_buffer(time_buf, [frame / 60.0])

        with screen.render() as t:
            if frame == 0:
                st("screen_attachment 帧内可取",
                   pygame.render.screen_attachment() is not None)
            t.fill((16, 17, 24))
            t.blit(title, (14, 10))
            t.rect(color=(120, 60, 60), rect=(140, 320, 520, 250), width=2)
            # raw_render：命令记录，__exit__ 排程回放
            p = t.raw_render()
            with p:
                p.set_pipeline(pipe)
                p.set_bind_group(0, cam_bind)
                p.set_mesh(mesh)
                p.draw(0, 6)

        pygame.display.flip()
        frame += 1

        if not asserted and frame == 60:
            st("raw_render 60 帧排程不抛", True)
            asserted = True
            print("[cp-demo] SUMMARY",
                  "PASS" if not _fail else "FAIL " + str(_fail))
        yield

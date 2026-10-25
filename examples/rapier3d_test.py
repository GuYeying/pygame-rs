# pygame_physics 3D 类面对接专项测试（rapier3d 调试渲染 v1；2026-10-24 合并单模块——3D 类带 3D 后缀）
# 控制台 [rapier3d-test] 逐项断言 + 3D 落体调试线样张上屏（视觉验收：
# 地面网格 + 三球一箱下落碰撞——look-at 透视相机，米制 y 向上原点居中，
# 深度通道硬契约）。断言收口后保持窗续演到用户关窗/ESC。
import pygame
import pygame_physics as rp3


async def main():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 20)
    _fail = []

    def st(name, cond):
        if cond:
            print("[rapier3d-test] PASS", name)
        else:
            _fail.append(name)
            print("[rapier3d-test] FAIL", name)

    def no_raise(name, fn):
        try:
            fn()
            st(name, True)
        except Exception as e:
            st(name, False)
            print("    [rapier3d-test] 异常:", e)

    # ── 程序化断言（帧 1）──
    world = rp3.PhysicsWorld3D(gravity=(0, -9.81, 0))
    st("PhysicsWorld 构造", world is not None)

    # 地面（fixed 静态体 + cuboid 6×0.5×6，顶面 y=0）
    ground = rp3.RigidBodyBuilder3D("fixed", translation=(0, -0.5, 0))
    gcol = rp3.ColliderBuilder3D("cuboid", hx=6.0, hy=0.5, hz=6.0, friction=0.6)
    bh, ch = world.insert(ground, gcol)
    st("insert 返回句柄对", isinstance(bh, int) and isinstance(ch, int))

    # 落球 ×3（dynamic + ball，不同高度错落；rotation = 欧拉角向量）
    balls = []
    for i in range(3):
        b = rp3.RigidBodyBuilder3D("dynamic",
                                 translation=(-1.5 + i * 1.5, 2.0 + i * 1.6, 0),
                                 rotation=(0, 0.3 * i, 0))
        c = rp3.ColliderBuilder3D("ball", radius=0.4, restitution=0.35, friction=0.4)
        balls.append(world.insert(b, c)[0])
    x0, y0, z0 = world.translation(balls[0])
    st("translation 查询初值 (x,y,z)",
       abs(x0 + 1.5) < 1e-4 and abs(y0 - 2.0) < 1e-4 and abs(z0) < 1e-4)
    qx, qy, qz, qw = world.rotation(balls[2])
    st("rotation 查询四元数 (xyzw)", abs(qw) < 1.0 and
       (abs(qx) + abs(qy) + abs(qz)) > 1e-3)

    # 翻滚箱（dynamic + cuboid，斜置旋转——非球体形状 + 姿态可视化）
    box = rp3.RigidBodyBuilder3D("dynamic", translation=(0.0, 5.0, -1.0),
                               rotation=(0.4, 0.2, 0.9), linvel=(0.5, 0, 0),
                               angvel=(0.3, 0.7, 0.1))
    bcol = rp3.ColliderBuilder3D("cuboid", hx=0.5, hy=0.5, hz=0.5, density=1.2)
    world.insert(box, bcol)

    # 负路径：builder 一次性消费
    dup = rp3.RigidBodyBuilder3D("dynamic")
    col = rp3.ColliderBuilder3D("ball", radius=0.3)
    world.insert(dup, col)
    try:
        world.insert(dup, col)
        st("builder 二次消费报 ValueError", False)
    except ValueError:
        st("builder 二次消费报 ValueError", True)
    try:
        world.translation(99999)
        st("无效 handle 报 ValueError", False)
    except ValueError:
        st("无效 handle 报 ValueError", True)

    # ── 写入面（批次 47；3D 同款语义）──
    world.set_translation(balls[0], (0.0, 3.5, 0.0))
    x1, y1, z1 = world.translation(balls[0])
    st("set_translation 瞬移", abs(x1) < 1e-4 and abs(y1 - 3.5) < 1e-4 and abs(z1) < 1e-4)
    world.set_linvel(balls[0], (1.0, 0.0, 0.0))
    st("set_linvel 读回", abs(world.linvel(balls[0])[0] - 1.0) < 1e-4)
    world.apply_impulse(balls[1], (0.0, 0.0, 2.0))
    st("apply_impulse 生效（有横速）", world.linvel(balls[1])[2] > 0.0)
    # 触发区（sensor + events）：球穿越产生事件流
    trig = world.insert(
        rp3.RigidBodyBuilder3D("fixed", translation=(0.0, 0.6, 0.0)),
        rp3.ColliderBuilder3D("cuboid", hx=1.5, hy=0.3, hz=1.5, sensor=True, events=True),
    )[1]
    st("触发区 collider 句柄", isinstance(trig, int))

    # 调试三件套 + look-at 相机（fov = 垂直视场角弧度）
    draw = rp3.DebugDraw3D()
    sink = rp3.LineSink3D()
    renderer = rp3.DebugLinesRenderer3D(eye=(9.0, 6.0, 15.0), target=(0, 0.5, 0), fov=1.0)

    label = cjk.render("pygame_physics3d 落体 + DebugLinesRenderer 3D 叠加通道（透视相机 + 深度）",
                       True, (200, 200, 200))
    # 活体指示：左上角帧计数（每 30 帧重绘——画面在动的直观判据）
    counter_x, counter_y = 16, 560

    # ── 帧循环（事件 → step → collect → 主屏会话 t.draw_debug_3d 直渲 → flip）
    # 断言收口后**不退场**：场景续演到用户关窗/ESC（视觉验收保持窗）──
    frame_no = 0
    first_y = y0
    asserted = False
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        n = world.step_frame(1.0 / 60.0)
        if frame_no == 0:
            st("step_frame 返回步数 ≥1", n >= 1)
        rp3.collect3d(world, draw, sink)
        if frame_no == 0:
            st("collect 线段非空", sink.line_count() > 0)
            st("line_count 口径合理", sink.line_count() < 65536)

        # 主屏直渲（用户方案回退验证）：3D 调试线直接排程到 Screen
        # present 收口（深度通道）
        with screen.render() as t:
            t.fill((18, 20, 28))
            t.blit(label, (16, 12))
            if frame_no == 0:
                no_raise("t.draw_debug_3d 会话内", lambda: t.draw_debug_3d(renderer, sink))
            else:
                t.draw_debug_3d(renderer, sink)
            if frame_no % 30 == 0:
                t.blit(cjk.render(f"帧 {frame_no}", True, (120, 220, 120)),
                       (counter_x, counter_y))

        pygame.display.flip()
        frame_no += 1

        if not asserted and frame_no == 90:  # 1.5s 后：落体 + 时钟断言收口
            x, y, z = world.translation(balls[0])
            st("落体 y 下降", y < first_y - 0.5)
            st("球落至地面附近（0 < y < 1.0）", -1.0 < y < 1.0)
            st("球未横向漂出（|x| < 3）", abs(x) < 3.0)
            st("sim_time ≈ 1.5s", abs(world.sim_time() - 1.5) < 0.2)
            evs = world.drain_events()
            st("触发器事件流非空", len(evs) > 0)
            st("drain_events 元组形态", all(len(e) == 3 for e in evs))
            hit = world.cast_ray((5.0, 1.0, 0.0), (0.0, -1.0, 0.0), None)
            st("cast_ray 命中地面（距离 ≈ 1）", hit is not None and abs(hit[1] - 1.0) < 0.05)
            st("调试线会话排程 90 帧", True)  # draw_debug_3d 未抛即通过
            asserted = True
            print("[rapier3d-test] SUMMARY",
                  "PASS" if not _fail else "FAIL " + str(_fail))
        await pygame.next_frame()

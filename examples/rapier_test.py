# pygame.rapier 对接专项测试（渲染对接 v1）
# 控制台 [rapier-test] 逐项断言 + 落体调试线样张上屏（视觉验收：
# 地面横线 + 三球下落碰撞——世界原点居中、y 向上、ppm=60）
# 接口风格 = rapier 原生词汇（不 pygame 化）；绘制 = 会话排程形态
# `t.draw_debug(renderer, sink)`（与 egui 的 t.draw_ui 同一渲染行为）
import pygame
import pygame.rapier as rp


def game():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 20)
    _fail = []

    def st(name, cond):
        if cond:
            print("[rapier-test] PASS", name)
        else:
            _fail.append(name)
            print("[rapier-test] FAIL", name)

    def no_raise(name, fn):
        try:
            fn()
            st(name, True)
        except Exception as e:
            st(name, False)
            print("    [rapier-test] 异常:", e)

    # ── 程序化断言（帧 1）──
    world = rp.PhysicsWorld(gravity=(0.0, -9.81))
    no_raise("PhysicsWorld 构造", lambda: None)

    # 地面（fixed 静态体 + cuboid）
    ground = rp.RigidBodyBuilder("fixed", translation=(0.0, -2.0))
    gcol = rp.ColliderBuilder("cuboid", hx=6.0, hy=0.5, friction=0.6)
    bh, ch = world.insert(ground, gcol)
    st("insert 返回句柄对", isinstance(bh, int) and isinstance(ch, int))

    # 落球 ×3（dynamic + ball，不同高度错落下）
    balls = []
    for i in range(3):
        b = rp.RigidBodyBuilder("dynamic",
                                translation=(-1.2 + i * 1.2, 2.0 + i * 1.5),
                                rotation=0.3 * i)
        c = rp.ColliderBuilder("ball", radius=0.4, restitution=0.35, friction=0.4)
        balls.append(world.insert(b, c)[0])
    x0, y0 = world.translation(balls[0])
    st("translation 查询初值", abs(x0 + 1.2) < 1e-4 and abs(y0 - 2.0) < 1e-4)
    st("rotation 查询初值", abs(world.rotation(balls[2]) - 0.6) < 1e-4)

    # 负路径：builder 一次性消费
    dup = rp.RigidBodyBuilder("dynamic")
    col = rp.ColliderBuilder("ball", radius=0.3)
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

    # 调试三件套 + 相机
    draw = rp.DebugDraw()                       # 缺省 = 形状 + 接触位
    sink = rp.LineSink()
    renderer = rp.DebugLinesRenderer(ppm=60.0)

    label = cjk.render("pygame.rapier 落体 + DebugLinesRenderer 叠加通道（原点居中 y↑ ppm=60）",
                       True, (200, 200, 200))

    # ── 帧循环（事件 → step → collect → 游戏会话 + t.draw_debug 排程 → flip）
    # 断言收口后**不退场**：场景续演到用户关窗/ESC（视觉验收保持窗；
    # 宿主凭 QUIT 或生成器耗尽收尾）──
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
        rp.collect(world, draw, sink)
        if frame_no == 0:
            st("collect 线段非空", sink.line_count() > 0)
            st("line_count 口径合理", sink.line_count() < 65536)

        with screen.render() as t:
            t.fill((18, 20, 28))
            t.blit(label, (16, 12))
            # 会话排程：调试线（与 egui 的 t.draw_ui 同一渲染行为——
            # 会话内调用、flip 时恒在游戏内容之上）
            if frame_no == 0:
                no_raise("t.draw_debug 会话内", lambda: t.draw_debug(renderer, sink))
            else:
                t.draw_debug(renderer, sink)

        pygame.display.flip()
        frame_no += 1

        if not asserted and frame_no == 90:  # 1.5s 后：落体 + 时钟断言收口
            y = world.translation(balls[0])[1]
            st("落体 y 下降", y < first_y - 0.5)
            st("球落至地面附近（y < -0.9）", y < -0.9)
            st("sim_time ≈ 1.5s", abs(world.sim_time() - 1.5) < 0.2)
            st("调试线会话排程 90 帧", True)  # draw_debug 未抛即通过
            asserted = True
            print("[rapier-test] SUMMARY",
                  "PASS" if not _fail else "FAIL " + str(_fail))
        yield

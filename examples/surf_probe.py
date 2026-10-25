# Surface 链路最小复现（用户假设排查：PySurface 缺要素静默不渲染？
# 1px 线被过滤？）——四象限逐项验证，任一象限空白即定位链路断点。
# ① 左上：纯 fill 红块（最基础 blit 路径）
# ② 右上：Surface + 2D 形状（rect/circle/line——draw::* 直绘）
# ③ 左下：depth_buffer=True Surface + fill 蓝块（深度面 blit）
# ④ 右下：Surface + 2D 物理调试线（1px 白线 + Surface 叠加通道）
# 中央：帧计数活体。全部可见 = Surface 链路健康，问题在用法/环境。
import pygame
import pygame_physics as rp


def game():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 20)
    _fail = []

    def st(name, cond):
        if cond:
            print("[surf-probe] PASS", name)
        else:
            _fail.append(name)
            print("[surf-probe] FAIL", name)

    # 落体世界（右下象限的调试线源）
    world = rp.PhysicsWorld(gravity=(0, -9.81))
    world.insert(
        rp.RigidBodyBuilder("fixed", translation=(0.0, -2.0)),
        rp.ColliderBuilder("cuboid", hx=3.0, hy=0.2),
    )
    ball = world.insert(
        rp.RigidBodyBuilder("dynamic", translation=(0.0, 2.0)),
        rp.ColliderBuilder("ball", radius=0.3),
    )[0]
    draw2 = rp.DebugDraw()
    sink2 = rp.LineSink()
    renderer2 = rp.DebugLinesRenderer()
    renderer2.set_camera(60.0)  # 60 px/m

    # 四块测试面（frame 外创建——持久复用）
    s_fill = pygame.Surface((200, 150))                # ① 纯 fill
    s_draw = pygame.Surface((200, 150))                # ② 2D 形状
    s_depth = pygame.Surface((200, 150), depth_buffer=True)  # ③ 深度面
    s_lines = pygame.Surface((200, 150))               # ④ 调试线

    frame_no = 0
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        world.step_frame(1.0 / 60.0)
        rp.collect(world, draw2, sink2)

        # ① 纯 fill：红块（每帧 fill——最基础路径）
        with s_fill.render() as t:
            t.fill((200, 40, 40))
        # ② 2D 形状：绿框 + 黄圆 + 青线
        with s_draw.render() as t:
            t.fill((24, 26, 36))
            t.rect((60, 220, 80), (10, 10, 180, 130), 0)
            t.circle((220, 220, 60), (100, 75), 50, 0)
            t.line((60, 220, 220), (0, 0), (200, 150), 3)
        # ③ 深度面 fill：蓝块（depth_buffer=True 的 Surface 采样）
        with s_depth.render() as t:
            t.fill((50, 90, 220))
        # ④ 2D 物理调试线（1px 白线 + Surface 客人叠加通道）
        with s_lines.render() as t:
            t.fill((10, 12, 18))
            t.draw_debug(renderer2, sink2)

        # 合成：四象限 + 中央标签 + 帧计数
        with screen.render() as t:
            t.fill((16, 16, 22))
            t.blit(s_fill, (16, 40))
            t.blit(s_draw, (584, 40))
            t.blit(s_depth, (16, 434))
            t.blit(s_lines, (584, 434))
            t.blit(cjk.render("① fill 红", True, (220, 220, 220)), (16, 12))
            t.blit(cjk.render("② 2D 形状", True, (220, 220, 220)), (584, 200))
            t.blit(cjk.render("③ 深度面蓝", True, (220, 220, 220)), (16, 594 - 18))
            t.blit(cjk.render("④ 1px 调试线", True, (220, 220, 220)), (584, 594 - 18))
            t.blit(cjk.render(f"帧 {frame_no}", True, (120, 220, 120)), (368, 300))
        pygame.display.flip()
        frame_no += 1

        if frame_no == 90:
            y = world.translation(ball)[1]
            st("① 纯 fill 面构造", s_fill.get_size() == (200, 150))
            st("④ 物理 y 下降（线源活着）", y < 1.5)
            st("90 帧无异常", True)
            print("[surf-probe] SUMMARY", "PASS" if not _fail else "FAIL " + str(_fail))
        yield

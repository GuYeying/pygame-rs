# Surface 链路最小复现（用户假设排查：PySurface 缺要素静默不渲染？
# 1px 线被过滤？3D 调试线离屏不渲染？）——五象限逐项验证，任一象限
# 空白即定位链路断点。
# ① 左上：纯 fill 红块（最基础 blit 路径）
# ② 右上：Surface + 2D 形状（rect/circle/line——draw::* 直绘）
# ③ 左下：depth_buffer=True Surface + fill 蓝块（深度面 blit）
# ④ 中下：Surface + 2D 物理调试线（1px 白线 + Surface 叠加）
# ⑤ 右下：depth Surface + **3D 调试线离屏**（用户 A/B：主屏有线、
#    离屏没线——本象限直接复现该路径）
# 中央：帧计数活体。须 --features physics3d（3D 类在）。
import pygame
import pygame_physics as rp


async def main():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 20)
    _fail = []

    def st(name, cond):
        if cond:
            print("[surf-probe] PASS", name)
        else:
            _fail.append(name)
            print("[surf-probe] FAIL", name)

    # ── 2D 世界（④ 象限线源）──
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

    # ── 3D 世界（⑤ 象限线源；与 pip_test 同构）──
    world3 = rp.PhysicsWorld3D(gravity=(0, -9.81, 0))
    world3.insert(
        rp.RigidBodyBuilder3D("fixed", translation=(0, -0.5, 0)),
        rp.ColliderBuilder3D("cuboid", hx=6.0, hy=0.5, hz=6.0),
    )
    ball3 = world3.insert(
        rp.RigidBodyBuilder3D("dynamic", translation=(0.0, 2.0, 0)),
        rp.ColliderBuilder3D("ball", radius=0.4),
    )[0]
    draw3 = rp.DebugDraw3D()
    sink3 = rp.LineSink3D()
    cam3 = rp.DebugLinesRenderer3D(eye=(6.0, 5.0, 10.0), target=(0, 0.5, 0), fov=1.0)

    # 五块测试面（frame 外创建——持久复用）
    s_fill = pygame.Surface((200, 150))                      # ① 纯 fill
    s_draw = pygame.Surface((200, 150))                      # ② 2D 形状
    s_depth = pygame.Surface((200, 150), depth_buffer=True)  # ③ 深度面
    s_lines = pygame.Surface((200, 150))                     # ④ 2D 调试线
    s_lines3d = pygame.Surface((200, 150), depth_buffer=True)  # ⑤ 3D 调试线离屏

    frame_no = 0
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        world.step_frame(1.0 / 60.0)
        rp.collect(world, draw2, sink2)
        world3.step_frame(1.0 / 60.0)
        rp.collect3d(world3, draw3, sink3)

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
        # ⑤ 3D 调试线离屏（用户 A/B 复现路径：深度面 + draw_debug_3d）
        with s_lines3d.render() as t:
            t.fill((10, 12, 18))
            t.draw_debug_3d(cam3, sink3)

        # 合成：五象限 + 标签 + 帧计数
        with screen.render() as t:
            t.fill((16, 16, 22))
            t.blit(s_fill, (16, 40))          # 左上
            t.blit(s_draw, (584, 40))         # 右上
            t.blit(s_depth, (16, 218))        # 左中
            t.blit(s_lines, (300, 218))       # 中中
            t.blit(s_lines3d, (584, 218))     # 右中
            t.blit(cjk.render("① fill", True, (220, 220, 220)), (16, 12))
            t.blit(cjk.render("② 2D 形状", True, (220, 220, 220)), (584, 12))
            t.blit(cjk.render("③ 深度面蓝", True, (220, 220, 220)), (16, 560))
            t.blit(cjk.render("④ 2D 线", True, (220, 220, 220)), (300, 560))
            t.blit(cjk.render("⑤ 3D 线离屏", True, (220, 220, 220)), (584, 560))
            t.blit(cjk.render(f"帧 {frame_no}", True, (120, 220, 120)), (368, 400))
        pygame.display.flip()
        frame_no += 1

        if frame_no == 90:
            y = world.translation(ball)[1]
            y3 = world3.translation(ball3)[1]
            st("④ 物理 y 下降（2D 线源活着）", y < 1.5)
            st("⑤ 物理 y 下降（3D 线源活着）", y3 < 1.5)
            st("⑤ 3D 线集非空（sink3）", sink3.line_count() > 0)
            st("90 帧无异常", True)
            print("[surf-probe] SUMMARY", "PASS" if not _fail else "FAIL " + str(_fail))
        await pygame.next_frame()

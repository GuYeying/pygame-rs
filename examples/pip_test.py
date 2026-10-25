# 画中画 / 游戏内面板验证（批次 48：Surface 会话客人叠加通道）
# ① 3D 物理调试动画画进离屏 Surface（depth_buffer=True + 深度通道 +
#    look-at 相机）→ blit 上屏 = 画中画视口；
# ② egui 面板画进离屏 Surface（游戏内面板形态）→ blit 上屏；
# ③ 主屏画面与两块画中画同帧合成。[pip-test] 逐项断言 + 保持窗。
import pygame
import pygame_gui as egui
import pygame_physics as rp


class PipPanel(egui.UiBase):
    """画中画内嵌面板：实例属性捕获状态。"""

    def __init__(self):
        self.frames = 0
        self.clicked = 0
        # 游戏侧变量（每帧由游戏循环写入——变量驱动交互定约）
        self.sim_t = 0.0
        self.landed = False

    def draw(self, ui):
        self.frames += 1
        ui.label("内嵌面板 (Surface)")
        # ── 变量驱动：控件显示完全由游戏侧变量决定（传值不回写——
        #    egui 返回值忽略，交互判定在游戏侧）──
        ui.progress_bar(min(1.0, self.sim_t / 3.0))
        ui.label(f"仿真 {self.sim_t:.2f}s")
        ui.checkbox("落体完成", self.landed)
        if ui.button("面板按钮"):
            self.clicked += 1
        ui.label(f"帧 {self.frames}")


async def main():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 20)
    _fail = []

    def st(name, cond):
        if cond:
            print("[pip-test] PASS", name)
        else:
            _fail.append(name)
            print("[pip-test] FAIL", name)

    def no_raise(name, fn):
        try:
            fn()
            st(name, True)
        except Exception as e:
            st(name, False)
            print("    [pip-test] 异常:", e)

    # ── 3D 物理（画中画内容源）──
    world = rp.PhysicsWorld3D(gravity=(0, -9.81, 0))
    world.insert(
        rp.RigidBodyBuilder3D("fixed", translation=(0, -0.5, 0)),
        rp.ColliderBuilder3D("cuboid", hx=6.0, hy=0.5, hz=6.0, friction=0.6),
    )
    balls = []
    for i in range(3):
        balls.append(world.insert(
            rp.RigidBodyBuilder3D("dynamic",
                                  translation=(-1.2 + i * 1.2, 2.0 + i * 1.5, 0)),
            rp.ColliderBuilder3D("ball", radius=0.4, restitution=0.35),
        )[0])
    draw3 = rp.DebugDraw3D()
    sink3 = rp.LineSink3D()
    cam = rp.DebugLinesRenderer3D(eye=(6.0, 5.0, 10.0), target=(0, 0.5, 0), fov=1.0)

    # 画中画视口：**depth_buffer=True**（3D 深度通道硬契约）；加大居中
    # 展示（480×360——用户反馈右下角太小看不清）
    pip = pygame.Surface((480, 360), depth_buffer=True)
    # 游戏内面板：无深度面（2D egui 通道）；右上角
    ui_surf = pygame.Surface((260, 140))
    panel = PipPanel()

    egui.init()

    label = cjk.render("画中画：中间 = 3D 物理调试（Surface 深度通道）· 右上 = egui 内嵌面板",
                       True, (200, 200, 200))

    # ── 帧循环（step → collect → 主屏画面 → 两块画中画 → blit → flip）──
    frame_no = 0
    asserted = False
    while True:
        for e in pygame.event.get():
            egui.event(e)                # 显式逐条喂（不喂 = egui 无输入）
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        world.step_frame(1.0 / 60.0)
        rp.collect3d(world, draw3, sink3)
        # 游戏侧变量 → 面板（变量驱动交互：游戏循环写状态，draw 读显示）
        panel.sim_t = world.sim_time()
        panel.landed = world.translation(balls[0])[1] < 0.5

        # 画中画 ①：3D 物理调试动画（Surface 会话客人叠加——__exit__ 时
        # flush 把游戏 batch 与叠加通道一并烘进本面纹理）
        with pip.render() as t:
            t.fill((10, 12, 18))
            if frame_no == 0:
                no_raise("pip 3D draw_debug_3d", lambda: t.draw_debug_3d(cam, sink3))
            else:
                t.draw_debug_3d(cam, sink3)

        # 画中画 ②：egui 内嵌面板（Surface 会话客人叠加；纯展示形态——
        # egui 指针交互归主屏 Screen，见 CLAUDE.md 批次 48 定约）
        with ui_surf.render() as t:
            t.fill((30, 32, 44))
            if frame_no == 0:
                no_raise("pip egui draw_ui", lambda: t.draw_ui(panel))
            else:
                t.draw_ui(panel)

        # 主屏合成：两块画中画 blit 上屏（pip 的叠加通道已在各自 flush
        # 收口时烘进纹理——blit 采样即得合成结果；会话内 blit = 主屏
        # batch 的既有贴图路径）
        with screen.render() as t:
            t.fill((18, 20, 28))
            t.blit(label, (16, 12))
            t.blit(pip, (160, 120))
            t.blit(ui_surf, (524, 40))
        pygame.display.flip()
        frame_no += 1

        if not asserted and frame_no == 90:
            y = world.translation(balls[0])[1]
            st("画中画物理在走（落体 y 下降）", y < 2.0 - 0.5)
            st("内嵌面板帧计数推进", panel.frames >= 90)
            st("游戏变量驱动面板显示（sim_t 流入）", panel.sim_t > 1.4)
            st("游戏变量驱动复选（落体完成判定）", panel.landed is True)
            st("样张合成 90 帧", True)  # 全链未抛即通过
            asserted = True
            print("[pip-test] SUMMARY",
                  "PASS" if not _fail else "FAIL " + str(_fail))
        await pygame.next_frame()

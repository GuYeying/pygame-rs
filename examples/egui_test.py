# pygame_gui 对接专项测试（原 pygame.egui，2026-10-24 升格）（渲染对接 v1，2026-10-24 会话形态终版）
# 控制台 [egui-test] 逐项断言 + HUD 样张上屏（视觉验收：egui 面板画在
# 游戏画面之上、中文正常渲染）
# 接口形态（本批定约）：
#   ① OOP 门面 + 会话目标——`t.draw_ui(panel)`（t = with screen.render()
#      的会话对象）：panel 强制继承 egui.UiBase 且实现 draw(self, ui)；
#   ② 事件逐条显式喂——`egui.event(e)`（pygame.event.get() 循环内调用；
#      不喂则 egui 无输入——捕获行为在数据流层完全可见）；
#   ③ 嵌套布局 with 语法——`with ui.horizontal():`；
#   ④ 落屏时机 = flip 的叠加阶段（恒在游戏内容之上，结构保证）。
import pygame
import pygame_gui as egui


class HUD(egui.UiBase):
    """HUD 面板：实例属性捕获状态与游戏对象（OOP 门面用例）。"""

    def __init__(self, game):
        self.game = game                 # 捕获游戏对象（属性捕获用例）
        self.frames = 0
        self.clicks = 0
        self.checked = False
        self.gravity = 9.81
        self.last_slider = None
        self.name = "玩家"
        self.mode = 1                    # radio/combo 共享选中序号
        self.speed = 1.0
        self.sel = False

    def draw(self, ui):
        self.frames += 1
        ui.heading("egui-starfish-we HUD 测试")
        ui.label("pygame_gui OOP 门面 · 中文渲染验收")
        if ui.button("点我 (button)"):
            self.clicks += 1
        self.checked = ui.checkbox("复选框 (checkbox)", self.checked)
        g = ui.slider_f32("重力 (slider)", self.gravity, 0.0, 20.0)
        self.gravity = g
        self.last_slider = g
        ui.separator()
        # ── 词汇扩充 v2（批次 47）──
        self.name = ui.text_edit(self.name)
        self.mode = ui.radio_group(("Easy", "Normal", "Hard"), self.mode)
        self.sel = ui.selectable("可选行 (selectable)", self.sel)
        ui.progress_bar(min(1.0, self.frames / 600))
        self.speed = ui.drag_f32("速度", self.speed, 0.05, 0.0, 5.0)
        self.mode = ui.combo_box("难度", ("Easy", "Normal", "Hard"), self.mode)
        ui.separator()
        with ui.horizontal():
            ui.label(f"帧 {self.frames}")
            ui.label(f"点击 {self.clicks}")
            ui.label(f"g={g:.2f}")
            ui.label(f"速度={self.speed:.2f}")
        ui.horizontal(lambda inner: inner.label("回调形态同行"))
        ui.separator()
        # 经属性捕获的游戏对象参与 UI
        ui.label(f"游戏画面 {self.game['size'][0]}x{self.game['size'][1]}")


async def main():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 20)
    game_state = {"size": [800, 600]}
    _fail = []

    def st(name, cond):
        if cond:
            print("[egui-test] PASS", name)
        else:
            _fail.append(name)
            print("[egui-test] FAIL", name)

    def no_raise(name, fn):
        try:
            fn()
            st(name, True)
        except Exception as e:
            st(name, False)
            print("    [egui-test] 异常:", e)

    # ── init（幂等；无隐式事件挂接——输入走 event(e) 逐条喂）──
    no_raise("egui.init", lambda: egui.init())
    no_raise("egui.init 幂等", lambda: egui.init())
    st("wants_pointer 初值布尔", isinstance(egui.wants_pointer(), bool))

    # ── OOP 负路径（draw_ui 的会话形态——须在 with 块内调用）──
    # with screen.render() as t:
    #     t.fill((18, 20, 28))
    #     try:
    #         t.draw_ui(object())
    #         st("非 UiBase 实例 TypeError", False)
    #     except TypeError:
    #         st("非 UiBase 实例 TypeError", True)

    #     class Empty(egui.UiBase):
    #         pass

    #     try:
    #         t.draw_ui(Empty())
    #         st("未实现 draw NotImplementedError", False)
    #     except NotImplementedError:
    #         st("未实现 draw NotImplementedError", True)

    # # 会话块外调用（探针先行——句柄持有无碍，槽已清）
    # hud_probe = HUD(game_state)
    # try:
    #     with screen.render() as t:
    #         t.fill((0, 0, 0))
    #     t.draw_ui(hud_probe)
    #     st("会话块外调用 TypeError", False)
    # except TypeError:
    #     st("会话块外调用 TypeError", True)

    # ── 面板实例（属性捕获游戏状态）──
    hud = HUD(game_state)

    label = cjk.render("pygame_gui 叠加通道：egui 面板应在游戏画面之上（左上角）",
                       True, (200, 200, 200))

    # ── 帧循环（事件逐条喂 egui → t.draw_ui 会话内构建+排程 → flip 落屏）──
    frame_no = 0
    while True:
        for e in pygame.event.get():
            egui.event(e)                # 显式逐条喂（不喂 = egui 无输入）
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        with screen.render() as t:
            t.fill((18, 20, 28))
            t.blit(label, (16, 12))
            if frame_no == 0:
                no_raise("t.draw_ui 首帧", lambda: t.draw_ui(hud))
            else:
                t.draw_ui(hud)

        pygame.display.flip()
        frame_no += 1

        # if frame_no == 60:
        #     st("draw 每帧调用（60 次）", hud.frames == 60)
        #     st("slider 返回值在值域内", hud.last_slider is not None
        #        and 0.0 <= hud.last_slider <= 20.0)
        #     st("checkbox 返回布尔", isinstance(hud.checked, bool))
        #     st("属性捕获游戏对象生效", hud.game["size"] == [800, 600])
        #     st("draw_ui 持续 60 帧", True)  # 未抛即通过
        #     break
        await pygame.next_frame()

    # ── shutdown（幂等）──
    no_raise("egui.shutdown", lambda: egui.shutdown())
    no_raise("shutdown 后 wants_pointer 可查", lambda: egui.wants_pointer())
    print("[egui-test] SUMMARY", "PASS" if not _fail else "FAIL " + str(_fail))
    await pygame.next_frame()

# 绑定收官 parity 验证（批次 49 新接口：颜色表/色彩空间/mixer Channel+Effect/深度会话）
# [parity-test] 逐项断言 + 视觉样张（色板网格 + 深度会话矩形 + mixer 音频）
# 保持窗形态（关窗/ESC 收尾）。Web 提示：音频需首次手势解锁后生效。
import pygame


async def main():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 20)
    _fail = []

    def st(name, cond):
        if cond:
            print("[parity-test] PASS", name)
        else:
            _fail.append(name)
            print("[parity-test] FAIL", name)

    def no_raise(name, fn):
        try:
            fn()
            st(name, True)
        except Exception as e:
            st(name, False)
            print("    [parity-test] 异常:", e)

    # ── 颜色 parity（批次 49：颜色表装载 + 色彩空间方法面）──
    red = pygame.Color("red")
    st("color.named", red.r == 255 and red.g == 0 and red.b == 0)
    red.update(40, 50, 60, 200)
    st("color.update", red.r == 40 and red.a == 200)
    st("color.cmy 域", all(0.0 <= v <= 1.0 for v in red.cmy))
    st("color.hsva 域", 0.0 <= red.hsva[0] <= 360.0 and len(red.hsva) == 4)
    st("color.hsla 域", len(red.hsla) == 4)
    st("color.i1i2i3 域", len(red.i1i2i3) == 3)
    st("color.contains", red.contains(40))
    red.set_length(4)
    st("color.set_length", red.r == 40)
    names = ("red", "green", "blue", "gold", "purple", "orange", "aqua", "pink")
    swatches = [pygame.Color(n) for n in names]

    # ── mixer parity（Channel 定点 + Effect 特效 + beep）──
    pygame.mixer.init(None)
    ch = pygame.mixer.Channel(0)
    st("mixer.Channel 构造", ch is not None)
    st("mixer.num_channels", pygame.mixer.num_channels() >= 1)
    st("mixer.output_sample_rate", pygame.mixer.output_sample_rate() > 0)
    pygame.mixer.set_reserved(1)
    fx = pygame.mixer.Effect("echo")
    no_raise("mixer.Effect.set", lambda: fx.set("delay_ms", 120.0))
    ch.add_effect(fx)
    st("mixer.Channel.add_effect", True)
    beep = pygame.mixer.beep()
    ch.play(beep, None)
    st("mixer.Channel.play/get_busy", ch.get_busy())
    no_raise("mixer.Channel.fade_out", lambda: ch.fade_out(400))

    label = cjk.render("绑定收官 parity：颜色表/色彩空间（色板）· mixer Channel+Effect（音频）· 深度会话（中）",
                       True, (200, 200, 200))

    frame_no = 0
    asserted = False
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        # 主屏会话：底色 + 色板网格（命名颜色实例逐格填充）
        with screen.render() as t:
            t.fill((18, 20, 28))
            t.blit(label, (16, 12))
            for i, c in enumerate(swatches):
                x, y = 24 + (i % 8) * 92, 60 + (i // 8) * 64
                t.rect((c.r, c.g, c.b), (x, y, 80, 44), None)
                t.blit(cjk.render(names[i], False, (220, 220, 220)), (x, y + 46))

        # 深度会话：API 链路验证（独立槽位——绘制按深度管线打包）
        with screen.render_depth() as t:
            t.fill((30, 30, 44))
            t.rect((0, 200, 90), (300, 200, 200, 140))
            t.rect((200, 90, 0), (380, 260, 200, 140))

        # 普通会话回填说明（画家序叠加演示：三段会话同帧合成）
        with screen.render() as t:
            t.blit(cjk.render("深度会话样张 ↑（两次 fill+rect 按深度管线）",
                              False, (160, 220, 160)), (300, 180))

        pygame.display.flip()
        frame_no += 1

        if not asserted and frame_no == 60:
            st("样张合成 60 帧", True)
            asserted = True
            print("[parity-test] SUMMARY",
                  "PASS" if not _fail else "FAIL " + str(_fail))
        await pygame.next_frame()

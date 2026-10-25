# pygame 透明窗口专项验证（桌宠形态基座；逐模块独立案例）
#
# 宿主以 TRANSPARENT 创建旗标建窗（透明合成 + 清屏 alpha 0，无铺底）。
# 验收标准（**肉眼**，本文件控制台只做程序化断言）：
#   1. 窗口无底色矩形——桌面直接透出（SDL colorkey 方案不会有此效果）
#   2. 黄色大字 / 圆形边缘平滑淡出——无 fill 色描边（colorkey 指纹）
#   3. 半透明白色面板与桌面混合可见
#   4. 窗口整体透明度呼吸（set_opacity 运行时生效）
# 平台边界：透明合成 = 桌面 DWM/合成器语义；Web/移动端窗口透明支持
# 有限（Canvas/合成器差异），本案例以桌面为验收基准。
import pygame


async def main():
    # 透明 = DIRECTX 后端专属（Vulkan Win32 表面只报 Opaque——wgpu-30
    # d3d12 的 DxgiFromVisual/DirectComposition 才支持逐像素透明）
    screen = pygame.display.set_mode((800, 600), pygame.display.TRANSPARENT | pygame.display.DIRECTX)
    _fail = []

    def st(name, cond):
        if cond:
            print("[transparent-test] PASS", name)
        else:
            _fail.append(name)
            print("[transparent-test] FAIL", name)

    font = pygame.font.Font(None, 32)
    big = pygame.font.Font(None, 64)

    # ── 程序化断言（表面级 alpha / 可见态 / 不透明度调用面）──
    _p = pygame.Surface((16, 16))
    with _p.render() as r:
        r.fill((255, 0, 0))
    _p.set_alpha(120)
    st("Surface set_alpha/get_alpha 回读", _p.get_alpha() == 120)
    st("mouse.get_visible 初始 True", pygame.mouse.get_visible() == True)
    try:
        pygame.display.set_opacity(1.0)
        st("display.set_opacity(1.0) 调用面", True)
    except Exception as e:
        st("display.set_opacity(1.0) 调用面", False)
        print("    [transparent-test] 异常:", e)

    # ── 桌宠本体（预渲染一次——ADR-5：循环内只 blit）──
    body = pygame.Surface((280, 240))
    with body.render() as br:
        br.fill((255, 180, 80, 255))                     # 橙色身体（不透明主体）
        br.circle((40, 30, 30), (90, 90), 18)            # 眼
        br.circle((40, 30, 30), (190, 90), 18)
    body.set_alpha(255)                                  # 主体不透明（桌宠形态：背景全透、本体实心）

    # AA 文字边缘探针：黄色大字（若出现 fill 色描边 = colorkey 类缺陷）
    title = big.render("TRANSPARENT 桌宠基座", True, (255, 230, 120))
    hint = font.render("边缘应平滑淡出桌面（Qt 级合成）；ESC 退出", True, (220, 220, 220))

    print("[transparent-test] 汇总:", "全部通过" if not _fail
          else "失败项 " + repr(_fail) + "（透明质量本身以肉眼验收为准）")

    clock_anchor = pygame.time.get_ticks()
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        # 呼吸/脉冲用三角波（RP 冻结 stdlib 无 math——sin 换三角波近似；
        # math 薄 shim 为待办）
        _t = pygame.time.get_ticks() - clock_anchor

        def tri(period, t):
            """三角波 0→1→0（周期 period 毫秒）"""
            p = (t % period) / period
            return 1 - abs(p * 2 - 1)

        # 窗口整体透明度呼吸（set_opacity 运行时——桌面合成器实时响应）
        pygame.display.set_opacity(0.75 + 0.25 * tri(1200, _t))

        # 桌宠形态：窗口无底色（全透），只有画出的本体可见——
        # 半透明面板测试改小面积局部（不再全屏罩白）
        panel_a = int(90 + 60 * tri(1800, _t))

        with screen.render() as r:
            r.blit(body, (60, 120))                        # 桌宠本体（不透明）
            r.blit(title, (60, 40))                        # AA 文字边缘探针
            r.blit(hint, (60, 100))
            r.rect((255, 255, 255, max(0, panel_a)), (460, 60, 200, 120))  # 局部半透明面板
            r.circle((120, 255, 160), (420, 180),
                     max(6, int(30 * tri(1400, _t))))
        pygame.display.flip()
        await pygame.next_frame()

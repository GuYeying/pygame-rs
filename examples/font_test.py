# pygame.font 模块专项测试（逐模块独立案例 1/N）
# 控制台 [font-test] PASS/FAIL 逐项断言 + 字体样张上屏（视觉验收）
# 退出：ESC / 关窗。三平台：CJK 走各自预载链（msyh / fetch / 系统候选）
import pygame


async def main():
    # flags 落地验证（批次 O 扩展）：尺寸跟随 + 可调整大小
    screen = pygame.display.set_mode((1000, 700), pygame.display.RESIZABLE)
    _fail = []

    def st(name, cond):
        if cond:
            print("[font-test] PASS", name)
        else:
            _fail.append(name)
            print("[font-test] FAIL", name)

    # ── 构造族 ──
    f24 = pygame.font.Font(None, 24)
    st("Font(None, 24) 构造", type(f24).__name__ == "Font")
    st("Font.size 正宽", f24.size("abc")[0] > 0)
    st("Font.size 高 = line_height", f24.size("abc")[1] == f24.line_height)
    st("Font.size 空串 (0, line_height)", f24.size("") == (0, f24.line_height))
    try:
        pygame.font.Font(None)
        st("Font 缺 size TypeError", False)
    except TypeError:
        st("Font 缺 size TypeError", True)
    try:
        pygame.font.Font(b"not-a-font", 24)  # 坏字节（须带 size——构造双参定约）
        st("Font 坏字节 ValueError", False)
    except ValueError:
        st("Font 坏字节 ValueError", True)
    sf = pygame.font.SysFont("arial", 24)
    st("SysFont shim（路由内嵌默认字体）", type(sf).__name__ == "Font")

    # ── CJK（三平台预载链：msyh / fetch cjk.ttf / 系统候选）──
    try:
        cjk = pygame.font.Font("cjk", 28)
        st('Font("cjk") 构造', True)
    except (ValueError, TypeError) as e:
        cjk = f24
        st('Font("cjk") 构造', False)
        print("    [font-test] CJK 异常:", e)

    # ── 路径字体（开发者自填路径语义；候选链按平台命中——Windows 命中
    # arial / Android 命中 Noto / Web·无此文件的设备全 miss → SKIP 非回归）──
    _path_font = None
    for _p in ("C:/Windows/Fonts/arial.ttf", "/system/fonts/NotoSansCJK-Regular.ttc"):
        try:
            _path_font = pygame.font.Font(_p, 24)
            st("Font(路径) " + _p, True)
            break
        except (ValueError, TypeError):
            continue
    if _path_font is None:
        print("[font-test] SKIP Font(路径)（平台无候选路径字体——非回归）")

    # ── render → Surface 一等化 ──
    ts = f24.render("render -> Surface (Antonio 24)", True, (255, 255, 255))
    st("render -> Surface", type(ts).__name__ == "Surface" and ts.get_size()[0] > 0)
    tc = cjk.render("中文渲染测试 pygame-rs", True, (255, 255, 0))
    st("CJK render 非空", tc.get_size()[0] > 0)

    # ── 文字旋转 / 缩放（transform 全族吃一等化 Surface）──
    trot = pygame.transform.rotate(tc, 90)
    st("文字 rotate 90°（宽高互换）",
       trot.get_size() == (tc.get_size()[1], tc.get_size()[0]))
    tsc = pygame.transform.smoothscale(ts, (ts.get_size()[0] * 2, ts.get_size()[1] * 2))
    st("文字 smoothscale ×2", tsc.get_size() == (ts.get_size()[0] * 2, ts.get_size()[1] * 2))
    tz = pygame.transform.rotozoom(tc, 15, 1.5)
    st("文字 rotozoom 复合", tz.get_size()[0] > 0)
    ts.set_alpha(160)
    st("文字 set_alpha", ts.get_alpha() == 160)

    # ── 字体样张（静态表面一次创建——ADR-5 红线：循环内只 blit）──
    spec = [
        f24.render("Font(None, 24): The quick brown fox 0123456789", True, (240, 240, 240)),
        pygame.font.Font(None, 40).render("Font(None, 40): Larger Specimen Row", True,
                                          (120, 200, 255)),
        cjk.render("CJK 28：中文渲染测试——pygame-rs 三平台字体链", True, (255, 255, 0)),
        sf.render("SysFont(arial, 24): shim -> 内嵌默认字体", True, (160, 255, 160)),
    ]
    if _path_font is not None:
        spec.append(_path_font.render("Font(path): system font by path", True, (255, 160, 255)))
    # 旋转 / 缩放 / 半透明的文字表面（预渲染一次——循环内只 blit）。
    # ⚠ transform 的源表面必须保活（_keep）：GL 平台（安卓）临时纹理
    # 销毁后 ID 立即复用，已提交未执行的绘制会采到"新纹理"串内容
    # （ADR-5 红线同族；桌面 wgpu 有生命周期托管不复现）
    _keep = []
    _rot_src = cjk.render("旋转 30° 的中文文字", True, (255, 150, 150))
    _keep.append(_rot_src)
    spec.append(pygame.transform.rotate(_rot_src, 30))
    _sc_src = f24.render("SMOOTHSCALE x2", True, (150, 220, 255))
    _keep.append(_sc_src)
    spec.append(pygame.transform.smoothscale(
        _sc_src, (f24.size("SMOOTHSCALE x2")[0] * 2, f24.size("SMOOTHSCALE x2")[1] * 2)))
    _alpha = f24.render("set_alpha(128) 半透明文字", True, (255, 255, 255))
    _alpha.set_alpha(128)
    spec.append(_alpha)

    print("[font-test] 汇总:", "全部通过" if not _fail
          else "失败项 " + repr(_fail))

    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        
        with screen.render() as r:
            r.fill((24, 24, 34))
            y = 12
            for surf in spec:
                r.blit(surf, (12, y))
                y += 48
            # 底部状态行：CJK 是否可用（颜色即状态）
            r.blit(cjk.render("状态：CJK 可用 ✓" if len(_fail) == 0
                              else "状态：见控制台失败项", True,
                              (120, 255, 120) if not _fail else (255, 120, 120)),
                   (12, y + 8))
        pygame.display.flip()
        await pygame.next_frame()

# pygame.draw 模块专项测试（逐模块独立案例 2/N）
# 控制台 [draw-test] 逐项断言 + 全形状样张上屏（视觉验收）
# 覆盖：六形状（rect/circle/line/polygon/ellipse/arc）× 填充/描边 ×
#       颜色形态（3元/4元/kwargs）× 半透明叠加 × gfxdraw 兼容混绘
# 退出：ESC / 关窗
import pygame


async def main():
    screen = pygame.display.set_mode((800, 600))
    font = pygame.font.Font(None, 20)
    cjk = pygame.font.Font("cjk", 20)
    _fail = []

    def st(name, cond):
        if cond:
            print("[draw-test] PASS", name)
        else:
            _fail.append(name)
            print("[draw-test] FAIL", name)

    def no_raise(name, fn):
        try:
            fn()
            st(name, True)
        except Exception as e:
            st(name, False)
            print("    [draw-test] 异常:", e)

    # ── 程序化断言（帧 1；离屏 Surface 会话）──
    off = pygame.Surface((200, 120))

    def shapes_on(s):
        with s.render() as t:
            t.rect((0, 200, 90), (5, 5, 60, 40))                                # 实心
            t.rect((255, 255, 255), (5, 5, 60, 40), 3)                          # 描边
            t.circle((255, 200, 0), (150, 30), 22)                              # 填充圆
            t.circle((255, 255, 255), (150, 30), 22, 2)                         # 描边圆
            t.line((200, 60, 60), (5, 80), (190, 80), 2)                        # 线
            t.polygon((90, 120, 220), [(30, 95), (90, 100), (70, 115)])         # 填充三角
            t.polygon((255, 160, 60), [(120, 95), (180, 100), (160, 115)], 2)   # 描边
            t.ellipse((60, 180, 120), (5, 5, 80, 40))                           # 椭圆
            t.arc((120, 220, 255), (100, 5, 80, 40), 0.0, 3.14159, 2)           # 弧（弧度制）

    no_raise("六形状全绘（离屏会话）", lambda: shapes_on(off))

    # 半透明叠加（4 元色 alpha 分量）
    def translucent(s):
        with s.render() as t:
            t.rect((255, 0, 0, 90), (5, 5, 40, 40))
            t.rect((0, 0, 255, 90), (25, 25, 40, 40))   # 叠加区 = 混合色
    no_raise("半透明叠加", lambda: translucent(off))

    # gfxdraw 兼容签名面（与 r.* 同一 batch 零额外成本）
    def gfx(s):
        with s.render() as t:
            pygame.gfxdraw.circle(t, 40, 30, 18, (120, 255, 120))
            pygame.gfxdraw.filled_circle(t, 40, 30, 8, (255, 255, 120))
            pygame.gfxdraw.rectangle(t, (70, 12, 50, 36), (255, 120, 255))
    no_raise("gfxdraw 兼容面", lambda: gfx(off))

    # vertices/geometry 逃逸舱（原始顶点 + 索引几何）
    def raw_verts(s):
        with s.render() as t:
            t.vertices("triangles", [(10, 10), (90, 10), (50, 90)], (120, 220, 255))
            t.vertices("lines", [(10, 100), (90, 100)], (255, 160, 60))
    no_raise("vertices 三角/线", lambda: raw_verts(off))

    def raw_geom(s):
        with s.render() as t:
            t.geometry("filled", [(10, 10), (90, 10), (90, 90), (10, 90)],
                       [0, 1, 2, 0, 2, 3], (120, 255, 160))
            t.geometry("line", [(10, 10), (90, 10), (90, 90), (10, 90)],
                       None, (255, 120, 255))
    no_raise("geometry 索引填充/线框", lambda: raw_geom(off))

    # 负路径：颜色分量不足 / 非法项
    def bad_color(s):
        with s.render() as t:
            t.rect((0, 200), (0, 0, 1, 1))
    try:
        bad_color(off)
        st("颜色分量不足 TypeError", False)
    except TypeError:
        st("颜色分量不足 TypeError", True)
    try:
        off2 = pygame.Surface((30, 30))
        with off2.render() as t:
            t.circle((255, 0, 0), (15, 15), -5)   # 负半径钳制（不抛）
        st("circle 负半径钳制", True)
    except Exception:
        st("circle 负半径钳制", False)

    # ── 样张（每帧实绘——形状即测试本体）──
    label = cjk.render("pygame.draw 六形状 + 半透明 + gfxdraw 兼容", True, (200, 200, 200))
    spec = [
        font.render("filled rect / outline w=3 / kwargs", True, (180, 180, 180)),
        font.render("circle fill / outline w=2 / line", True, (180, 180, 180)),
        font.render("polygon fill+outline / ellipse / arc(rad)", True, (180, 180, 180)),
        font.render("translucent overlap / gfxdraw compat", True, (180, 180, 180)),
    ]

    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        
        with screen.render() as r:
            r.fill((24, 24, 34))
            r.blit(label, (12, 8))
            # 第一区：矩形三形态
            r.rect((0, 200, 90), (20, 40, 120, 80))
            r.rect((255, 255, 255), (20, 40, 120, 80), 3)
            r.rect(color=(90, 200, 255), rect=(160, 40, 120, 80), width=2)
            r.blit(spec[0], (20, 128))
            # 第二区：圆 + 线
            r.circle((255, 200, 0), (60, 220), 36)
            r.circle((255, 255, 255), (150, 220), 36, 2)
            r.line((200, 60, 60), (20, 270), (250, 270), 2)
            r.blit(spec[1], (20, 268))
            # 第三区：多边形 + 椭圆 + 弧
            r.polygon((90, 120, 220), [(280, 220), (380, 240), (350, 310), (280, 300)])
            r.polygon((255, 160, 60), [(20, 330), (120, 340), (90, 410)], 2)
            r.ellipse((60, 180, 120), (160, 330, 180, 90))
            r.arc((120, 220, 255), (360, 330, 160, 120), 3.14159, 6.28318, 2)
            r.blit(spec[2], (20, 428))
            # 第四区：半透明叠加 + gfxdraw 兼容混绘
            r.rect((255, 0, 0, 90), (420, 60, 90, 90))
            r.circle((0, 0, 255, 90), (480, 120), 45)
            pygame.gfxdraw.circle(r, 620, 120, 40, (120, 255, 120))
            pygame.gfxdraw.filled_circle(r, 620, 120, 18, (255, 255, 120))
            pygame.gfxdraw.rectangle(r, (580, 200, 140, 80), (255, 120, 255))
            r.blit(spec[3], (420, 290))
            # 离屏 Surface 会话产物 blit（独立绘制目标）
            r.blit(off, (20, 460))
            # 第五区：vertices/geometry 逃逸舱（批次三十八——原始顶点 +
            # 索引几何直通；主体三角扇 + 线框）
            r.vertices("triangles",
                       [(560, 460), (760, 460), (760, 560), (560, 560), (660, 620)],
                       (90, 120, 220))
            r.vertices("lines", [(560, 460), (760, 460), (760, 560), (560, 560)],
                       (255, 255, 255))
            r.geometry("filled", [(580, 480), (740, 480), (740, 540), (580, 540)],
                       [0, 1, 2, 0, 2, 3], (255, 160, 60))
        pygame.display.flip()
        await pygame.next_frame()

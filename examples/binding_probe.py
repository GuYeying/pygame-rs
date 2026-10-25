# M1 绑定面探针（架构 §九 单形态定约）：
# with screen.render() as r: → r.rect/circle/line/polygon/blit（会话显式绘制）。
# 生成器门：yield = 帧边界（Web 端 rAF 节流；原生端 vsync）。
#
# ── 性能定约（批次 F/G 真机实测）──
# 静态 GPU 表面（文字/贴图）一律【创建一次、循环内只 blit】：
# 每帧新建/丢弃 GPU 表面会与 ADR-5 延迟提交冲突（纹理先于命令执行销毁）
# → 通道拆分/内容丢失 → 文字与几何交替闪烁。动态文本（分数等）同理：
# 文本内容变化时才重新 render，不变帧直接 blit 缓存表面。
import pygame


print(dir(pygame))
print(dir(pygame.mask))
print(dir(pygame.event))
async def main():
    screen = pygame.display.set_mode((800, 600))
    font = pygame.font.Font(None, 40)   # 内嵌默认字体（批次 D：拉丁）
    try:
        cjk = pygame.font.Font("cjk", 40)   # 宿主预载的 CJK 字体（批次 G）
        has_cjk = True
    except (ValueError, TypeError):
        cjk = font
        has_cjk = False                      # 回退：拉丁（中文缺字静默跳过）
    try:
        ctry = pygame.font.Font("canterbury", 32)   # 任意字体文件（风格由文件决定）
        has_ctry = True
    except (ValueError, TypeError):
        ctry = font
        has_ctry = False
    # ── 音频：三平台同代码（2026-10-18 定约：Web 自动播放策略 = 部署
    # 约定——build_web.py 产物带"点击启动"覆盖层，AudioContext 创建
    # 必然落在首次手势调用栈内；无运行时手势检测）──
    snd = None
    try:
        snd = pygame.mixer.Sound()
        snd.play(-1)                     # 无限循环：播完自动回卷（引擎音频
                                         #  回调内完成，无缝衔接）
        print("[diag] 提示音已循环播放 len =", snd.get_length())
    except (ValueError, TypeError) as e:
        print("[diag] 音频初始化失败（继续无音频运行）:", e)

    # ── 静态表面：全部创建一次 ──
    tile = pygame.Surface((40, 40))
    with tile.render() as t:            # Surface 会话：离屏绘制同 Screen 语法
        t.fill((200, 120, 40))
    rot = pygame.transform.rotate(tile, 30)
    sm = pygame.transform.smoothscale(tile, (80, 80))     # 批次 L：平滑缩放
    sm.set_alpha(128)                                     # 表面级半透明
    tsurf = font.render("Surface!", True, (255, 255, 255))
    line1 = cjk.render("pygame-rs 中文渲染" if has_cjk else "Hello pygame-rs!",
                       True, (255, 255, 0))
    line2 = ctry.render("Canterbury 中文渲染", True, (120, 200, 255)) if has_ctry else None
    # ── 诊断打印（stdout 取证；None 守卫——Web 上可选字体回退后为 None）──
    print("[diag] gamepad connected =", pygame.gamepad.get_count())
    print("[diag] cjk line_height =", cjk.line_height)
    print("[diag] cjk size('中') =", cjk.size("中"))
    print("[diag] cjk size('abc') =", cjk.size("abc"))
    if has_ctry:
        print("[diag] ctry line_height =", ctry.line_height)
        print("[diag] ctry size('Canterbury') =", ctry.size("Canterbury"))
    if line1 is not None:
        print("[diag] line1 size =", line1.get_size())
    if line2 is not None:
        print("[diag] line2 size =", line2.get_size())
    print("[diag] tsurf size =", tsurf.get_size())

    # ── 批次 I–L 运行时自测（打包验收：帧 1 执行一次，[selftest] 行
    #  三端同源——桌面终端 / Web F12 / Android logcat；严格空表断言
    #  依赖"帧 1 尚无输入事件"的时序）──
    _st_fail = []

    def _st(name, cond):
        if cond:
            print("[selftest] PASS", name)
        else:
            _st_fail.append(name)
            print("[selftest] FAIL", name)

    # touch（批次 I）：查询面 + 帧 1 空表语义
    _st("touch.get_count", pygame.touch.get_count() == 0)
    _st("touch.get_fingers", pygame.touch.get_fingers() == [])
    _st("touch.get_pos", pygame.touch.get_pos(7) is None)
    # gamepad（批次 J）：常量取值 + 无硬件安全语义
    _st("gamepad.const",
        pygame.gamepad.BTN_SOUTH == 0 and pygame.gamepad.BTN_DPAD_RIGHT == 15
        and pygame.gamepad.AXIS_LX == 0 and pygame.gamepad.AXIS_RT == 5)
    _st("gamepad.query", isinstance(pygame.gamepad.get_count(), int)
        and pygame.gamepad.is_connected(99) == False
        and pygame.gamepad.get_pressed(99, pygame.gamepad.BTN_SOUTH) == False
        and pygame.gamepad.get_axis(99, pygame.gamepad.AXIS_LX) == 0.0)
    # math 3D（批次 K）：右手系叉积 / 90° 轴角四元数 / 复合矩阵
    _cv = pygame.math.Vector3(1, 2, 3).cross(pygame.math.Vector3(0, 1, 0))
    _st("math.cross", (_cv.x, _cv.y, _cv.z) == (-3, 0, 1))
    _q = pygame.math.quat_from_axis_angle(0, 0, 1, 90)
    _st("math.quat", abs(_q.z - 2 ** 0.5 / 2) < 1e-5 and abs(_q.w - 2 ** 0.5 / 2) < 1e-5)
    _mi = pygame.math.mat4_translation(1, 2, 3).mul_mat4(pygame.math.mat4_scale(2, 2, 2))
    _p3 = _mi.transform_point3(pygame.math.Vector3(1, 1, 1))
    _st("math.mat4", abs(_p3.x - 3) < 1e-5 and abs(_p3.z - 5) < 1e-5)
    # draw ellipse/arc（批次 L）：离屏会话绘制不抛错
    try:
        _t = pygame.Surface((64, 64))
        with _t.render() as _tr:
            _tr.ellipse((255, 0, 0), (8, 8, 48, 48))
            _tr.arc((0, 255, 0), (8, 8, 48, 48), 0.0, 3.14159)
        _st("draw.ellipse_arc", True)
    except Exception as _e:
        _st("draw.ellipse_arc", False)
        print("    [selftest] 异常:", _e)
    # smoothscale + set_alpha（批次 L）：尺寸与表面级 alpha 回读
    _sm = pygame.transform.smoothscale(tile, (20, 20))
    _sm.set_alpha(100)
    _st("surface.alpha", _sm.get_alpha() == 100 and _sm.get_size() == (20, 20))
    # gfxdraw（批次 30）：SDL_gfx 签名适配——离屏会话绘制不抛错
    try:
        _g = pygame.Surface((32, 32))
        with _g.render() as _gf:
            pygame.gfxdraw.circle(_gf, 16, 16, 10, (255, 255, 0))
            pygame.gfxdraw.box(_gf, (2, 2, 28, 28), (40, 40, 60))
        _st("gfxdraw.circle_box", True)
    except Exception as _e:
        _st("gfxdraw.circle_box", False)
        print("    [selftest] 异常:", _e)
    # ── 全体系对接批次（mask/Clock/permission/video/深度面/targets/
    #  sprite）运行时自测——判定约定同上：帧 1 执行，[selftest] 三端同源 ──
    # mask：位构造/读写/count/invert/overlap
    try:
        _m = pygame.mask.Mask((8, 8))
        _m.set_at((0, 0)); _m.set_at((1, 1))
        _st("mask.basic", _m.get_at((0, 0)) and not _m.get_at((2, 2)) and _m.count() == 2)
        _m.invert()
        _st("mask.invert", _m.count() == 62)
        _full = pygame.mask.Mask((8, 8), True)
        _st("mask.overlap", _full.overlap(_m, (0, 0)) is not None
            and _full.overlap_area(_m, (0, 0)) == 62)
    except Exception as _e:
        _st("mask.basic", False); print("    [selftest] 异常:", _e)
    # mask.from_surface：影子快路径（表面 alpha 影子）三端同步零回读
    try:
        _fs = pygame.Surface((8, 8))
        _fs.fill((255, 255, 255, 255))   # 不透明面 → 全部置位（全透明面 count=0 是正确语义）
        _mf = pygame.mask.from_surface(_fs, 127)
        _st("mask.from_surface", _mf.count() == 64 and _mf.get_size() == (8, 8))
    except Exception as _e:
        _st("mask.from_surface", False); print("    [selftest] 异常:", _e)
    # time.Clock：上限节流帧时钟（tick 返回毫秒）
    _ck = pygame.time.Clock()
    _ms = _ck.tick(0)
    _st("clock.tick", isinstance(_ms, float) and _ms >= 0.0
        and _ck.get_fps() >= 0.0 and _ck.get_time() >= 0.0)
    # permission：非阻塞项 INTERNET 三平台直通/委派
    _st("permission.internet",
        pygame.permission.ensure(pygame.permission.INTERNET) is True)
    # video：接口可调（缺失文件错误路径 → ValueError）
    try:
        _vs = pygame.video.VideoSystem()
        _bad = False
        try:
            _vs.open("__missing_video__.mp4")
        except ValueError:
            _bad = True
        _st("video.error_path", _bad)
    except Exception as _e:
        _st("video.error_path", False); print("    [selftest] 异常:", _e)
    # surface 深度面：depth_buffer 构造定约（§九.2——depth= 禁用）
    _sd = pygame.Surface((32, 32), depth_buffer=True)
    _sp = pygame.Surface((32, 32))
    _st("surface.depth", _sd.has_depth() and not _sp.has_depth())
    # targets：MRT 会话（一次 pass 镜像双写，不抛错 = 接口通）
    try:
        _ta = pygame.Surface((64, 64)); _tb = pygame.Surface((64, 64))
        with pygame.targets([_ta, _tb]) as t:
            t.fill((60, 20, 80))
            t.rect((255, 255, 255), (8, 8, 48, 48), 2)
        _st("targets.mrt", True)
    except Exception as _e:
        _st("targets.mrt", False); print("    [selftest] 异常:", _e)
    # sprite：配置 → add（世代 id 回写）→ move/position → remove 生命周期
    try:
        _gs = pygame.Surface((16, 16))
        _grp = pygame.sprite.Group()
        _sp1 = pygame.sprite.Sprite(_gs).at(10, 20).tag("enemy")
        _grp.add(_sp1)
        _grp.move_by(_sp1, 5, 0)
        _ok = (_grp.count() == 1 and _grp.alive(_sp1)
               and _grp.position(_sp1) == (15, 20))
        _ok = _ok and _grp.remove(_sp1) and not _grp.alive(_sp1) and _grp.count() == 0
        _st("sprite.lifecycle", _ok)
    except Exception as _e:
        _st("sprite.lifecycle", False); print("    [selftest] 异常:", _e)
    # with_batch 直通口：blit_area（UV 子区域）+ quad（任意四角）
    try:
        _ba = pygame.Surface((32, 16)); _ba.fill((90, 200, 255))
        with screen.render() as r:
            r.blit_area(_ba, (100, 500), (0, 0, 16, 16))          # 源左上 16×16 子区域
            r.quad(_ba, [(600, 480), (660, 490), (650, 540), (590, 530)],
                   [(0, 0), (1, 0), (1, 1), (0, 1)], (255, 255, 255))
        _st("withbatch.direct", True)
    except Exception as _e:
        _st("withbatch.direct", False); print("    [selftest] 异常:", _e)
    # draw 族 FromArgs 收参契约（原 sig_probe 全局探针面迁入——真会话内
    # 验证：正路径各形态全绑定执行；负路径参数层先于会话执行拒绝）
    try:
        with screen.render() as r:
            r.rect((0, 200, 90), (10, 20, 30, 40))                     # 3 元色 + width 缺省
            r.rect(color=(0, 200, 90, 255), rect=(1, 2, 3, 4), width=4)  # kwargs 全形态
            r.circle((255, 200, 0), (5, 6), 7)                         # width 缺省
            r.line((200, 60, 60), (0, 0), (9, 9), 2)                   # width 显式
            r.polygon((90, 120, 220), [(1, 2), (3, 4), (5, 6)])        # 嵌套点列
            r.polygon((10, 20, 30), ((1, 2), (3, 4)))                  # tuple 点列
            r.ellipse((0, 200, 90), (10, 20, 30, 40))                  # width 缺省
            r.ellipse((0, 200, 90), (1, 2, 3, 4), 2)                   # width 显式
            r.arc((200, 60, 60), (10, 20, 30, 40), 0.0, 1.5)           # 弧度制
            r.arc((200, 60, 60), (1, 2, 3, 4), 0, 1)                   # 弧度兼容 int
            r.rect((0, 200, 90), (1, 2, 3, 4))                         # 序列形态（Rect 已退役）
        _ok = True
    except Exception as _e:
        _ok = False; print("    [selftest] 异常:", _e)
    _st("draw.positive", _ok)
    # 负路径：参数层拒绝（TypeError——缺参/越界/非 int/未知 kwarg/点长度错）
    try:
        with screen.render() as r:
            _bad_calls = (
                lambda: r.rect((0, 200), (0, 0, 1, 1)),            # 颜色分量不足
                lambda: r.rect((300, 0, 0), (0, 0, 1, 1)),         # 颜色越界
                lambda: r.rect((0, 0, 0), (1.5, 0, 1, 1)),         # rect 分量 float 严格拒
                lambda: r.rect((0, 0, 0)),                         # 缺 rect（必填 any）
                lambda: r.polygon((0, 0, 0), [(1, 2), (3,)]),      # 点长度错
                lambda: r.rect((0, 0, 0), (0, 0, 1, 1), bogus=1),  # 未知 kwarg
                lambda: r.arc((200, 60, 60), (10, 20, 30, 40)),    # 缺起止角
            )
            _rejected = 0
            for _bad in _bad_calls:
                try:
                    _bad()
                except TypeError:
                    _rejected += 1
        _st("draw.negative", _rejected == len(_bad_calls))
    except Exception as _e:
        _st("draw.negative", False); print("    [selftest] 异常:", _e)
    # event poll/clear + KMOD 常量
    _pe = pygame.event.poll()
    _st("event.poll", _pe is None or hasattr(_pe, "type"))
    pygame.event.clear()
    _st("kmod.const", pygame.key.KMOD_SHIFT == 1 and pygame.key.KMOD_ALT == 4)

    # ── 批次 49（绑定收官）：color parity / 深度会话 / mixer Channel+Effect ──
    try:
        _c = pygame.Color(10, 20, 30)
        _c.update(40, 50, 60, 200)
        _st("color.update", _c.r == 40 and _c.a == 200)
        _st("color.cmy", all(0.0 <= v <= 1.0 for v in _c.cmy))
        _st("color.hsva", 0.0 <= _c.hsva[0] <= 360.0)
        _st("color.hsla", len(_c.hsla) == 4)
        _st("color.i1i2i3", len(_c.i1i2i3) == 3)
        _c.set_length(3)
        _st("color.set_length", _c.r == 40)  # 长度语义（非可迭代——元组化留 v2）
        _st("color.contains", _c.contains(40))
        _named = pygame.Color("red")
        _st("color.named (颜色表装载)", _named.r == 255 and _named.g == 0)
    except Exception as _e:
        _st("color.parity", False); print("    [selftest] 异常:", _e)
    try:
        _st("key.get_focused", isinstance(pygame.key.get_focused(), bool))
        pygame.key.set_repeat(0, 0)
        _st("key.set_repeat", True)
        _st("display.set_caption", (pygame.display.set_caption("probe"), True)[1])
        pygame.display.set_opacity(0.9)
        _st("display.set_opacity", True)
    except Exception as _e:
        _st("display/key.parity", False); print("    [selftest] 异常:", _e)
    try:
        _ch = pygame.mixer.Channel(0)
        _probe_snd = pygame.mixer.Sound(bytes=b"RIFF") if False else None
        _st("mixer.Channel 构造", _ch is not None)
        _st("mixer.num_channels", pygame.mixer.num_channels() >= 1)
        _st("mixer.output_sample_rate", pygame.mixer.output_sample_rate() > 0)
        pygame.mixer.set_reserved(1)
        _st("mixer.set_reserved", True)
        _fx = pygame.mixer.Effect("gain")
        _fx.set("gain", 0.5)
        _st("mixer.Effect.set", True)
        _ch.add_effect(_fx)
        _st("mixer.Channel.add_effect", True)
        _beep = pygame.mixer.beep()
        _st("mixer.beep", _beep is not None)
    except Exception as _e:
        _st("mixer.parity", False); print("    [selftest] 异常:", _e)
    try:
        with screen.render() as rd:
            rd.fill((0, 0, 0))
            rd.rect((0, 200, 90), (10, 10, 50, 50))
        _st("render_depth 会话", True)
    except Exception as _e:
        _st("render_depth 会话", False); print("    [selftest] 异常:", _e)
    print("[selftest] 汇总:", "全部通过" if not _st_fail
          else "失败项 " + repr(_st_fail))

    x = 370
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            # touch 诊断（批次 I）：移动端点按/滑动可看手指表联动
            # （桌面恒 0——引擎仅移动端产生 Touch 事件）
            if e.type == pygame.event.MOUSEBUTTONDOWN:
                print("[diag] tap: touch 手指数 =", pygame.touch.get_count())
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        
        box = (x, 270, 60, 60)              # 区域元组（Rect 已退役——碰撞/触发器归物理引擎）
        # ── 批次 K 数学驱动视觉自证（图元承载，零新建 GPU 表面——
        #  ADR-5 延迟提交红线）：四元数旋转的手 + lerp 脉冲圆 ──
        _ang = (pygame.time.get_ticks() // 6) % 360           # ~2.16s/圈
        _hand = pygame.math.mat4_from_quat(
            pygame.math.quat_from_axis_angle(0, 0, 1, _ang)
        ).transform_point3(pygame.math.Vector3(90, 0, 0))
        _t01 = (pygame.time.get_ticks() % 1600) / 1600
        _pulse = pygame.math.Vector3(0.5, 0.5, 0.5).lerp(
            pygame.math.Vector3(1.6, 1.6, 1.6), 1 - abs(_t01 * 2 - 1))
        with screen.render() as r:
            r.fill((30, 30, 46))
            # 核心查询活体证明：key.get_pressed / mouse.get_pressed 链路
            # （Space 按住 → 描边变红；左键按住 → 方块上出现标记点）
            _space = pygame.key.get_pressed().get(pygame.key.K_SPACE)
            _lmb = pygame.mouse.get_pressed()[0]
            r.rect((255, 80, 80) if _space else (255, 255, 255), (x, 430, 60, 60), 4)
            if _lmb:
                r.circle((80, 255, 120), (x, 270), 8)
            r.rect((0, 200, 90), box)
            # 文字表面 blit（预渲染一次）。注意：内嵌 Antonio 为拉丁字体——
            # 中文字形不存在会被跳过（demo 文案用纯拉丁；CJK 见 line1）
            if line2 is not None:
                r.blit(line2, (10, 60))                           # 哥特风（通用字体）
            r.circle((255, 200, 0), (400, 300), 40)
            r.line((200, 60, 60), (x, 0), (x + 60, 600))
            # 批次 K：quat_from_axis_angle → Mat4 → transform_point3 的指针；
            # Vector3.lerp 三角波驱动脉冲圆半径（旋转/缩放全程数学值承载。
            # 注：数学产物为 float，绘制坐标收 int——SfPoint 契约严格拒 float）
            r.line((255, 120, 120), (260, 220),
                   (260 + int(_hand.x), 220 + int(_hand.y)), 2)
            r.circle((255, 120, 120), (260, 220), 4)
            r.circle((100, 220, 255), (620, 300), max(2, int(_pulse.x * 30)))
            r.rect((100, 220, 255), (600, 280, 40, 40), 2)        # 脉冲圆的外框参照
            # gfxdraw 兼容面（批次 30）：SDL_gfx 签名（目标在前/坐标分离/
            # 颜色在后），实现与 r.circle 同一 batch 零额外成本
            pygame.gfxdraw.circle(r, 700, 300, 26, (255, 120, 120))
            pygame.gfxdraw.hline(r, 660, 740, 342, (120, 200, 255))
            pygame.gfxdraw.filled_circle(r, 700, 382, 16, (120, 255, 160))
            pygame.gfxdraw.box(r, (660, 420, 80, 46), (60, 60, 90))
            r.polygon((90, 120, 220), [(100, 100), (180, 80), (200, 160), (120, 170)])
            r.ellipse((60, 180, 120), (420, 380, 140, 80))                # 批次 L：椭圆
            r.arc((255, 160, 60), (420, 470, 120, 80), 0.0, 3.14159)      # 批次 L：弧（弧度制）
            r.rect(color=(90, 200, 255), rect=(x, 120, 60, 60))   # kwargs 形态（批次 A）
            r.blit(tile, (500, 100))                              # Surface 贴图（批次 F）
            r.blit(rot, (560, 90))                                # transform.rotate（批次 F）
            r.blit(sm, (640, 420))                                # 半透明平滑缩放（批次 L）
            r.blit(tsurf, (300, 20))                              # 文字表面一等化（批次 F）
            if line1 is not None:
                r.blit(line1, (10, 10))                           # 预渲染黄字（中文能力）
        pygame.display.flip()

        x = 0 if x >= 730 else x + 4
        await pygame.next_frame()

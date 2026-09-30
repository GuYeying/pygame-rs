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


def game():
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
    # ── 音频：平台分支（needs_gesture = Web 浏览器自动播放策略，cfg 判定）──
    # 原生（桌面/Android）：无手势限制，直接创建 + 无限循环播放
    # Web：AudioContext 须待首次用户手势（keydown/mousedown 均为
    #  User Activation），snd 保持 None，延后到事件循环的解锁点创建
    snd = None
    if not pygame.mixer.needs_gesture():
        try:
            snd = pygame.mixer.Sound()
            snd.play(-1)                     # 无限循环：播完自动回卷（引擎音频
                                             #  回调内完成，无缝衔接）
            print("[diag] 原生平台：提示音已直接循环播放 len =", snd.get_length())
        except (ValueError, TypeError) as e:
            print("[diag] 原生平台：音频初始化失败（继续无音频运行）:", e)

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
    print("[selftest] 汇总:", "全部通过" if not _st_fail
          else "失败项 " + repr(_st_fail))

    x = 370
    while True:
        for e in pygame.event.get():
            if e.type == pygame.QUIT:
                return
            # 仅 Web 会进入（原生平台 snd 已就绪，None 守卫恒假）：
            # 首次按键/点击 = 用户手势，AudioContext 解锁点
            if e.type in (pygame.KEYDOWN, pygame.MOUSEBUTTONDOWN) and snd is None:
                snd = pygame.mixer.Sound()
                snd.play(-1)
                print("[diag] Web 手势解锁：提示音开始循环播放 len =",
                      snd.get_length(), "busy =", pygame.mixer.get_busy())
            # touch 诊断（批次 I）：移动端点按/滑动可看手指表联动
            # （桌面恒 0——引擎仅移动端产生 Touch 事件）
            if e.type == pygame.MOUSEBUTTONDOWN:
                print("[diag] tap: touch 手指数 =", pygame.touch.get_count())
            if e.type == pygame.KEYDOWN and e.key == pygame.K_ESCAPE:
                return

        screen.fill((30, 30, 46))
        box = pygame.Rect(x, 270, 60, 60)   # Rect 直通句柄（批次 C）：实例直接当 draw 参数
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
            # 核心查询活体证明：key.get_pressed / mouse.get_pressed 链路
            # （Space 按住 → 描边变红；左键按住 → 方块上出现标记点）
            _space = pygame.key.get_pressed().get(pygame.K_SPACE)
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
        yield

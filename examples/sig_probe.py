# sig_probe 脚本（签名契约锚点，批次 A/B）：原生 pygame 模块路径的
# 可执行规格——模块树/常量/Color/事件全部在此锁定。
#
# draw 族 FromArgs 收参（正路径各形态 + 负路径参数层拒绝）已迁往
# binding_probe.py——真会话内验证（原全局探针面 sf_draw_* 随 bridge
# 瘦身退役，2026-10-18）。

RESULTS = []


def expect(cond, msg):
    """布尔断言（模块路径用）。"""
    RESULTS.append(("OK", msg) if cond else ("FAIL", msg))


# ── 原生 pygame 模块路径（批次 B：pymodule 树 + pyclass）────────
import pygame

expect(pygame.event.QUIT == "quit", "事件常量 QUIT 同源定约")
expect(pygame.Color(1, 2, 3) == (1, 2, 3, 255), "Color(1,2,3) 补 alpha")
expect(pygame.Color((4, 5, 6)) == (4, 5, 6, 255), "Color((4,5,6)) 补 alpha")
expect(pygame.Color(7, 8, 9, 10) == (7, 8, 9, 10), "Color(7,8,9,10) 原样")
_c = pygame.Color(1, 2, 3)
expect(type(_c).__name__ == "Color", "Color 是 class（批次三十九）")
expect((_c.r, _c.g, _c.b, _c.a) == (1, 2, 3, 255), "Color r/g/b/a 属性读")
_c.r = 10
expect(_c.r == 10, "Color r 属性写")
_n = _c.normalize()
expect(abs(_n[0] - 10 / 255.0) < 1e-6, "Color.normalize")
expect(pygame.Color(255, 0, 0).premul_alpha().a == 255, "Color.premul_alpha")
_lerped = pygame.Color(0, 0, 0).lerp(pygame.Color(255, 255, 255), 0.5)
expect(_lerped.r == 128 and _lerped.g == 128, "Color.lerp 中点（round）")
expect(pygame.Color(255, 255, 255).invert() == (0, 0, 0, 255), "Color.invert")
expect(pygame.Color(255, 0, 0) == pygame.Color(255, 0, 0), "Color == Color")
expect(pygame.init() == (0, 0), "init() 占位 (0, 0)")
scr = pygame.display.set_mode((8, 6))
expect(type(scr).__name__ == "Screen", "set_mode → Screen 占位")
expect(pygame.event.get() == [], "event.get() 无头排空")
expect(pygame.event.Event is not None, "Event 类暴露（match 类模式解构用）")
expect(pygame.time.get_ticks() >= 0, "time.get_ticks() 非负")
# ── Key：typed 键值（批次 C）────────────────────────────────────
expect(type(pygame.key.K_a).__name__ == "Key", "K_* typed（Key 实例）")
expect(pygame.key.K_a == pygame.key.K_a and pygame.key.K_a != pygame.key.K_b, "Key 值相等/不等")
expect(pygame.key.K_SHIFT == pygame.key.K_LSHIFT, "修饰键别名相等")
expect(pygame.key.K_ESCAPE == pygame.key.K_RETURN or True, "键集完备性占位（恒真，防漏注册报 AttributeError）")

# ── mouse / 旗标（批次 C）───────────────────────────────────────
expect(pygame.mouse.get_pos() == (0, 0), "mouse.get_pos 无头 (0,0)")
expect(pygame.mouse.get_pressed() == (False, False, False), "mouse.get_pressed 无头三 False")
expect(pygame.key.get_pressed().get(pygame.key.K_a) == False, "key.get_pressed().get 无头 False")
expect(pygame.display.RESIZABLE == 1, "display 旗标透传")

# ── font（批次 D：纯度量面；render 需 GPU 由 binding_probe 覆盖）──
font = pygame.font.Font(None, 24)
expect(type(font).__name__ == "Font", "Font(None, 24) 默认字体构造")
w, h = font.size("abc")
expect(w > 0, "size('abc') 宽度为正")
lh = font.line_height
expect(h == lh and lh > 0, "size 高度 = line_height")
expect(font.size("") == (0, font.line_height), "空串 size (0, line_height)")
sf = pygame.font.SysFont("arial", 24)
expect(type(sf).__name__ == "Font", "SysFont 占位默认字体")
try:
    pygame.font.Font(None)
    expect(False, "Font 缺 size 应报 TypeError")
except TypeError:
    expect(True, "Font 缺 size 报 TypeError")

# ── mixer（批次 E：SFX + music；桌面有音频设备）─────────────────
pygame.mixer.init(4)
snd = pygame.mixer.Sound()
expect(type(snd).__name__ == "Sound", "Sound() 内嵌提示音构造")
expect(snd.get_length() > 0.1, "get_length 时长为正")
snd.play()
snd.stop()
expect(pygame.mixer.get_init(), "get_init True")
expect(isinstance(pygame.mixer.get_busy(), bool), "get_busy 返回 bool")
try:
    pygame.mixer.Sound(b"not-an-audio-file")
    expect(False, "Sound 坏字节应报 ValueError")
except ValueError:
    expect(True, "Sound 坏字节报 ValueError")
expect(pygame.mixer.music.get_busy() == False, "music.get_busy 无头 False")

# ── sndarray（批次 H：Sound ↔ 嵌套 list 互转）────────────────────
_sa = pygame.sndarray.array(pygame.mixer.Sound())
expect(isinstance(_sa, list) and len(_sa) > 0, "sndarray.array 非空")
_ms = pygame.sndarray.make_sound(_sa)
expect(type(_ms).__name__ == "Sound", "sndarray.make_sound -> Sound")

# ── image（批次 F：纯查询零 GPU；Surface/transform 需 GPU 由 binding_probe 覆盖）──
expect(pygame.image.get_extended() == True, "image.get_extended True")

# ── touch（批次 I：多点触控查询面；无头桌面恒空表）────────────────
expect(pygame.touch.get_count() == 0, "touch.get_count 桌面恒 0")
expect(pygame.touch.get_fingers() == [], "touch.get_fingers 桌面空表")
expect(pygame.touch.get_pos(1) is None, "touch.get_pos 未知手指 None")

# ── gamepad（批次 J：手柄查询面；不假设硬件——只验类型语义）──────
expect(isinstance(pygame.gamepad.get_count(), int), "gamepad.get_count int")
_p = pygame.gamepad.primary()
expect(_p is None or isinstance(_p, int), "gamepad.primary int|None")
expect(pygame.gamepad.is_connected(99) == False, "gamepad.is_connected 未知 False")
expect(pygame.gamepad.BTN_SOUTH == 0 and pygame.gamepad.BTN_DPAD_RIGHT == 15,
       "BTN_* 整数常量（标准布局索引）")
expect(pygame.gamepad.AXIS_LX == 0 and pygame.gamepad.AXIS_RT == 5,
       "AXIS_* 整数常量")
expect(pygame.gamepad.get_pressed(99, pygame.gamepad.BTN_SOUTH) == False,
       "gamepad.get_pressed 未连接 False")
expect(pygame.gamepad.just_pressed(99, pygame.gamepad.BTN_SOUTH) == False,
       "gamepad.just_pressed 未连接 False")
expect(pygame.gamepad.get_axis(99, pygame.gamepad.AXIS_LX) == 0.0,
       "gamepad.get_axis 未连接 0.0")
try:
    pygame.gamepad.get_pressed(0, 200)
    expect(False, "gamepad 越界索引应报 ValueError")
except ValueError:
    expect(True, "gamepad 越界索引报 ValueError")
expect(len(pygame.gamepad.connected()) == pygame.gamepad.get_count(),
       "connected 数与 get_count 一致")

# ── mouse/key 完整度（mouse.set_pos 垫底改写状态表——无头无事件覆盖）──
expect(isinstance(pygame.mouse.get_rel(), tuple), "mouse.get_rel 相对位移")
expect(pygame.mouse.set_visible(True) is None, "mouse.set_visible")
expect(pygame.mouse.get_visible() == True, "mouse.get_visible 回读")
expect(isinstance(pygame.mouse.get_focused(), bool), "mouse.get_focused")
pygame.mouse.set_pos((10, 20))
expect(pygame.mouse.get_pos() == (10, 20), "mouse.set_pos 垫底状态表")
expect(pygame.key.get_mods() == 0, "key.get_mods 无修饰键 0")
expect(pygame.key.KMOD_SHIFT == 1 and pygame.key.KMOD_CTRL == 2
       and pygame.key.KMOD_ALT == 4 and pygame.key.KMOD_META == 8,
       "KMOD_* 位常量")
_kn = pygame.key.name(pygame.key.K_x)
expect(_kn == "KeyX", "key.name W3C code 命名")
expect(isinstance(pygame.key.get_focused(), bool), "key.get_focused bool")
expect(pygame.key.set_repeat(0, 0) is None, "key.set_repeat 垫底")
expect(pygame.key.get_repeat() == (0, 0), "key.get_repeat 垫底")

# ── io/net/dialog（批次 M：base 直映射；无头桌面。io 往返会在 CWD
#    留下 _sig_io_tmp.bin 测试产物——无删除 API，有意为之）────────
expect(pygame.io.exists("_sig_io_absent.bin") == False, "io.exists 不存在 False")
expect(pygame.io.write("_sig_io_tmp.bin", b"abc") is None, "io.write bytes")
expect(pygame.io.read("_sig_io_tmp.bin") == b"abc", "io.read bytes 往返")
expect(pygame.io.read_text("_sig_io_tmp.bin") == "abc", "io.read_text")
expect(pygame.io.write("_sig_io_tmp.bin", "文本str") is None, "io.write str")
expect(pygame.io.read_text("_sig_io_tmp.bin") == "文本str", "io str→utf8 编码")
expect(not hasattr(pygame.io, "set_base_dir"), "io.set_base_dir 已撤（引擎内部语义不暴露）")
_tcp = pygame.net.TCP("127.0.0.1:1")
expect(type(_tcp).__name__ == "TCP", "net.TCP 构造（立即返回不阻塞）")
expect(_tcp.recv() is None, "net.TCP.recv 无帧 None")
expect(_tcp.state() in ("connecting", "connected", "closed"), "net.TCP.state 合法值")
_tcp.close()
expect(pygame.dialog.open_file is not None, "dialog.open_file 面")
expect(pygame.dialog.save_bytes is not None, "dialog.save_bytes 面")

# ── gfxdraw（批次 30：SDL_gfx 兼容签名面；无头只验路由拒绝）────────
expect(pygame.gfxdraw.circle is not None and pygame.gfxdraw.box is not None
       and pygame.gfxdraw.aacircle is not None, "gfxdraw 函数面（circle/box/aacircle 等）")
try:
    pygame.gfxdraw.circle(None, 5, 6, 7, (255, 0, 0))
    expect(False, "gfxdraw 非法目标应 TypeError")
except TypeError:
    expect(True, "gfxdraw 非法目标报 TypeError")

# ── display flags（批次 O：位翻译；无头时窗口未建 = 垫底 no-op）────
expect(pygame.display.set_mode((8, 6), pygame.display.RESIZABLE | pygame.display.FULLSCREEN) is not None,
       "set_mode flags 位集形态")
expect(pygame.display.set_mode((8, 6), [pygame.display.RESIZABLE, pygame.display.NOFRAME]) is not None,
       "set_mode flags 序列形态")
expect(pygame.display.set_mode((8, 6), pygame.display.ALWAYS_ON_TOP) is not None,
       "set_mode flags ALWAYS_ON_TOP")
expect(pygame.display.set_mode((8, 6)) is not None, "set_mode 无 flags")
try:
    pygame.display.set_mode((8, 6), ["bad"])
    expect(False, "set_mode 非法 flags 项应 TypeError")
except TypeError:
    expect(True, "set_mode 非法 flags 项报 TypeError")

# ── quat/mat 算术（批次 P：AsNumber 槽）───────────────────────────
_q1 = pygame.math.quat_from_axis_angle(0, 0, 1, 90)
_qc = _q1 * _q1
expect(abs(_qc.z - 1) < 1e-5 and abs(_qc.w) < 1e-5, "Quaternion __mul__ 组合（90°×2=180°）")
_qs = _q1 * 2
expect(abs(_qs.length() - 2) < 1e-4, "Quaternion 标量乘")
_qn = -_q1
expect((_qn.x, _qn.y, _qn.z, _qn.w)
       == (-_q1.x, -_q1.y, -_q1.z, -_q1.w), "Quaternion __neg__")
_mi = pygame.math.mat4_identity()
_mc = _mi * _mi
expect(_mc.transform_point3(pygame.math.Vector3(5, 6, 7)).x == 5, "Mat4 __mul__ 恒等组合")
_msn = -pygame.math.mat4_scale(2, 2, 2)
_rp = _msn.mul_mat4(pygame.math.mat4_scale(2, 2, 2)).transform_point3(
    pygame.math.Vector3(1, 1, 1))
expect(abs(_rp.x + 4) < 1e-5 and abs(_rp.z + 4) < 1e-5,
       "Mat4 __neg__ × 组合（-2I·2I=-4I，无 w 除 glam 语义）")

# ── math / version / sndarray（批次 H）───────────────────────────
expect(abs(pygame.math.lerp(0, 10, 0.5) - 5.0) < 1e-6, "math.lerp 兼收 int")
expect(pygame.math.clamp(15, 0, 10) == 10, "math.clamp 夹取")
expect(abs(pygame.math.inv_lerp(0, 10, 5) - 0.5) < 1e-6, "math.inv_lerp")
expect(abs(pygame.math.remap(0, 10, 100, 200, 5) - 150.0) < 1e-6, "math.remap")
expect(pygame.math.smoothstep(0, 10, 5) == 0.5, "math.smoothstep 中点")
v3 = pygame.math.Vector3(1, 2, 3)
expect(type(v3).__name__ == "Vector3", "Vector3 构造")
expect(v3.length_squared() == 14, "Vector3.length_squared")
c = v3.cross(pygame.math.Vector3(0, 1, 0))
expect((c.x, c.y, c.z) == (-3, 0, 1), "Vector3.cross 右手系")
n = v3.normalize()
expect(abs(n.x - 1 / 14 ** 0.5) < 1e-5, "Vector3.normalize")
expect(pygame.math.Vector3(0, 0, 0).normalize().length() == 0, "Vector3 零向量归一 = 零")
z = v3.lerp(pygame.math.Vector3(2, 4, 6), 0.5)
expect(abs(z.x - 1.5) < 1e-5 and abs(z.z - 4.5) < 1e-5, "Vector3.lerp")
v3.scale_to_length(14 ** 0.5)
expect(abs(v3.x - 1) < 1e-5, "Vector3.scale_to_length 就地")
q = pygame.math.quat_from_axis_angle(0, 0, 1, 90)
expect(abs(q.w - 2 ** 0.5 / 2) < 1e-5 and abs(q.z - 2 ** 0.5 / 2) < 1e-5,
       "quat_from_axis_angle 90°")
qi = pygame.math.quat_identity()
expect((qi.x, qi.y, qi.z, qi.w) == (0, 0, 0, 1), "quat_identity")
qh = qi.slerp(q, 0.5)
expect(abs(qh.length() - 1) < 1e-5, "quat.slerp 保单位长")
m = pygame.math.mat4_from_quat(q)
p = m.transform_point3(pygame.math.Vector3(1, 0, 0))
expect(abs(p.x) < 1e-5 and abs(p.y - 1) < 1e-5, "Mat4 变换点（绕 Z 转 90°）")
mi = pygame.math.mat4_identity()
p2 = mi.transform_point3(pygame.math.Vector3(5, 6, 7))
expect((p2.x, p2.y, p2.z) == (5, 6, 7), "mat4_identity 恒等")
mt = pygame.math.mat4_translation(1, 2, 3).mul_mat4(pygame.math.mat4_scale(2, 2, 2))
p3 = mt.transform_point3(pygame.math.Vector3(1, 1, 1))
expect(abs(p3.x - 3) < 1e-5 and abs(p3.z - 5) < 1e-5, "mat4 平移×缩放叠加")

# ── 向量算术协议（批次 N：AsNumber 槽接线）────────────────────────
_v2a = pygame.math.Vector2(1, 2)
_v2b = pygame.math.Vector2(3, 4)
_s = _v2a + _v2b
expect((_s.x, _s.y) == (4, 6), "Vector2 __add__")
_d = _v2a - _v2b
expect((_d.x, _d.y) == (-2, -2), "Vector2 __sub__")
_m = _v2a * 2
expect((_m.x, _m.y) == (2, 4), "Vector2 标量乘（int 兼收）")
_rm = 2 * _v2a
expect((_rm.x, _rm.y) == (2, 4), "Vector2 __rmul__")
_vm2 = _v2a * _v2b
expect((_vm2.x, _vm2.y) == (3, 8), "Vector2 元素级乘（glam 语义）")
_dv = _v2b / 2
expect((_dv.x, _dv.y) == (1.5, 2), "Vector2 __truediv__")
_ng = -_v2a
expect((_ng.x, _ng.y) == (-1, -2), "Vector2 __neg__")
try:
    _v2a + 1
    expect(False, "Vector2 + 标量应 TypeError")
except TypeError:
    expect(True, "Vector2 + 标量报 TypeError")
try:
    _v2b / 0
    expect(False, "Vector2 零除应 ZeroDivisionError")
except ZeroDivisionError:
    expect(True, "Vector2 零除报 ZeroDivisionError")
_v2c = pygame.math.Vector2(1, 1)
_v2c += _v2b
expect((_v2c.x, _v2c.y) == (4, 5), "Vector2 __iadd__ 就地变更")
_v3s = pygame.math.Vector3(1, 2, 3) - pygame.math.Vector3(1, 1, 1)
expect((_v3s.x, _v3s.y, _v3s.z) == (0, 1, 2), "Vector3 __sub__")
_v3m = pygame.math.Vector3(1, 2, 3) * 2
expect((_v3m.x, _v3m.y, _v3m.z) == (2, 4, 6), "Vector3 标量乘")
v2 = pygame.math.Vector2(3, 4)
expect(type(v2).__name__ == "Vector2", "Vector2 构造")
expect(abs(v2.length() - 5.0) < 1e-6, "Vector2.length")
n = v2.normalize()
expect(abs(n.x - 0.6) < 1e-6 and abs(n.y - 0.8) < 1e-6, "Vector2.normalize")
expect(abs(v2.dot(pygame.math.Vector2(1, 0)) - 3.0) < 1e-6, "Vector2.dot")
expect(pygame.version.ver == "0.9.56", "version.ver")  # 随 starfish workspace 版本联动更新
expect(pygame.version.get_version() == (0, 9, 56), "version.vernum")
#expect(pygame.version.get_wgpu_version() == (2, 0, 20), "version.get_sdl_version 占位")

for status, msg in RESULTS:
    print("[%s] %s" % (status, msg))

_failures = [r for r in RESULTS if r[0] != "OK"]
if _failures:
    raise AssertionError("sig_probe %d/%d 项未过" % (len(_failures), len(RESULTS)))
print("sig_probe: %d 项全过" % len(RESULTS))

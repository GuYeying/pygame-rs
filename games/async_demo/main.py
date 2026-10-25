# async_demo（产品化批次 58）：**协程入口**形态——`async def main():` +
# `await pygame.next_frame()`（与生成器形态 `async def main(): ... yield` 等价；
# 为将来 `await` 网络 IO 不卡帧铺路——net 模块已备非阻塞轮询，真
# awaitable 形态后续接入）。
import pygame


async def main():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 24)
    title = cjk.render("async def main() + await pygame.next_frame() - OK", True, (120, 220, 120))
    frame = 0
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
        with screen.render() as t:
            t.fill((16, 18, 26))
            t.blit(title, (40, 40))
            t.rect((90, 90, 150), (560, 300, 40 + (frame % 60), 40), 0)
        pygame.display.flip()
        frame += 1
        await pygame.next_frame()

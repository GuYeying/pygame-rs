import pygame


async def main():
    screen = pygame.display.set_mode((800, 600))
    cjk = pygame.font.Font("cjk", 24)
    title = cjk.render("games/ mini_game - runner OK", True, (120, 220, 120))
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
        with screen.render() as t:
            t.fill((16, 18, 26))
            t.blit(title, (40, 40))
        pygame.display.flip()
        await pygame.next_frame()

"""
sprites.py — All sprite/animation loading for units, merchants, ships, and effects.
Call init() after pygame display is set up.
"""

import pygame


# === Shared state ===
anim_tick = 0  # global animation counter, incremented in game loop

# === Faction flag images ===
faction_images = {}

# === Infantry animations (per faction) ===
infantry_anims = {}  # {"Great Britain": {...}, "France": {...}, ...}

# === Cavalry animations (per faction) ===
cavalry_anims = {}  # {"Great Britain": {...}, "France": {...}, ...}

# === 1800 era sprites (loaded but swapped in at year 1800) ===
infantry_anims_1800 = {}
cavalry_anims_1800 = {}

# === Canoe animations (for native factions on water) ===
canoe_anims = {}

# === Ship animations ===
ship_anims = {}

# === Merchant animations ===
merchant_anims = {}
native_merchant_anims = {}
haiti_merchant_anims = {}

# === Explosion ===
explosion_frames = []
explosion_frame_count = 0
explosion_anim_speed = 4


def _load_infantry_sheet(path, fw=20, fh=40):
    """Load a standard infantry spritesheet (20x40 frames, attack row 9 at 30x40)."""
    sheet = pygame.image.load(path).convert_alpha()

    def frames(row, count):
        result = []
        for i in range(count):
            x, y = i * fw, row * fh
            if x + fw <= sheet.get_width() and y + fh <= sheet.get_height():
                result.append(sheet.subsurface(pygame.Rect(x, y, fw, fh)))
        return result if result else [sheet.subsurface(pygame.Rect(0, 0, fw, fh))]

    attack_frames = []
    for i in range(4):
        x = i * 30
        if x + 30 <= sheet.get_width() and 9 * fh + fh <= sheet.get_height():
            attack_frames.append(sheet.subsurface(pygame.Rect(x, 9 * fh, 30, fh)))

    return {
        "idle": frames(0, 1),
        "walk_right": frames(1, 2),
        "walk_left": frames(2, 2),
        "walk_up": frames(3, 2),
        "walk_down": frames(4, 2),
        "walk_br": frames(5, 2),
        "walk_bl": frames(6, 2),
        "walk_tl": frames(7, 2),
        "walk_tr": frames(8, 2),
        "attack": attack_frames if attack_frames else frames(0, 1),
    }


def _load_cavalry_sheet(path, fh=40):
    """Load cavalry spritesheet with mixed frame sizes per row."""
    sheet = pygame.image.load(path).convert_alpha()
    y = 0  # track vertical offset since rows have different heights

    # Row 1: idle — 1 frame, 20x40
    idle = [sheet.subsurface(pygame.Rect(0, y, 20, fh))]
    y += fh

    # Row 2: right — 2 frames, 40x40
    right = []
    for i in range(2):
        right.append(sheet.subsurface(pygame.Rect(i * 40, y, 40, fh)))
    y += fh

    # Row 3: left — 2 frames, 40x40
    left = []
    for i in range(2):
        left.append(sheet.subsurface(pygame.Rect(i * 40, y, 40, fh)))
    y += fh

    # Row 4: down — 2 frames, 20x40
    down = []
    for i in range(2):
        down.append(sheet.subsurface(pygame.Rect(i * 20, y, 20, fh)))
    y += fh

    # Row 5: up — 2 frames, 20x40
    up = []
    for i in range(2):
        up.append(sheet.subsurface(pygame.Rect(i * 20, y, 20, fh)))
    y += fh

    # Row 6: bottom-right — 2 frames, 30x40
    br = []
    for i in range(2):
        br.append(sheet.subsurface(pygame.Rect(i * 30, y, 30, fh)))
    y += fh

    # Row 7: bottom-left — 2 frames, 30x40
    bl = []
    for i in range(2):
        bl.append(sheet.subsurface(pygame.Rect(i * 30, y, 30, fh)))
    y += fh

    # Row 8: top-left — 2 frames, 30x40
    tl = []
    for i in range(2):
        tl.append(sheet.subsurface(pygame.Rect(i * 30, y, 30, fh)))
    y += fh

    # Row 9: top-right — 2 frames, 30x40
    tr = []
    for i in range(2):
        tr.append(sheet.subsurface(pygame.Rect(i * 30, y, 30, fh)))
    y += fh

    # Row 10: attack — 6 frames, 40x40
    attack = []
    for i in range(6):
        attack.append(sheet.subsurface(pygame.Rect(i * 40, y, 40, fh)))

    return {
        "idle": idle,
        "walk_right": right,
        "walk_left": left,
        "walk_down": down,
        "walk_up": up,
        "walk_br": br,
        "walk_bl": bl,
        "walk_tl": tl,
        "walk_tr": tr,
        "attack": attack,
    }


def _load_canoe_sheet(path):
    """Load canoe spritesheet. Rows have mixed heights: 30px for horizontal/diagonal, 40px for up/down."""
    sheet = pygame.image.load(path).convert_alpha()
    y = 0

    # Row 1: right — 2 frames, 40x30
    right = []
    for i in range(2):
        right.append(sheet.subsurface(pygame.Rect(i * 40, y, 40, 30)))
    y += 30

    # Row 2: left — 2 frames, 40x30
    left = []
    for i in range(2):
        left.append(sheet.subsurface(pygame.Rect(i * 40, y, 40, 30)))
    y += 30

    # Row 3: down — 2 frames, 20x40
    down = []
    for i in range(2):
        down.append(sheet.subsurface(pygame.Rect(i * 20, y, 20, 40)))
    y += 40

    # Row 4: up — 2 frames, 20x40
    up = []
    for i in range(2):
        up.append(sheet.subsurface(pygame.Rect(i * 20, y, 20, 40)))
    y += 40

    # Row 5: bottom-right — 2 frames, 40x30
    br = []
    for i in range(2):
        br.append(sheet.subsurface(pygame.Rect(i * 40, y, 40, 30)))
    y += 30

    # Row 6: bottom-left — 2 frames, 40x30
    bl = []
    for i in range(2):
        bl.append(sheet.subsurface(pygame.Rect(i * 40, y, 40, 30)))
    y += 30

    # Row 7: top-left — 2 frames, 40x30
    tl = []
    for i in range(2):
        tl.append(sheet.subsurface(pygame.Rect(i * 40, y, 40, 30)))
    y += 30

    # Row 8: top-right — 2 frames, 40x30
    tr = []
    for i in range(2):
        tr.append(sheet.subsurface(pygame.Rect(i * 40, y, 40, 30)))

    return {
        "right": right,
        "left": left,
        "down": down,
        "up": up,
        "br": br,
        "bl": bl,
        "tl": tl,
        "tr": tr,
    }


def _load_merchant_sheet(path, fw=20, fh=40):
    """Load a merchant spritesheet with mixed frame sizes."""
    sheet = pygame.image.load(path).convert_alpha()

    def frames(row, count):
        result = []
        for i in range(count):
            x, y = i * fw, row * fh
            if x + fw <= sheet.get_width() and y + fh <= sheet.get_height():
                result.append(sheet.subsurface(pygame.Rect(x, y, fw, fh)))
        return result if result else [sheet.subsurface(pygame.Rect(0, 0, fw, fh))]

    # Row 8: trading — mixed sizes (20, 30, 30, 20)
    y8 = 8 * fh
    trading = [
        sheet.subsurface(pygame.Rect(0, y8, 20, 40)),
        sheet.subsurface(pygame.Rect(20, y8, 30, 40)),
        sheet.subsurface(pygame.Rect(50, y8, 30, 40)),
        sheet.subsurface(pygame.Rect(80, y8, 20, 40)),
    ]

    # Row 9: resting — 2 frames at 30x40
    y9 = 9 * fh
    resting = [
        sheet.subsurface(pygame.Rect(0, y9, 30, 40)),
        sheet.subsurface(pygame.Rect(30, y9, 30, 40)),
    ]

    return {
        "walk_right": frames(0, 2),
        "walk_left": frames(1, 2),
        "walk_up": frames(2, 2),
        "walk_down": frames(3, 2),
        "walk_br": frames(4, 2),
        "walk_bl": frames(5, 2),
        "walk_tl": frames(6, 2),
        "walk_tr": frames(7, 2),
        "trading": trading,
        "resting": resting,
    }


def _load_ship_sheet(path, fh=40):
    """Load ship spritesheet with mixed frame sizes."""
    sheet = pygame.image.load(path).convert_alpha()

    def frames_40(row, count):
        result = []
        for i in range(count):
            x, y = i * 40, row * fh
            if x + 40 <= sheet.get_width() and y + fh <= sheet.get_height():
                result.append(sheet.subsurface(pygame.Rect(x, y, 40, fh)))
        return result if result else [sheet.subsurface(pygame.Rect(0, 0, 40, fh))]

    def frames_30(row, count):
        result = []
        for i in range(count):
            x, y = i * 30, row * fh
            if x + 30 <= sheet.get_width() and y + fh <= sheet.get_height():
                result.append(sheet.subsurface(pygame.Rect(x, y, 30, fh)))
        return result if result else [sheet.subsurface(pygame.Rect(0, 0, 30, fh))]

    return {
        "right": frames_40(0, 2),
        "left": frames_40(1, 2),
        "down": frames_30(2, 2),
        "up": frames_30(3, 2),
        "tl": frames_40(4, 2),
        "tr": frames_40(5, 2),
        "bl": frames_40(6, 2),
        "br": frames_40(7, 2),
        "attack": frames_30(8, 4),
    }


def init():
    """Load all sprites. Call after pygame.init() and display mode is set."""
    global faction_images, infantry_anims, cavalry_anims, infantry_anims_1800, cavalry_anims_1800, canoe_anims, ship_anims, merchant_anims
    global native_merchant_anims, haiti_merchant_anims, explosion_frames, explosion_frame_count

    # Faction flags
    faction_images = {
        k: pygame.transform.scale(pygame.image.load(v), (30, 30))
        for k, v in [
            ("France", "images/flags/france.png"),
            ("Spain", "images/flags/spain.png"),
            ("Great Britain", "images/flags/britian.png"),
            ("Russia", "images/flags/russia.webp"),
            ("Iroquois", "images/flags/iroquois.png"),
            ("Wabanaki", "images/flags/wabanaki.png"),
            ("Comanche", "images/flags/comanche.png"),
            ("Cree", "images/flags/cree.webp"),
            ("Dakota", "images/flags/dakota.png"),
            ("Denmark", "images/flags/denmark.webp"),
            ("Rupert's Land", "images/flags/rupertsland.webp"),
            ("Pirates", "images/flags/pirates.webp"),
        ]
    }

    # Infantry spritesheets
    infantry_anims = {
        "Great Britain": _load_infantry_sheet("images/sprites/britinf.png"),
        "France": _load_infantry_sheet("images/sprites/frenchinf.png"),
        "Russia": _load_infantry_sheet("images/sprites/rusinf.png"),
        "Spain": _load_infantry_sheet("images/sprites/spaininf.png"),
        "United States": _load_infantry_sheet("images/sprites/americaninf.png"),
        "Pirates": _load_infantry_sheet("images/sprites/pirateinf.png"),
        "Denmark": _load_infantry_sheet("images/sprites/daneinf.png"),
        "Iroquois": _load_infantry_sheet("images/sprites/gunbearer.png"),
        "Wabanaki": _load_infantry_sheet("images/sprites/gunbearer.png"),
        "Cree": _load_infantry_sheet("images/sprites/bowman.png"),
        "Dakota": _load_infantry_sheet("images/sprites/bowman.png"),
        "Rupert's Land": _load_infantry_sheet("images/sprites/caninf.png"),
    }

    # Cavalry spritesheets
    cavalry_anims.update({
        "Great Britain": _load_cavalry_sheet("images/sprites/britishcav.png"),
        "France": _load_cavalry_sheet("images/sprites/frenchcav.png"),
        "Russia": _load_cavalry_sheet("images/sprites/russiancav.png"),
        "Spain": _load_cavalry_sheet("images/sprites/spanishcav.png"),
        "United States": _load_cavalry_sheet("images/sprites/americancav.png"),
        "Denmark": _load_cavalry_sheet("images/sprites/danecav.png"),
        "Comanche": _load_cavalry_sheet("images/sprites/horseman.png"),
        "Dakota": _load_cavalry_sheet("images/sprites/horseman.png"),
        "Rupert's Land": _load_cavalry_sheet("images/sprites/cancav.png"),
    })

    # 1800 era infantry spritesheets
    infantry_anims_1800.update({
        "Great Britain": _load_infantry_sheet("images/sprites/britinf1800.png"),
        "France": _load_infantry_sheet("images/sprites/frenchinf1800.png"),
        "Russia": _load_infantry_sheet("images/sprites/rusinf1800.png"),
        "Spain": _load_infantry_sheet("images/sprites/spaininf1800.png"),
        "United States": _load_infantry_sheet("images/sprites/americaninf1800.png"),
        "Denmark": _load_infantry_sheet("images/sprites/daneinf1800.png"),
    })

    # 1800 era cavalry spritesheets
    cavalry_anims_1800.update({
        "Great Britain": _load_cavalry_sheet("images/sprites/britcav1800.png"),
        "France": _load_cavalry_sheet("images/sprites/frenchcav1800.png"),
        "Russia": _load_cavalry_sheet("images/sprites/ruscav1800.png"),
        "Spain": _load_cavalry_sheet("images/sprites/spaincav1800.png"),
        "United States": _load_cavalry_sheet("images/sprites/americancav1800.png"),
        "Denmark": _load_cavalry_sheet("images/sprites/danecav1800.png"),
    })

    # Canoe sprites (shared by all native factions)
    canoe_anims = _load_canoe_sheet("images/sprites/canoe.png")

    # Ship sprites
    ship_anims = _load_ship_sheet("images/sprites/ship.png")

    # Merchant sprites
    merchant_anims = _load_merchant_sheet("images/sprites/merchant.png")
    native_merchant_anims = _load_merchant_sheet("images/sprites/natmerchant.png")
    haiti_merchant_anims = _load_merchant_sheet("images/sprites/blackmerchant.png")

    # Explosion frames
    try:
        from PIL import Image as PILImage

        def _load_gif_frames(path, size=(24, 24)):
            pil_img = PILImage.open(path)
            frames = []
            try:
                while True:
                    frame = pil_img.convert("RGBA").resize(size, PILImage.LANCZOS)
                    frames.append(pygame.image.fromstring(frame.tobytes(), size, "RGBA"))
                    pil_img.seek(pil_img.tell() + 1)
            except EOFError:
                pass
            return frames if frames else [pygame.Surface(size, pygame.SRCALPHA)]

        explosion_frames = _load_gif_frames("images/explosion.gif", (24, 24))
    except ImportError:
        explosion_frames = [pygame.transform.scale(pygame.image.load("images/explosion.gif"), (24, 24))]

    explosion_frame_count = len(explosion_frames)


def get_explosion_frame(size):
    """Get the current explosion animation frame at the given size."""
    idx = (anim_tick // explosion_anim_speed) % explosion_frame_count
    frame = explosion_frames[idx]
    if frame.get_size() != (size, size):
        return pygame.transform.scale(frame, (size, size))
    return frame


def get_infantry_anims(faction):
    """Get infantry animation dict for a faction, or None if not available."""
    return infantry_anims.get(faction)


def swap_to_1800_sprites():
    """Swap colonial infantry/cavalry sprites to 1800 era versions."""
    global infantry_anims, cavalry_anims
    for faction, anims in infantry_anims_1800.items():
        infantry_anims[faction] = anims
    for faction, anims in cavalry_anims_1800.items():
        cavalry_anims[faction] = anims

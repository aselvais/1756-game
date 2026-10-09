"""Colonial Simulator — Grand Strategy Colonial Simulation Game
==============================================
Entry point and main game loop.

Project Structure:
    index.py       - Main game loop, entities, rendering, events
    collision.py   - Collision detection & A* pathfinding (collisionmap.png)
    sprites.py     - Spritesheet loading for all units, ships, merchants
    config.py      - Game constants, balance values, formation costs
    game_state.py  - Shared mutable game state (entity lists, diplomacy)
    warfare.py     - War scoring constants and system documentation
    utilities.py   - Regiment/settlement name generators

Collision Rules:
    - collisionmap.png: RED = land, BLUE = water
    - Units/Merchants/Settlers can ONLY walk on RED (land)
    - Ships can ONLY sail on BLUE (water)
"""

import pygame, random, math, os, sys, io
import config
import game_state
import warfare
_orig_stdout = sys.stdout; sys.stdout = io.StringIO()
from utilities import random_french_regiment, random_british_regiment, random_spanish_regiment, random_russian_regiment, random_comanche_settlement, random_american_regiment, random_cree_settlement, random_danish_regiment, random_governor_name, random_chief_name, random_dakota_settlement
sys.stdout = _orig_stdout
pygame.init(); os.environ["SDL_VIDEO_CENTERED"] = "1"
INFO = pygame.display.Info(); SCREEN_W, SCREEN_H = INFO.current_w, INFO.current_h
# Scale map to fill screen while keeping aspect ratio.
# New map (nasatelliteview.jpg) is 5595x7717 (includes South America), aspect ratio ~0.725.
# Native coordinate space enlarged 3x so land/regions aren't compressed.
_COORD_SCALE = 3  # all fixed coordinates (cities etc.) are multiplied by this at load
_MAP_NATIVE_W = 533 * _COORD_SCALE  # 1599
_MAP_NATIVE_H = int(533 * (7717 / 5595)) * _COORD_SCALE  # ~2205
_scale_factor = min(SCREEN_W / _MAP_NATIVE_W, SCREEN_H / _MAP_NATIVE_H)
_DISPLAY_W, _DISPLAY_H = int(_MAP_NATIVE_W * _scale_factor), int(_MAP_NATIVE_H * _scale_factor)
fullscreen = True
# Use SDL2's GPU-accelerated presentation: SCALED routes the final frame through the
# hardware renderer, DOUBLEBUF + vsync reduce tearing/stutter. Fall back gracefully if the
# driver can't provide a vsync'd accelerated surface.
_DISPLAY_FLAGS = pygame.FULLSCREEN | pygame.SCALED | pygame.DOUBLEBUF
def _make_display(size, flags):
    try:
        return pygame.display.set_mode(size, flags, vsync=1)
    except Exception:
        try:
            return pygame.display.set_mode(size, flags)
        except Exception:
            return pygame.display.set_mode(size, pygame.FULLSCREEN if (flags & pygame.FULLSCREEN) else 0)
screen = _make_display((SCREEN_W, SCREEN_H), _DISPLAY_FLAGS)
pygame.display.set_caption("Colonial Simulator"); clock = pygame.time.Clock()
font = pygame.font.SysFont(None, 24); small_font = pygame.font.SysFont(None, 20)
news_font = pygame.font.SysFont(None, 22); treaty_font = pygame.font.SysFont(None, 24)
# Load map at FULL source resolution (5595x7717) so zooming in reveals real pixel detail
img_raw = pygame.image.load("images/nasasatelliteview.jpg").convert()
_MAP_SRC_W, _MAP_SRC_H = img_raw.get_width(), img_raw.get_height()
# Pre-build downscaled "mip levels" of the map. When zoomed out we scale from a small copy
# (cheap) instead of the full 43MP source; when zoomed in we use full-res for detail.
# Each entry: (surface, width, height). Level 0 = full source.
_MAP_MIPS = [(img_raw, _MAP_SRC_W, _MAP_SRC_H)]
for _div in (2, 4):
    try:
        _mw, _mh = _MAP_SRC_W // _div, _MAP_SRC_H // _div
        _MAP_MIPS.append((pygame.transform.smoothscale(img_raw, (_mw, _mh)).convert(), _mw, _mh))
    except Exception:
        break
def _pick_map_mip(disp_w):
    """Choose the smallest map copy whose width is still >= the on-screen display width,
    so we never scale down from far more pixels than are actually shown. _MAP_MIPS is
    ordered largest -> smallest."""
    chosen = _MAP_MIPS[0]
    for level in _MAP_MIPS:
        if level[1] >= disp_w:
            chosen = level   # still enough detail; keep shrinking toward the smallest valid
        else:
            break            # too small from here on
    return chosen
# Min zoom fits the map by WIDTH (so North America fills the screen width, not squished by the tall height)
ZOOM_MIN = SCREEN_W / _MAP_NATIVE_W
# Start zoomed to fill screen width — North America appears full-size, pan down for South America
zoom = ZOOM_MIN
# Max zoom: allow zooming past full source pixel density so individual pixels become visible.
ZOOM_MAX = max(zoom * 6.0, (_MAP_SRC_W / _MAP_NATIVE_W) * 2.0)
# Center camera on North America (top portion of the map)
cam_x = 0
cam_y = 0
CAM_Y_MAX = 0; PAN_SPEED = config.PAN_SPEED; DAY_TICKS = config.DAY_TICKS
# Collision map — separate from background. Red = land, Blue = water
_collision_map = pygame.transform.scale(pygame.image.load("images/collisonmap.png"), (_MAP_NATIVE_W, _MAP_NATIVE_H))
# Climate/biome map — pixel colors determine biome at each position
_climate_map = pygame.transform.scale(pygame.image.load("images/climates.png"), (_MAP_NATIVE_W, _MAP_NATIVE_H))

# === Region/Territory System ===
# regionmap.png: red land plots separated by black lines = territories.
# All territories start unclaimed (gray, low opacity overlay).
_REGION_RES = 1  # overlay at native map resolution; territories are big blobs so full-res
# is wasteful (a ~5595px overlay was being sliced+scaled every frame, causing choppiness).
_region_overlay = None  # pre-built RGBA surface: red areas -> gray semi-transparent

_region_overlay_masks = None  # cached (white_mask, black_mask, land_mask, shape) from the regionmap

def _build_region_overlay():
    """Build the SINGLE territory overlay, coloured by ownership:
      - a region owned by a nation is filled with that nation's colour (clean, part of the map)
      - unclaimed / native-held regions show light gray
      - the #000000 border lines stay as thin dark lines
      - ocean / white background is fully transparent
    Rebuilt only when ownership changes (see mark_claims_dirty), not per frame."""
    global _region_overlay, _region_overlay_masks, _region_owner
    try:
        from PIL import Image as PILImage
        import numpy as np
        # Cache the regionmap masks + per-pixel tile colour once; reuse on every rebuild.
        if _region_overlay_masks is None:
            ow = _MAP_NATIVE_W * _REGION_RES
            oh = _MAP_NATIVE_H * _REGION_RES
            pil = PILImage.open("images/regionmap.png").convert("RGB").resize((ow, oh), PILImage.NEAREST)
            arr = np.asarray(pil, dtype=np.int16)
            r = arr[:, :, 0]; g = arr[:, :, 1]; b = arr[:, :, 2]
            white = (r > 235) & (g > 235) & (b > 235)
            black = (r <= 3) & (g <= 3) & (b <= 3)
            land = ~white & ~black
            # Per-pixel region id, sampled from the detection grid (nearest).
            if _region_id_grid is not None:
                gh, gw = _region_id_grid.shape
                ys = (np.arange(oh) * gh // oh).clip(0, gh - 1)
                xs = (np.arange(ow) * gw // ow).clip(0, gw - 1)
                pix_rid = _region_id_grid[np.ix_(ys, xs)]   # (oh, ow) region id per pixel
            else:
                pix_rid = np.full((oh, ow), -1, dtype=np.int32)
            _region_overlay_masks = (white, black, land, pix_rid, ow, oh)
        white, black, land, pix_rid, ow, oh = _region_overlay_masks

        owners = _compute_region_owners() if _region_id_grid is not None else {}
        _region_owner = owners

        out = np.zeros((oh, ow, 4), dtype=np.uint8)
        # Base: all land tiles start gray (unclaimed)
        out[land] = [130, 130, 130, 70]
        # Owned regions: recolour to the owning nation's colour, more opaque so it reads as the map
        if owners:
            maxid = int(pix_rid.max()) if pix_rid.size else -1
            if maxid >= 0:
                lut = np.zeros((maxid + 2, 4), dtype=np.uint8)   # rgba per region id
                for rid, owner in owners.items():
                    if rid < 0 or rid > maxid: continue
                    col = FACTION_COLORS.get(owner) or next(
                        (c["color"] for c in cities if c.get("owner") == owner and c.get("color")), (150, 150, 150))
                    lut[rid] = (col[0], col[1], col[2], 130)
                safe = np.where(pix_rid >= 0, pix_rid, maxid + 1)
                colored = lut[safe]                              # (oh, ow, 4)
                has_col = colored[:, :, 3] > 0
                sel = land & has_col
                out[sel] = colored[sel]
        # Borders on top so territory edges stay crisp
        out[black] = [0, 0, 0, 110]
        _region_overlay = pygame.image.frombuffer(out.tobytes(), (ow, oh), "RGBA").convert_alpha()
    except Exception:
        _region_overlay = None

# === Region Detection & City Assignment ===
# Flood-fill the region map to give each distinct territory a unique ID.
# Then assign each city to whatever territory its coordinates fall inside.
_region_id_grid = None       # 2D array [gy][gx] -> region id (-1 = water/background)
_REGION_GRID_W = 0
_REGION_GRID_H = 0
_region_count = 0

_region_colors = {}          # region id -> (r,g,b) dominant tile color
_region_centroids = {}       # region id -> (gx, gy) grid centroid (for number labels)

def _detect_regions():
    """Detect distinct territories on regionmap.png.

    Each tile is drawn as its own solid color, enclosed by black border lines, on a
    white background. We treat every black-bordered colored blob as one region:
    connected-component labeling over 'colored' pixels (not white, not black) gives
    one stable id per tile — immune to the anti-aliasing that makes a naive
    unique-color count over-count. Each region also stores its dominant color and
    centroid (used for on-map number labels and future faction claiming)."""
    global _region_id_grid, _REGION_GRID_W, _REGION_GRID_H, _region_count
    global _region_colors, _region_centroids
    try:
        from PIL import Image as PILImage
        import numpy as np
        # High grid resolution so the thin black borders fully separate adjacent tiles.
        # At low res (e.g. 800-1100) borders vanish and neighboring tiles merge into one
        # blob, which drops regions. Measured on this map: count stabilizes at ~2400px.
        gw = 2400
        gh = int(gw * (_MAP_NATIVE_H / _MAP_NATIVE_W))
        _REGION_GRID_W, _REGION_GRID_H = gw, gh
        pil = PILImage.open("images/regionmap.png").convert("RGB").resize((gw, gh), PILImage.NEAREST)
        arr = np.asarray(pil, dtype=np.int16)
        r = arr[:, :, 0]; g = arr[:, :, 1]; b = arr[:, :, 2]
        white = (r > 235) & (g > 235) & (b > 235)      # ocean / background
        # Border lines are strictly #000000 (rgb 0,0,0). Match tightly (<=3 for resize
        # rounding only) so dark-colored tiles are NOT mistaken for border.
        black = (r <= 3) & (g <= 3) & (b <= 3)
        is_land = ~white & ~black                        # colored tile interiors
        try:
            from scipy import ndimage
            labeled, cur = ndimage.label(is_land)        # 1..cur, 0 = background
            # Drop tiny specks (anti-alias fragments) by merging them back to background.
            # Scaled to resolution: at 2400px real tiles are hundreds of px; 40 is safe.
            sizes = np.bincount(labeled.ravel())
            min_px = 40
            small = np.where(sizes < min_px)[0]
            if small.size:
                remap = np.arange(sizes.size)
                remap[small] = 0
                labeled = remap[labeled]
                # Re-pack ids so they're contiguous 1..N
                uniq = np.unique(labeled)
                pack = np.zeros(uniq.max() + 1, dtype=np.int32)
                pack[uniq] = np.arange(uniq.size)
                labeled = pack[labeled]
                cur = int(uniq.size) - 1  # minus background
            ids = labeled.astype(np.int32) - 1           # background -> -1, regions 0..cur-1
        except ImportError:
            from collections import deque
            ids = np.full((gh, gw), -1, dtype=np.int32)
            cur = 0
            land = is_land
            for sy in range(gh):
                for sx in range(gw):
                    if land[sy, sx] and ids[sy, sx] == -1:
                        q = deque([(sx, sy)]); ids[sy, sx] = cur
                        while q:
                            cx, cy = q.popleft()
                            for dx, dy in ((1,0),(-1,0),(0,1),(0,-1)):
                                nx, ny = cx+dx, cy+dy
                                if 0 <= nx < gw and 0 <= ny < gh and land[ny, nx] and ids[ny, nx] == -1:
                                    ids[ny, nx] = cur; q.append((nx, ny))
                        cur += 1
        _region_id_grid = ids
        _region_count = int(cur)
        # Record dominant color + centroid per region (for labels / claiming).
        # Vectorized: use ndimage on the 1..cur label image (ids+1, background=0).
        _region_colors = {}; _region_centroids = {}
        try:
            from scipy import ndimage as _ndi
            lab1 = ids + 1  # regions 1..cur, background 0
            idx = list(range(1, cur + 1))
            cents = _ndi.center_of_mass(np.ones_like(lab1), lab1, idx)   # (row, col) per region
            rmean = _ndi.mean(r, lab1, idx)
            gmean = _ndi.mean(g, lab1, idx)
            bmean = _ndi.mean(b, lab1, idx)
            for k, rid in enumerate(range(cur)):
                cy, cx = cents[k]
                _region_centroids[rid] = (int(cx), int(cy))
                _region_colors[rid] = (int(rmean[k]), int(gmean[k]), int(bmean[k]))
        except Exception:
            for rid in range(cur):
                ys, xs = np.where(ids == rid)
                if xs.size == 0: continue
                _region_centroids[rid] = (int(xs.mean()), int(ys.mean()))
                _region_colors[rid] = (int(np.median(r[ys, xs])), int(np.median(g[ys, xs])), int(np.median(b[ys, xs])))
    except Exception as _e:
        _region_id_grid = None
        _region_count = 0
        _region_colors = {}; _region_centroids = {}

def _region_at(wx, wy):
    """Return the region id at native world coordinates, or -1 if none."""
    if _region_id_grid is None: return -1
    gx = int(wx / _MAP_NATIVE_W * _REGION_GRID_W)
    gy = int(wy / _MAP_NATIVE_H * _REGION_GRID_H)
    if gx < 0 or gx >= _REGION_GRID_W or gy < 0 or gy >= _REGION_GRID_H: return -1
    rid = int(_region_id_grid[gy][gx])
    if rid >= 0: return rid
    # Exact pixel is a border/background: expand outward and return the nearest tile.
    # Radius scales with grid resolution so clicking on a black border still resolves.
    max_r = max(6, int(_REGION_GRID_W / 120))  # ~20 at 2400px grid
    for radius in range(1, max_r + 1):
        # Check only the perimeter of the current ring (avoids re-scanning inner cells)
        for dx in range(-radius, radius + 1):
            for dy in (-radius, radius):
                nx, ny = gx + dx, gy + dy
                if 0 <= nx < _REGION_GRID_W and 0 <= ny < _REGION_GRID_H:
                    v = int(_region_id_grid[ny][nx])
                    if v >= 0: return v
        for dy in range(-radius + 1, radius):
            for dx in (-radius, radius):
                nx, ny = gx + dx, gy + dy
                if 0 <= nx < _REGION_GRID_W and 0 <= ny < _REGION_GRID_H:
                    v = int(_region_id_grid[ny][nx])
                    if v >= 0: return v
    return -1

def _assign_cities_to_regions():
    """Assign each city a _region id based on its coordinates."""
    for c in cities:
        c["_region"] = _region_at(c["x"], c["y"])

_detect_regions()

# === Region Ownership Tint ===
# A region (territory tile) is "claimed" by the nation whose city sits inside it, and is
# tinted that nation's color on the overlay instead of gray. Native factions have no marked
# land, so their cities never claim a region (it stays gray/unclaimed).
_region_owner = {}             # region_id -> owning faction (non-native only)
_region_overlay_dirty = True   # rebuild flag: overlay is regenerated when ownership changes

def _compute_region_owners():
    """Map each region to the nation that owns a (non-native) city sitting in it."""
    owners = {}
    for c in cities:
        owner = c.get("owner")
        if owner in NATIVE_FACTIONS:      # natives don't claim territory
            continue
        rid = c.get("_region", -1)
        if rid is None or rid < 0:
            continue
        # Capitals win ties; otherwise first claim stands.
        if rid not in owners or c.get("is_capital"):
            owners[rid] = owner
    return owners

def mark_claims_dirty():
    """Call whenever city ownership changes so the region colouring refreshes."""
    global _region_overlay_dirty
    _region_overlay_dirty = True

def _blit_overlay_viewport(surf, return_cache=False):
    """Blit only the visible portion of a full-map overlay surface, scaled to zoom.
    If return_cache is True, also returns (scaled_surface, blit_x, blit_y) for caching."""
    if surf is None: return None
    W, H = get_screen_size()
    disp_w = _MAP_NATIVE_W * zoom
    disp_h = _MAP_NATIVE_H * zoom
    ov_w, ov_h = surf.get_width(), surf.get_height()
    src_per_disp_x = ov_w / disp_w
    src_per_disp_y = ov_h / disp_h
    vis_left = max(0, -cam_x)
    vis_top = max(0, -cam_y)
    vis_right = min(disp_w, W - cam_x)
    vis_bottom = min(disp_h, H - cam_y)
    if vis_right <= vis_left or vis_bottom <= vis_top: return None
    sx = int(vis_left * src_per_disp_x)
    sy = int(vis_top * src_per_disp_y)
    sw = max(1, int((vis_right - vis_left) * src_per_disp_x))
    sh = max(1, int((vis_bottom - vis_top) * src_per_disp_y))
    sw = min(sw, ov_w - sx); sh = min(sh, ov_h - sy)
    if sw <= 0 or sh <= 0: return None
    sub = surf.subsurface(pygame.Rect(sx, sy, sw, sh))
    dw = max(1, int(vis_right - vis_left))
    dh = max(1, int(vis_bottom - vis_top))
    scaled = pygame.transform.scale(sub, (dw, dh))
    bx, by = int(cam_x + vis_left), int(cam_y + vis_top)
    screen.blit(scaled, (bx, by))
    return (scaled, bx, by) if return_cache else None

_region_draw_cache = None       # (surface, blit_x, blit_y)
_region_draw_key = None         # (cam_x, cam_y, zoom, W, H, overlay_id)

def _draw_regions():
    """Draw the single territory overlay (nation-coloured owned regions, gray unclaimed,
    dark borders). The scaled slice is cached and reused while the view + overlay are
    unchanged, so we don't re-scale it every frame."""
    global _region_overlay_dirty, _region_draw_cache, _region_draw_key
    if _region_overlay_dirty:
        _build_region_overlay()
        _region_overlay_dirty = False
        _region_draw_key = None  # overlay changed -> invalidate scaled cache
    if _region_overlay is None: return
    W, H = get_screen_size()
    key = (cam_x, cam_y, round(zoom, 6), W, H, id(_region_overlay))
    if key == _region_draw_key and _region_draw_cache is not None:
        surf, bx, by = _region_draw_cache
        screen.blit(surf, (bx, by)); return
    result = _blit_overlay_viewport(_region_overlay, return_cache=True)
    if result is not None:
        _region_draw_cache = result; _region_draw_key = key

def _region_grid_to_world(gx, gy):
    """Convert a region-grid cell to native world coordinates."""
    wx = (gx + 0.5) / _REGION_GRID_W * _MAP_NATIVE_W
    wy = (gy + 0.5) / _REGION_GRID_H * _MAP_NATIVE_H
    return wx, wy

# Region adjacency graph: rid -> set of neighboring region ids. Built once, lazily.
_region_adjacency = None

def _build_region_adjacency():
    """Determine which regions border which. Two regions are adjacent if their tiles are
    near each other in the grid (touching, or separated only by the thin border line).
    Used to route roads region-by-region across the map."""
    global _region_adjacency
    if _region_adjacency is not None:
        return _region_adjacency
    _region_adjacency = {}
    if _region_id_grid is None:
        return _region_adjacency
    try:
        import numpy as np
        ids = _region_id_grid
        gh, gw = ids.shape
        adj = {}
        # Compare each cell to neighbors a few pixels away (to bridge the black border gap).
        gap = 4
        # right/down comparisons cover all pairs
        a = ids[:, :-gap]; b = ids[:, gap:]
        mask = (a >= 0) & (b >= 0) & (a != b)
        for ra, rb in zip(a[mask].tolist(), b[mask].tolist()):
            adj.setdefault(ra, set()).add(rb); adj.setdefault(rb, set()).add(ra)
        a = ids[:-gap, :]; b = ids[gap:, :]
        mask = (a >= 0) & (b >= 0) & (a != b)
        for ra, rb in zip(a[mask].tolist(), b[mask].tolist()):
            adj.setdefault(ra, set()).add(rb); adj.setdefault(rb, set()).add(ra)
        _region_adjacency = adj
    except Exception:
        _region_adjacency = {}
    return _region_adjacency

def _find_region_route(start_rid, goal_rid):
    """BFS over the region adjacency graph. Returns a list of region ids from start to goal
    (inclusive), or None if there's no land route between their territories."""
    if start_rid < 0 or goal_rid < 0:
        return None
    if start_rid == goal_rid:
        return [start_rid]
    adj = _build_region_adjacency()
    if not adj:
        return None
    from collections import deque
    q = deque([start_rid])
    prev = {start_rid: None}
    while q:
        cur = q.popleft()
        if cur == goal_rid:
            # reconstruct
            route = []
            n = goal_rid
            while n is not None:
                route.append(n); n = prev[n]
            return route[::-1]
        for nb in adj.get(cur, ()):
            if nb not in prev:
                prev[nb] = cur; q.append(nb)
    return None

# Cache of boundary pixels per region: rid -> numpy array of (gx, gy) grid coords on the edge.
_region_boundary_cache = {}

def _get_region_boundary(rid):
    """Return the boundary grid cells (edge pixels) of a region, computing once and caching.
    A cell is on the boundary if it belongs to the region but has at least one 4-neighbor
    that does not (border line, another region, or background)."""
    if rid in _region_boundary_cache:
        return _region_boundary_cache[rid]
    if _region_id_grid is None:
        return None
    try:
        import numpy as np
        ids = _region_id_grid
        ys, xs = np.where(ids == rid)
        if xs.size == 0:
            _region_boundary_cache[rid] = None
            return None
        # Work within the region's bounding box for speed.
        y0, y1 = ys.min(), ys.max() + 1
        x0, x1 = xs.min(), xs.max() + 1
        sub = (ids[y0:y1, x0:x1] == rid)
        # Pad so edges of the bbox count as boundary too.
        p = np.pad(sub, 1, mode="constant", constant_values=False)
        # A True cell is interior if all 4 neighbors are also True.
        interior = (p[1:-1, 1:-1] & p[:-2, 1:-1] & p[2:, 1:-1] & p[1:-1, :-2] & p[1:-1, 2:])
        edge = sub & ~interior
        eys, exs = np.where(edge)
        pts = np.stack([exs + x0, eys + y0], axis=1)  # (gx, gy)
        _region_boundary_cache[rid] = pts
        return pts
    except Exception:
        _region_boundary_cache[rid] = None
        return None

_region_panel_font = None
_region_panel_title_font = None

def _draw_region_panel():
    """Info panel for the currently clicked territory: id, tile color, biome, and the
    settlements that fall inside it. Drawn bottom-left so it doesn't cover the city panel."""
    global _region_panel_font, _region_panel_title_font
    if _selected_region < 0: return
    if _region_id_grid is None: return
    if _region_panel_font is None:
        _region_panel_font = pygame.font.SysFont("Arial", 14)
        _region_panel_title_font = pygame.font.SysFont("Arial", 17, bold=True)
    rid = _selected_region
    col = _region_colors.get(rid, (150, 150, 150))
    # Cities inside this region
    region_cities = [c for c in cities if c.get("_region", -1) == rid]
    # Biome at the region centroid (representative)
    cent = _region_centroids.get(rid)
    biome = "-"
    if cent:
        wx, wy = _region_grid_to_world(*cent)
        biome = _get_biome(wx, wy)

    W, H = get_screen_size()
    pad = 10
    line_h = 20
    lines = []
    lines.append(("Territory #%d" % rid, _region_panel_title_font, (255, 255, 255)))
    lines.append(("Tile color: rgb(%d, %d, %d)" % col, _region_panel_font, (220, 220, 220)))
    lines.append(("Biome: %s" % biome, _region_panel_font, (220, 220, 220)))
    lines.append(("Settlements: %d" % len(region_cities), _region_panel_font, (220, 220, 220)))
    for c in region_cities[:8]:
        owner = c.get("owner", "?")
        lines.append(("  - %s (%s)" % (c["name"], owner), _region_panel_font, (180, 210, 255)))
    if len(region_cities) > 8:
        lines.append(("  ...and %d more" % (len(region_cities) - 8), _region_panel_font, (170, 170, 170)))
    lines.append(("(click empty tile to close)", _region_panel_font, (150, 150, 150)))

    pw = 300
    ph = pad * 2 + 24 + len(lines) * line_h  # 24 for color swatch row
    px = 12
    py = H - ph - 12
    # Panel background
    bg = pygame.Surface((pw, ph), pygame.SRCALPHA)
    bg.fill((20, 20, 35, 225))
    screen.blit(bg, (px, py))
    pygame.draw.rect(screen, (120, 100, 50), (px, py, pw, ph), 2, border_radius=4)
    # Color swatch showing the tile's actual color
    sw_x, sw_y = px + pad, py + pad + 4
    pygame.draw.rect(screen, col, (sw_x, sw_y, 20, 16))
    pygame.draw.rect(screen, (255, 255, 255), (sw_x, sw_y, 20, 16), 1)
    # Text
    ty = py + pad
    for i, (text, fnt, tcol) in enumerate(lines):
        # First line (title) sits to the right of the swatch
        x_off = px + pad + (28 if i == 0 else 0)
        screen.blit(fnt.render(text, True, tcol), (x_off, ty))
        ty += line_h if i != 0 else 24

def _highlight_selected_region():
    """Highlight the selected territory by tracing its border shape on the map."""
    if _selected_region < 0 or _region_id_grid is None: return
    pts = _get_region_boundary(_selected_region)
    if pts is None or len(pts) == 0: return
    W, H = get_screen_size()
    # World size of one grid cell, scaled to screen. Draw each edge cell as a small square
    # so the outline reads as a continuous glowing border regardless of zoom.
    cell_w = (_MAP_NATIVE_W / _REGION_GRID_W) * zoom
    cell_h = (_MAP_NATIVE_H / _REGION_GRID_H) * zoom
    sz = max(2, int(max(cell_w, cell_h)) + 1)
    off = sz // 2
    col = (255, 255, 0)
    inv_w = _MAP_NATIVE_W / _REGION_GRID_W
    inv_h = _MAP_NATIVE_H / _REGION_GRID_H
    # Stride: when zoomed far out, cells map to sub-pixel, so skip some to save draw calls.
    stride = 1
    if cell_w < 1.0:
        stride = max(1, int(1.0 / cell_w))
    for i in range(0, len(pts), stride):
        gx, gy = pts[i]
        wx = (gx + 0.5) * inv_w
        wy = (gy + 0.5) * inv_h
        sx = int(wx * zoom + cam_x)
        sy = int(wy * zoom + cam_y)
        if sx < -sz or sx > W + sz or sy < -sz or sy > H + sz:
            continue
        screen.fill(col, (sx - off, sy - off, sz, sz))

# === Biome System ===
# Biome colors (RGB) from climates.png
_BIOME_COLORS = {
    "taiga": (0x09, 0x61, 0x1c),       # 09611c
    "tundra": (0x98, 0xda, 0xeb),       # 98daeb
    "grassland": (0xfe, 0xff, 0x00),    # feff00 (exact from climates.png)
    "desert": (0xff, 0x3a, 0x00),       # ff3a00
    "temperate": (0x0f, 0xc9, 0x38),    # 0fc938
    "swamp": (0x81, 0x81, 0x81),        # 818181
    "tropical": (0xa3, 0x00, 0xd0),     # a300d0 (exact from climates.png)
    "mountains": (0x52, 0x3f, 0x10),    # 523f10 dark brown (exact from climates.png)
    "rainforest": (0xf9, 0x1e, 0x59),   # f91e59 pink/red (exact from climates.png)
}

def _get_biome(wx, wy):
    """Get biome at world coordinates by sampling climates.png pixel color.
    Colors are matched to the exact palette used in climates.png (nearest color within
    tolerance), so every painted biome — including mountains (523f10) and rainforest
    (f91e59) — is classified correctly."""
    ix, iy = int(wx), int(wy)
    if ix < 0 or ix >= _MAP_NATIVE_W or iy < 0 or iy >= _MAP_NATIVE_H:
        return "temperate"
    r, g, b = _climate_map.get_at((ix, iy))[:3]
    # Ocean/background is strong blue (#0400ff) — decide biome by latitude.
    if b > 200 and r < 40 and g < 40:
        if iy < 200: return "tundra"      # arctic/northern ocean
        elif iy > 350: return "tropical"   # Caribbean/tropical/southern ocean (incl. South America)
        else: return "temperate"           # mid-latitude ocean
    # Nearest exact-palette color (generous tolerance handles JPEG/resize drift).
    best, best_dist = "temperate", 10**9
    for name, (cr, cg, cb) in _BIOME_COLORS.items():
        dist = abs(r-cr) + abs(g-cg) + abs(b-cb)
        if dist < best_dist:
            best, best_dist = name, dist
    return best if best_dist < 120 else "temperate"

# Biome modifiers: (move_mult, food_mult, lumber_mult, hide_mult, gold_mult, defense_mult)
_BIOME_MODIFIERS = {
    "temperate": (1.0, 1.0, 1.0, 1.0, 1.0, 1.0),
    "taiga":     (0.75, 0.75, 2.0, 2.0, 1.0, 1.25),
    "tundra":    (0.60, 0.50, 0.0, 2.0, 1.0, 1.0),
    "grassland": (1.25, 1.50, 0.50, 1.50, 1.0, 1.0),
    "desert":    (0.75, 0.50, 1.0, 1.25, 1.50, 1.0),
    "swamp":     (0.50, 1.25, 1.0, 1.0, 1.0, 1.50),
    "tropical":  (0.50, 0.75, 1.50, 1.50, 1.0, 1.0),
    "mountains": (0.40, 0.40, 1.0, 1.50, 1.75, 2.0),
    "rainforest":(0.45, 0.85, 2.0, 1.75, 1.0, 1.25),
}

def _get_biome_move_mult(wx, wy, is_cavalry=False):
    """Get movement multiplier at position. Cavalry nerfed in swamp/taiga/temperate/tundra."""
    biome = _get_biome(wx, wy)
    mult = _BIOME_MODIFIERS[biome][0]
    if is_cavalry:
        if biome == "swamp": mult = 0.25  # -75% for cavalry in swamp
        elif biome == "tundra": mult *= 0.50  # -50% in tundra
        elif biome in ("taiga", "temperate"): mult *= 0.75  # -25% in taiga/temperate
    return mult

def _get_biome_defense_mult(wx, wy):
    """Get defense multiplier at position."""
    biome = _get_biome(wx, wy)
    return _BIOME_MODIFIERS[biome][5]

# === Collision & Pathfinding (from collision.py) ===
import collision
collision.init(_collision_map)
_is_water = collision.is_water
_is_land = collision.is_land
_is_port = collision.is_port
_path_needs_boat = collision.path_needs_boat
_nav_find_path = collision.find_water_path
_nav_find_land_path = collision.find_land_path
_deflect_from_water = collision.deflect_from_water
_build_nav_grid = collision.build_nav_grid
_ISLAND_CITY_NAMES = collision.ISLAND_CITY_NAMES
_TINY_ISLAND_NAMES = collision.TINY_ISLAND_NAMES
PORT_CITIES = collision.PORT_CITIES
# ====================================================================
# === CAMERA & DISPLAY UTILITIES =====================================
# ====================================================================
def get_screen_size(): return screen.get_size()
_map_cache = None       # (surface, blit_x, blit_y)
_map_cache_key = None   # (cam_x, cam_y, zoom, W, H) the cache was built for

def draw_map():
    """Draw only the visible portion of the full-res map, scaled to zoom. The scaled result
    is cached and reused while the view (camera/zoom/screen) hasn't changed, so we don't
    re-scale the 43MP source every frame — that was the main source of choppiness."""
    global _map_cache, _map_cache_key
    W, H = get_screen_size()
    key = (cam_x, cam_y, round(zoom, 6), W, H)
    if key == _map_cache_key and _map_cache is not None:
        surf, bx, by = _map_cache
        screen.blit(surf, (bx, by))
        return
    # Display dimensions of the full map at current zoom
    disp_w = _MAP_NATIVE_W * zoom
    disp_h = _MAP_NATIVE_H * zoom
    # Pick a map copy whose resolution matches the on-screen size (acceleration: avoids
    # downscaling from the full 43MP source when zoomed out).
    src_surf, msw, msh = _pick_map_mip(disp_w)
    src_per_disp_x = msw / disp_w
    src_per_disp_y = msh / disp_h
    vis_left = max(0, -cam_x)
    vis_top = max(0, -cam_y)
    vis_right = min(disp_w, W - cam_x)
    vis_bottom = min(disp_h, H - cam_y)
    if vis_right <= vis_left or vis_bottom <= vis_top:
        _map_cache = None; _map_cache_key = None; return
    sx = int(vis_left * src_per_disp_x)
    sy = int(vis_top * src_per_disp_y)
    sw = max(1, int((vis_right - vis_left) * src_per_disp_x))
    sh = max(1, int((vis_bottom - vis_top) * src_per_disp_y))
    sw = min(sw, msw - sx); sh = min(sh, msh - sy)
    if sw <= 0 or sh <= 0:
        _map_cache = None; _map_cache_key = None; return
    sub = src_surf.subsurface(pygame.Rect(sx, sy, sw, sh))
    dw = max(1, int(vis_right - vis_left))
    dh = max(1, int(vis_bottom - vis_top))
    scaled = pygame.transform.scale(sub, (dw, dh))
    bx, by = int(cam_x + vis_left), int(cam_y + vis_top)
    screen.blit(scaled, (bx, by))
    _map_cache = (scaled, bx, by); _map_cache_key = key
def clamp_camera():
    global cam_x, cam_y
    W, H = get_screen_size(); sw, sh = int(_MAP_NATIVE_W*zoom), int(_MAP_NATIVE_H*zoom)
    # Center map if smaller than screen, otherwise allow panning
    if sw <= W:
        cam_x = (W - sw) // 2
    else:
        cam_x = max(W-sw, min(0, cam_x))
    if sh <= H:
        cam_y = (H - sh) // 2
    else:
        cam_y = max(H-sh, min(0, cam_y))
def toggle_fullscreen():
    global fullscreen, screen, zoom, cam_x, cam_y, _map_cache_key, _region_draw_key
    fullscreen = not fullscreen
    if fullscreen:
        screen = _make_display((SCREEN_W, SCREEN_H), pygame.FULLSCREEN | pygame.SCALED | pygame.DOUBLEBUF)
    else:
        screen = _make_display((_DISPLAY_W, _DISPLAY_H), pygame.SCALED | pygame.DOUBLEBUF)
    zoom = _scale_factor
    _map_cache_key = None; _region_draw_key = None  # screen changed -> invalidate scaled caches
    clamp_camera()
def world_to_screen(wx, wy): return int(wx*zoom+cam_x), int(wy*zoom+cam_y)
def _on_screen(sx, sy, margin=48):
    """True if screen coords (sx,sy) are within the viewport (plus a margin for sprite size).
    Used to skip DRAWING off-screen entities; their logical positions still update in memory."""
    W, H = get_screen_size()
    return -margin <= sx <= W + margin and -margin <= sy <= H + margin

# ====================================================================
# === FACTION DEFINITIONS & ELIMINATION ==============================
# ====================================================================
FACTION_COLORS = {"France":(0,100,255),"Spain":(255,255,0),"Great Britain":(255,0,0),"Russia":(0,120,0),"Denmark":(0,255,0),"Rupert's Land":(200,120,60),"Iroquois":(150,50,200),"Wabanaki":(128,0,0),"Comanche":(180,120,40),"Cree":(60,180,130),"Dakota":(180,160,80),"Pirates":(20,20,20)}
FACTIONS = list(FACTION_COLORS); NATIVE_FACTIONS = {"Iroquois","Wabanaki","Comanche","Cree","Dakota"}; eliminated_factions = set(); _surrendered = set()
game_state.FACTIONS = FACTIONS; game_state.eliminated_factions = eliminated_factions
def check_eliminations():
    for f in FACTIONS[:]:
        if f in eliminated_factions: continue
        # Skip pirates if they haven't spawned yet
        if f == "Pirates" and not _pirates_have_spawned: continue
        if sum(1 for c in cities if c["owner"]==f) == 0:
            if f not in _surrendered:
                victors = [v for v in active_factions() if v != f and at_war(f, v)]
                if victors:
                    _surrendered.add(f)
                    news(f"{f} has lost all cities — forced to surrender!")
                    for v in victors: make_treaty(v, f, f"{v} dictates peace terms to defeated {f}.")
                    units[:] = [u for u in units if u.owner["owner"] != f]
                    merchants[:] = [m for m in merchants if m.owner_faction != f]
                    settlers[:] = [s for s in settlers if s.owner_faction != f]
            if sum(1 for c in cities if c["owner"]==f)==0 and sum(1 for c in cities if c["sovereign"]==f and c["owner"]!=f)==0:
                eliminated_factions.add(f)
                for key in list(treaties):
                    if f in key: del treaties[key]
                for key in list(alliances):
                    if f in key: del alliances[key]
                news(f"{f} has been eliminated from the game!")
                show_newspaper(f"{f} Eliminated!", f"The nation of {f} has been completely destroyed. All their cities have been conquered and their people scattered.")
                # Demote all their former capitals to normal cities
                for c in cities:
                    if c.get("sovereign")==f or c.get("_orig_owner")==f:
                        c["is_capital"] = False
        elif f in _surrendered:
            _surrendered.discard(f)
def active_factions(): return [f for f in FACTIONS if f not in eliminated_factions]

# ====================================================================
# === RENDERING UTILITIES ============================================
# ====================================================================
def draw_outlined_text(font_obj, text_str, color, pos, anchor="topleft"):
    # Use white outline for very dark text colors so they remain readable
    outline_color = (255,255,255) if (color[0]+color[1]+color[2]) < 100 else (0,0,0)
    outline_surf = font_obj.render(text_str, True, outline_color)
    main_surf = font_obj.render(text_str, True, color)
    base_rect = main_surf.get_rect(center=pos) if anchor=="center" else main_surf.get_rect(topleft=pos)
    for ox, oy in [(-1,-1),(-1,0),(-1,1),(0,-1),(0,1),(1,-1),(1,0),(1,1)]:
        r = outline_surf.get_rect(); r.topleft = (base_rect.x+ox, base_rect.y+oy); screen.blit(outline_surf, r)
    screen.blit(main_surf, base_rect)

def _draw_panel_frame(x, y, w, h):
    """Draw the gold double-border frame around a panel (matching the date/time frame style)."""
    pygame.draw.rect(screen, (120, 100, 50), (x, y, w, h), 2, border_radius=4)
    pygame.draw.rect(screen, (180, 150, 60), (x+1, y+1, w-2, h-2), 1, border_radius=4)

# === Sprites (from sprites.py) ===
import sprites
sprites.init()

# Pre-load sprites for nations that appear via events (for Codex)
try: sprites.infantry_anims["Mexico"] = sprites._load_infantry_sheet("images/sprites/mexinf.png")
except: pass
try: sprites.cavalry_anims["Mexico"] = sprites._load_cavalry_sheet("images/sprites/mexcav.png")
except: pass
try: sprites.infantry_anims["Haiti"] = sprites._load_infantry_sheet("images/sprites/haitinf.png")
except: pass
try: sprites.cavalry_anims["Haiti"] = sprites._load_cavalry_sheet("images/sprites/haiticav.png")
except: pass
try: sprites.infantry_anims["Texas"] = sprites._load_infantry_sheet("images/sprites/texinf.png")
except: pass
try: sprites.cavalry_anims["Texas"] = sprites._load_cavalry_sheet("images/sprites/texcav.png")
except: pass
try: sprites.infantry_anims["Confederate States"] = sprites._load_infantry_sheet("images/sprites/confedinf.png")
except: pass
try: sprites.cavalry_anims["Confederate States"] = sprites._load_cavalry_sheet("images/sprites/confedcav.png")
except: pass

# === Biome & Capital City Images (for city click panel) ===
_BIOME_IMAGES = {
    "desert": ["images/biomes/desert.png", "images/biomes/desert2.png", "images/biomes/desert3.png"],
    "grassland": ["images/biomes/grassland.jpg", "images/biomes/grassland2.jpeg", "images/biomes/grassland3.webp"],
    "swamp": ["images/biomes/swamp.jpg", "images/biomes/swamp2.jpg", "images/biomes/swamp3.webp"],
    "taiga": ["images/biomes/taiga.png", "images/biomes/taiga2.jpg", "images/biomes/taiga3.jpg"],
    "temperate": ["images/biomes/temperate.png", "images/biomes/temperate2.jpg", "images/biomes/temperate3.webp"],
    "tropical": ["images/biomes/tropics.jpg", "images/biomes/tropics2.jpg", "images/biomes/tropics3.jpg"],
    "tundra": ["images/biomes/tundra.jpg", "images/biomes/tundra2.png", "images/biomes/tundra3.jpg"],
}
_CAPITAL_IMAGES = {
    "Mexico City": "images/biomes/mexicocity.jpg",
    "Quebec": "images/biomes/quebec.png",
    "Boston": "images/biomes/boston.png",
}
_biome_img_cache = {}  # cache loaded/scaled images

# Pre-load building overlay images so cities display correctly from start
def _preload_building_overlays():
    _bldg_files = {
        "_woodfort_overlay": ("images/buildings/woodfort.png", (150, 113)),
        "_nativecamp_overlay": ("images/buildings/nativecamp.png", (113, 85)),
        "_britcity_overlay": ("images/buildings/britcity.png", (113, 85)),
        "_britcity2_overlay": ("images/buildings/britcity2.png", (113, 85)),
        "_britcity3_overlay": ("images/buildings/britcity3.png", (113, 85)),
        "_spancity_overlay": ("images/buildings/spancity.png", (113, 85)),
        "_spancity2_overlay": ("images/buildings/spancity2.png", (113, 85)),
        "_spancity3_overlay": ("images/buildings/spancity3.png", (113, 85)),
        "_frencity_overlay": ("images/buildings/frencity.png", (113, 85)),
        "_frencity2_overlay": ("images/buildings/frencity2.png", (113, 85)),
        "_frencity3_overlay": ("images/buildings/frencity3.png", (113, 85)),
        "_ruscity_overlay": ("images/buildings/ruscity.png", (113, 85)),
        "_dancity_overlay": ("images/buildings/dancity.png", (113, 85)),
    }
    for key, (path, size) in _bldg_files.items():
        try:
            surf = pygame.image.load(path).convert_alpha()
            _biome_img_cache[key] = pygame.transform.scale(surf, size)
        except:
            pass
_preload_building_overlays()

def _get_city_image(city):
    """Get the display image for a city panel. Capitals get unique images, others get biome-based."""
    name = city["name"]
    if name in _CAPITAL_IMAGES:
        key = name
        if key not in _biome_img_cache:
            _biome_img_cache[key] = pygame.transform.scale(pygame.image.load(_CAPITAL_IMAGES[name]), (250, 180))
        return _biome_img_cache[key]
    biome = _get_biome(city["x"], city["y"])
    # Pick a consistent random image for this city (based on city name hash)
    imgs = _BIOME_IMAGES.get(biome, _BIOME_IMAGES["temperate"])
    idx = hash(name) % len(imgs)
    key = imgs[idx]
    if key not in _biome_img_cache:
        _biome_img_cache[key] = pygame.transform.scale(pygame.image.load(key), (250, 180))
    base_img = _biome_img_cache[key].copy()
    # Overlay woodfort on top of biome for colonial forts
    if city.get("is_fort") and city.get("sovereign", city["owner"]) not in NATIVE_FACTIONS:
        fort_key = "_woodfort_overlay"
        if fort_key not in _biome_img_cache:
            fort_surf = pygame.image.load("images/buildings/woodfort.png").convert_alpha()
            _biome_img_cache[fort_key] = pygame.transform.scale(fort_surf, (150, 113))
        fort_img = _biome_img_cache[fort_key]
        # Place fort centered horizontally, near bottom
        fx = (250 - 150) // 2
        fy = 180 - 113 - 15
        base_img.blit(fort_img, (fx, fy))
    # Overlay nativecamp on top of biome for native camps and villages (only if still native-owned)
    elif (city.get("is_camp") or city.get("is_village")) and city.get("_original_sovereign", city.get("sovereign", city["owner"])) in NATIVE_FACTIONS and city["owner"] in NATIVE_FACTIONS:
        camp_key = "_nativecamp_overlay"
        if camp_key not in _biome_img_cache:
            camp_surf = pygame.image.load("images/buildings/nativecamp.png").convert_alpha()
            _biome_img_cache[camp_key] = pygame.transform.scale(camp_surf, (113, 85))
        camp_img = _biome_img_cache[camp_key]
        fx = (250 - 113) // 2
        fy = 180 - 85 - 15
        base_img.blit(camp_img, (fx, fy))
    # Overlay britcity on top of biome for British/American/Texan cities (sovereign)
    elif not city.get("is_fort") and not city.get("is_camp") and not city.get("is_village") and city.get("_original_sovereign", city.get("sovereign")) in ("Great Britain", "United States", "Texas", "Confederate States", "Canada", "Rupert's Land") and city["name"] not in _CAPITAL_IMAGES:
        tier = city.get("tier", 1)
        if tier >= 3: brit_key = "_britcity3_overlay"
        elif tier >= 2: brit_key = "_britcity2_overlay"
        else: brit_key = "_britcity_overlay"
        if brit_key not in _biome_img_cache:
            brit_surf = pygame.image.load(f"images/buildings/britcity{'3' if tier>=3 else ('2' if tier>=2 else '')}.png").convert_alpha()
            _biome_img_cache[brit_key] = pygame.transform.scale(brit_surf, (113, 85))
        brit_img = _biome_img_cache[brit_key]
        fx = (250 - 113) // 2
        fy = 180 - 85 + 5
        base_img.blit(brit_img, (fx, fy))
    # Overlay spancity on top of biome for Spanish/Mexican cities (sovereign)
    elif not city.get("is_fort") and not city.get("is_camp") and not city.get("is_village") and city.get("_original_sovereign", city.get("sovereign")) in ("Spain", "Mexico") and city["name"] not in _CAPITAL_IMAGES:
        tier = city.get("tier", 1)
        if tier >= 3: span_key = "_spancity3_overlay"
        elif tier >= 2: span_key = "_spancity2_overlay"
        else: span_key = "_spancity_overlay"
        if span_key not in _biome_img_cache:
            span_surf = pygame.image.load(f"images/buildings/spancity{'3' if tier>=3 else ('2' if tier>=2 else '')}.png").convert_alpha()
            _biome_img_cache[span_key] = pygame.transform.scale(span_surf, (113, 85))
        span_img = _biome_img_cache[span_key]
        fx = (250 - 113) // 2
        fy = 180 - 85 + 5
        base_img.blit(span_img, (fx, fy))
    # Overlay frencity on top of biome for French/Haitian cities (sovereign)
    elif not city.get("is_fort") and not city.get("is_camp") and not city.get("is_village") and city.get("_original_sovereign", city.get("sovereign")) in ("France", "Haiti") and city["name"] not in _CAPITAL_IMAGES:
        tier = city.get("tier", 1)
        if tier >= 3: fren_key = "_frencity3_overlay"
        elif tier >= 2: fren_key = "_frencity2_overlay"
        else: fren_key = "_frencity_overlay"
        if fren_key not in _biome_img_cache:
            fren_surf = pygame.image.load(f"images/buildings/frencity{'3' if tier>=3 else ('2' if tier>=2 else '')}.png").convert_alpha()
            _biome_img_cache[fren_key] = pygame.transform.scale(fren_surf, (113, 85))
        fren_img = _biome_img_cache[fren_key]
        fx = (250 - 113) // 2
        fy = 180 - 85 + 5
        base_img.blit(fren_img, (fx, fy))
    # Overlay ruscity on top of biome for Russian cities (sovereign, including capital)
    elif not city.get("is_fort") and not city.get("is_camp") and not city.get("is_village") and city.get("_original_sovereign", city.get("sovereign")) == "Russia":
        rus_key = "_ruscity_overlay"
        if rus_key not in _biome_img_cache:
            rus_surf = pygame.image.load("images/buildings/ruscity.png").convert_alpha()
            _biome_img_cache[rus_key] = pygame.transform.scale(rus_surf, (113, 85))
        rus_img = _biome_img_cache[rus_key]
        fx = (250 - 113) // 2
        fy = 180 - 85 + 5
        base_img.blit(rus_img, (fx, fy))
    # Overlay dancity on top of biome for Danish cities (sovereign, including capital)
    elif not city.get("is_fort") and not city.get("is_camp") and not city.get("is_village") and city.get("_original_sovereign", city.get("sovereign")) == "Denmark":
        dan_key = "_dancity_overlay"
        if dan_key not in _biome_img_cache:
            dan_surf = pygame.image.load("images/buildings/dancity.png").convert_alpha()
            _biome_img_cache[dan_key] = pygame.transform.scale(dan_surf, (113, 85))
        dan_img = _biome_img_cache[dan_key]
        fx = (250 - 113) // 2
        fy = 180 - 85 + 5
        base_img.blit(dan_img, (fx, fy))
    return base_img

# === Mini Merchant Bouncing Sprites (for city panel) ===
_panel_merchants = []  # list of {"x", "y", "dx", "dy", "anim_tick", "native", "haiti"}
_panel_city_id = None  # track which city spawned current merchants
_panel_merch_frames_r = []  # walk right frames for colonial
_panel_merch_frames_l = []  # walk left frames for colonial
_panel_natmerch_frames_r = []  # walk right frames for native
_panel_natmerch_frames_l = []  # walk left frames for native
_panel_haiti_frames_r = []  # walk right frames for Haiti
_panel_haiti_frames_l = []  # walk left frames for Haiti

def _init_panel_merchants(city):
    """Spawn 1-5 mini merchants bouncing in the image area."""
    global _panel_merchants, _panel_city_id
    global _panel_merch_frames_r, _panel_merch_frames_l
    global _panel_natmerch_frames_r, _panel_natmerch_frames_l
    global _panel_haiti_frames_r, _panel_haiti_frames_l
    _panel_city_id = id(city)
    is_native = city["owner"] in NATIVE_FACTIONS
    is_haiti = city["owner"] == "Haiti"
    count = random.randint(1, 5)
    _panel_merchants = []
    for _ in range(count):
        _panel_merchants.append({
            "x": random.uniform(10, 210),
            "y": random.uniform(135, 155),  # bottom 25% of 180px image
            "dx": random.choice([-1, 1]) * random.uniform(0.5, 1.2),
            "dy": random.choice([-1, 1]) * random.uniform(0.2, 0.5),
            "anim_tick": random.randint(0, 100),
            "native": is_native,
            "haiti": is_haiti,
        })
    # Load walking animation frames (row 0 = walk right, row 1 = walk left, 2 frames each, 20x40)
    if not _panel_merch_frames_r:
        try:
            sheet = pygame.image.load("images/sprites/merchant.png").convert_alpha()
            _panel_merch_frames_r = [sheet.subsurface(pygame.Rect(i*20, 0, 20, 40)) for i in range(2)]
            _panel_merch_frames_l = [sheet.subsurface(pygame.Rect(i*20, 40, 20, 40)) for i in range(2)]
        except:
            s = pygame.Surface((20, 40), pygame.SRCALPHA)
            _panel_merch_frames_r = [s]; _panel_merch_frames_l = [s]
    if not _panel_natmerch_frames_r:
        try:
            sheet = pygame.image.load("images/sprites/natmerchant.png").convert_alpha()
            _panel_natmerch_frames_r = [sheet.subsurface(pygame.Rect(i*20, 0, 20, 40)) for i in range(2)]
            _panel_natmerch_frames_l = [sheet.subsurface(pygame.Rect(i*20, 40, 20, 40)) for i in range(2)]
        except:
            s = pygame.Surface((20, 40), pygame.SRCALPHA)
            _panel_natmerch_frames_r = [s]; _panel_natmerch_frames_l = [s]
    if not _panel_haiti_frames_r:
        try:
            sheet = pygame.image.load("images/sprites/blackmerchant.png").convert_alpha()
            _panel_haiti_frames_r = [sheet.subsurface(pygame.Rect(i*20, 0, 20, 40)) for i in range(2)]
            _panel_haiti_frames_l = [sheet.subsurface(pygame.Rect(i*20, 40, 20, 40)) for i in range(2)]
        except:
            s = pygame.Surface((20, 40), pygame.SRCALPHA)
            _panel_haiti_frames_r = [s]; _panel_haiti_frames_l = [s]

def _update_panel_merchants():
    """Update bouncing positions (DVD-style bounce off walls). Bottom 25% only."""
    # Bottom 25% of 180px image = y 135 to 180, minus sprite height (40 scaled to 40 at 2x = 40... display is 40px)
    min_y, max_y = 135, 168
    min_x, max_x = 5, 220
    for m in _panel_merchants:
        m["x"] += m["dx"]
        m["y"] += m["dy"]
        m["anim_tick"] += 1
        if m["x"] <= min_x or m["x"] >= max_x:
            m["dx"] *= -1; m["x"] = max(min_x, min(max_x, m["x"]))
        if m["y"] <= min_y or m["y"] >= max_y:
            m["dy"] *= -1; m["y"] = max(min_y, min(max_y, m["y"]))

def _draw_panel_merchants(img_x, img_y):
    """Draw mini merchants on top of the city image with walking animations, doubled size."""
    for m in _panel_merchants:
        # Pick frame based on direction and animation tick
        if m.get("haiti"):
            frames = _panel_haiti_frames_r if m["dx"] > 0 else _panel_haiti_frames_l
        elif m["native"]:
            frames = _panel_natmerch_frames_r if m["dx"] > 0 else _panel_natmerch_frames_l
        else:
            frames = _panel_merch_frames_r if m["dx"] > 0 else _panel_merch_frames_l
        if frames:
            idx = (m["anim_tick"] // 10) % len(frames)
            frame = frames[idx]
            # Double size: 20x40 -> 40x80... but that's too big for the panel. Scale to 20x40 display (2x original 10x20)
            scaled = pygame.transform.scale(frame, (20, 40))
            screen.blit(scaled, (img_x + int(m["x"]), img_y + int(m["y"]) - 20))

# === Fire Effect for Damaged Settlements ===
_fire_frames = []
_fire_positions = []  # random positions for fire sprites
_fire_city_id = None

def _init_fire():
    """Load fire.gif frames."""
    global _fire_frames
    if _fire_frames: return
    try:
        from PIL import Image as PILImage
        pil_img = PILImage.open("images/fire.gif")
        try:
            while True:
                frame = pil_img.convert("RGBA").resize((24, 32), PILImage.LANCZOS)
                _fire_frames.append(pygame.image.fromstring(frame.tobytes(), (24, 32), "RGBA"))
                pil_img.seek(pil_img.tell() + 1)
        except EOFError:
            pass
    except ImportError:
        _fire_frames.append(pygame.transform.scale(pygame.image.load("images/fire.gif"), (24, 32)))
    if not _fire_frames:
        _fire_frames.append(pygame.Surface((24, 32), pygame.SRCALPHA))

def _init_fire_positions(city):
    """Generate random fire positions for a damaged city."""
    global _fire_positions, _fire_city_id
    _fire_city_id = id(city)
    _fire_positions = []
    count = random.randint(3, 6)
    for _ in range(count):
        _fire_positions.append({
            "x": random.randint(20, 220),
            "y": random.randint(120, 165),  # bottom area (merchant ground level)
        })

def _draw_panel_fires(img_x, img_y, city):
    """Draw animated fire on the city image when settlement is below 25% health."""
    # Calculate max health
    if city.get("is_camp"): base = 30
    elif city.get("is_village"): base = 50
    elif city["is_capital"]: base = 200
    else: base = 100
    max_hp = base + city.get("tier", 0) * 25
    # Only show fires below 25% health
    if city["troops"] > max_hp * 0.25: return
    _init_fire()
    if _fire_city_id != id(city):
        _init_fire_positions(city)
    if not _fire_frames: return
    frame_idx = (_brit_anim_tick // 4) % len(_fire_frames)
    fire_frame = _fire_frames[frame_idx]
    for fp in _fire_positions:
        screen.blit(fire_frame, (img_x + fp["x"], img_y + fp["y"]))

_selected_city = None  # currently clicked/open city panel
_selected_region = -1  # currently clicked territory/region id (-1 = none)

# === Governor System ===
_governor_sprites = []  # 4 governor types, each with idle + 3 move frames
_governor_haiti_sprite = None  # Separate sprite for Haitian governors
_governor_chief_sprites = []  # Native chief sprites (natchief1.png, natchief2.png)
_governor_bg = None
_governor_frame = None

def _init_governor_sprites():
    """Load governor portrait sprites: 4 separate files, each 20 wide. 
    Row 1: idle (1 frame, 20x30). Row 2: 3 turning frames side by side (60x30 total)."""
    global _governor_sprites, _governor_haiti_sprite, _governor_chief_sprites, _governor_bg, _governor_frame
    if _governor_sprites: return
    for i in range(1, 5):
        path = f"images/sprites/govenor{i}.png"
        try:
            sheet = pygame.image.load(path).convert_alpha()
            idle = sheet.subsurface(pygame.Rect(0, 0, 20, 30))
            # Row 2: 3 turning frames at 20x30 each, side by side
            turn_frames = []
            for col in range(3):
                x = col * 20
                if x + 20 <= sheet.get_width() and 60 <= sheet.get_height():
                    turn_frames.append(sheet.subsurface(pygame.Rect(x, 30, 20, 30)))
            if not turn_frames: turn_frames = [idle]
            _governor_sprites.append({"idle": idle, "turn": turn_frames})
        except:
            s = pygame.Surface((20, 30), pygame.SRCALPHA)
            _governor_sprites.append({"idle": s, "turn": [s]})
    # Load Haitian governor sprite (same format: 20x30 idle top, 60x30 turn bottom)
    try:
        sheet = pygame.image.load("images/sprites/blackgovenor.png").convert_alpha()
        idle = sheet.subsurface(pygame.Rect(0, 0, 20, 30))
        turn_frames = []
        for col in range(3):
            x = col * 20
            if x + 20 <= sheet.get_width() and 60 <= sheet.get_height():
                turn_frames.append(sheet.subsurface(pygame.Rect(x, 30, 20, 30)))
        if not turn_frames: turn_frames = [idle]
        _governor_haiti_sprite = {"idle": idle, "turn": turn_frames}
    except:
        _governor_haiti_sprite = None
    # Load native chief sprites (natchief1.png, natchief2.png — same format)
    for i in range(1, 3):
        path = f"images/sprites/natchief{i}.png"
        try:
            sheet = pygame.image.load(path).convert_alpha()
            idle = sheet.subsurface(pygame.Rect(0, 0, 20, 30))
            turn_frames = []
            for col in range(3):
                x = col * 20
                if x + 20 <= sheet.get_width() and 60 <= sheet.get_height():
                    turn_frames.append(sheet.subsurface(pygame.Rect(x, 30, 20, 30)))
            if not turn_frames: turn_frames = [idle]
            _governor_chief_sprites.append({"idle": idle, "turn": turn_frames})
        except:
            s = pygame.Surface((20, 30), pygame.SRCALPHA)
            _governor_chief_sprites.append({"idle": s, "turn": [s]})
    try: _governor_bg = pygame.image.load("images/portraitbackground.png").convert_alpha()
    except: _governor_bg = pygame.Surface((80, 100), pygame.SRCALPHA)
    try: _governor_frame = pygame.image.load("images/frame.webp").convert_alpha()
    except: _governor_frame = pygame.Surface((90, 110), pygame.SRCALPHA)

# === Governor Traits ===
GOVERNOR_TRAITS = {
    "Charismatic": {"desc": "+2 happiness", "color": (100, 255, 100)},
    "Economist": {"desc": "+2 gold production", "color": (255, 220, 50)},
    "Administrator": {"desc": "-250 gold off construction", "color": (100, 200, 255)},
    "Architect": {"desc": "25% less construction time", "color": (180, 150, 255)},
    "Agrarian": {"desc": "Boosts food production", "color": (50, 200, 50)},
    "Fortifier": {"desc": "+25 max garrison", "color": (140, 180, 220)},
    "Recruiter": {"desc": "Faster troop regen", "color": (100, 200, 150)},
    "Merchant": {"desc": "+1 merchant capacity", "color": (220, 180, 50)},
    "Diplomat": {"desc": "Reduces native raids", "color": (180, 220, 255)},
    "Devout": {"desc": "+3 happiness from religion", "color": (220, 200, 255)},
    "Shipwright": {"desc": "Faster naval production", "color": (80, 180, 220)},
    "Explorer": {"desc": "Faster settler spawns", "color": (200, 255, 150)},
    "Surgeon": {"desc": "Troops heal faster", "color": (255, 200, 200)},
    "Smuggler": {"desc": "+3 gold, occasional incident", "color": (180, 140, 80)},
    "Warmonger": {"desc": "+troops but -2 happiness", "color": (220, 80, 80)},
    "Isolationist": {"desc": "+defense, -trade income", "color": (160, 160, 200)},
    "Drunkard": {"desc": "Occasional admin penalty", "color": (200, 100, 100)},
    "Gambler": {"desc": "Occasional gold penalty", "color": (255, 150, 50)},
    "Embezzler": {"desc": "-2 gold production", "color": (200, 50, 50)},
    "Reformer": {"desc": "Focuses on upgrades", "color": (150, 200, 255)},
    "Lazy": {"desc": "Slower administration", "color": (150, 150, 150)},
    "Corrupt": {"desc": "Siphons gold periodically", "color": (160, 50, 80)},
    "Tyrant": {"desc": "+defense but -3 happiness", "color": (180, 40, 40)},
    "Paranoid": {"desc": "Wastes gold on defenses", "color": (170, 120, 170)},
    "Sickly": {"desc": "May die and get replaced", "color": (140, 140, 100)},
}
_TRAIT_NAMES = list(GOVERNOR_TRAITS.keys())

# Governor ethnicity/religion assignment based on owner faction
_GOV_ETHNICITY_BY_FACTION = {
    "Great Britain": ["British", "British", "British", "British", "French", "African"],
    "United States": ["American", "American", "American", "American", "British", "African", "French"],
    "France": ["French", "French", "French", "French", "African", "Indigenous"],
    "Spain": ["Spanish", "Spanish", "Spanish", "Latin American", "Indigenous", "African"],
    "Mexico": ["Latin American", "Latin American", "Latin American", "Spanish", "Indigenous"],
    "Haiti": ["African", "African", "African", "African", "French"],
    "Russia": ["Russian", "Russian", "Russian", "Russian", "Indigenous"],
    "Denmark": ["Danish", "Danish", "Danish", "Danish", "Indigenous"],
    "Texas": ["American", "American", "American", "Latin American", "Spanish"],
    "Confederate States": ["American", "American", "American", "American", "British"],
    "Canada": ["British", "British", "French", "French", "British"],
    "Rupert's Land": ["British", "British", "Indigenous", "French", "British"],
}
_GOV_RELIGION_BY_FACTION = {
    "Great Britain": ["Protestant", "Protestant", "Protestant", "Catholic"],
    "United States": ["Protestant", "Protestant", "Protestant", "Catholic"],
    "France": ["Catholic", "Catholic", "Catholic", "Catholic", "Protestant"],
    "Spain": ["Catholic", "Catholic", "Catholic", "Catholic"],
    "Mexico": ["Catholic", "Catholic", "Catholic", "Catholic"],
    "Haiti": ["Catholic", "Catholic", "Animism", "Animism"],
    "Russia": ["Orthodox", "Orthodox", "Orthodox", "Orthodox"],
    "Denmark": ["Protestant", "Protestant", "Protestant", "Protestant"],
    "Texas": ["Protestant", "Protestant", "Protestant", "Catholic"],
    "Confederate States": ["Protestant", "Protestant", "Protestant", "Catholic"],
    "Canada": ["Protestant", "Protestant", "Catholic", "Catholic"],
    "Rupert's Land": ["Protestant", "Protestant", "Catholic", "Animism"],
}

def _get_city_governor(city):
    """Get or assign a governor/chief to a city."""
    owner = city["owner"]
    if "_governor" not in city:
        if owner in NATIVE_FACTIONS:
            # Native settlements get chiefs
            city["_governor"] = random_chief_name(owner)
            city["_governor_type"] = random.randint(0, 1)  # 2 chief sprites
            city["_is_chief"] = True
            # Assign 1-2 random traits (fewer than colonial governors)
            num_traits = random.choice([1, 1, 2])
            city["_governor_traits"] = random.sample(_TRAIT_NAMES, num_traits)
            city["_governor_ethnicity"] = "Indigenous"
            city["_governor_religion"] = "Animism"
        else:
            city["_governor"] = random_governor_name(owner)
            city["_governor_type"] = random.randint(0, 3)
            city["_is_chief"] = False
            # Assign 2-3 random traits
            num_traits = random.choice([2, 2, 2, 3])
            city["_governor_traits"] = random.sample(_TRAIT_NAMES, num_traits)
            # Assign ethnicity and religion
            eth_pool = _GOV_ETHNICITY_BY_FACTION.get(owner, ["British"])
            rel_pool = _GOV_RELIGION_BY_FACTION.get(owner, ["Protestant"])
            # Filter out ethnic groups that are enslaved or segregated — they can't be governors
            laws = _get_faction_laws(owner)
            eligible_eth = [e for e in eth_pool if laws["ethnic"].get(e, 0) <= 0]
            if not eligible_eth:
                eligible_eth = [e for e in eth_pool if laws["ethnic"].get(e, 0) <= 2]
            if not eligible_eth:
                eligible_eth = eth_pool[:1]
            eligible_rel = [r for r in rel_pool if laws["religion"].get(r, 0) <= 1]
            if not eligible_rel:
                eligible_rel = [r for r in rel_pool if laws["religion"].get(r, 0) <= 2]
            if not eligible_rel:
                eligible_rel = rel_pool[:1]
            city["_governor_ethnicity"] = random.choice(eligible_eth)
            city["_governor_religion"] = random.choice(eligible_rel)
    return city["_governor"], city["_governor_type"]

def _get_governor_popularity(city):
    """Calculate governor popularity modifier based on how well their ethnicity/religion
    matches the city demographics. Returns happiness bonus/penalty (-5 to +5)."""
    gov_eth = city.get("_governor_ethnicity")
    gov_rel = city.get("_governor_religion")
    if not gov_eth or not gov_rel: return 0
    bonus = 0
    # Ethnicity match: compare governor's ethnicity to city demographics
    demos = _get_city_demographics(city)
    eth_pct = demos.get(gov_eth, 0)
    if eth_pct >= 50:
        bonus += 3  # majority matches — very popular
    elif eth_pct >= 25:
        bonus += 1  # significant minority — somewhat popular
    elif eth_pct < 10:
        bonus -= 3  # almost no one shares their background — unpopular
    # Religion match: compare governor's religion to city religion
    religion = _get_city_religion(city)
    rel_pct = religion.get(gov_rel, 0)
    if rel_pct >= 50:
        bonus += 2  # majority shares faith — popular
    elif rel_pct >= 25:
        bonus += 1
    elif rel_pct < 10:
        bonus -= 2  # religious outsider — unpopular
    return max(-5, min(5, bonus))

def _get_governor_traits(city):
    """Get the governor's traits for a city."""
    return city.get("_governor_traits", [])

def _draw_governor_portrait(city):
    """Draw governor portrait + traits panel aligned left of the city panel."""
    if not _selected_city: return
    _init_governor_sprites()
    gov_name, gov_type = _get_city_governor(city)
    if gov_name is None: return
    W, H = get_screen_size()
    # Align with city panel (panel is at center, 700 wide, 320 tall, 10px from bottom)
    panel_w = 700; panel_h = 320
    city_panel_x = (W - panel_w) // 2
    city_panel_y = H - panel_h - 10
    # Governor panel to the left of city panel
    gp_w = 200; gp_h = panel_h
    gp_x = city_panel_x - gp_w - 10
    gp_y = city_panel_y
    # Background panel
    gp_surf = pygame.Surface((gp_w, gp_h), pygame.SRCALPHA); gp_surf.fill((15, 15, 15, 200))
    screen.blit(gp_surf, (gp_x, gp_y))
    _draw_panel_frame(gp_x, gp_y, gp_w, gp_h)
    # Portrait background + sprite (upper portion)
    bg = pygame.transform.scale(_governor_bg, (120, 150))
    screen.blit(bg, (gp_x + 40, gp_y + 10))
    # Governor sprite animation
    if city["owner"] == "Haiti" and _governor_haiti_sprite:
        gov_data = _governor_haiti_sprite
    elif city["owner"] in NATIVE_FACTIONS and _governor_chief_sprites:
        gov_data = _governor_chief_sprites[gov_type % len(_governor_chief_sprites)]
    else:
        gov_data = _governor_sprites[gov_type % len(_governor_sprites)]
    idle_frame = gov_data["idle"]
    turn_frames = gov_data["turn"]
    cycle = (_brit_anim_tick % 300)
    if cycle < 200:
        gov_frame = idle_frame
    else:
        turn_progress = cycle - 200
        step = min(5, turn_progress // 16)
        sequence = [0, 1, 2, 2, 1, 0]
        frame_idx = sequence[step] if step < 6 else 0
        gov_frame = turn_frames[frame_idx] if frame_idx < len(turn_frames) else idle_frame
    look_left = (hash(gov_name) % 2 == 0)
    gov_sprite = pygame.transform.scale(gov_frame, (100, 140))
    if look_left and cycle >= 200:
        gov_sprite = pygame.transform.flip(gov_sprite, True, False)
    screen.blit(gov_sprite, (gp_x + 50, gp_y + 15))
    # Frame around portrait
    frame = pygame.transform.scale(_governor_frame, (130, 160))
    screen.blit(frame, (gp_x + 35, gp_y + 5))
    # Governor name and title below portrait
    _gov_font = pygame.font.SysFont(None, 20, bold=True)
    name_surf = _gov_font.render(gov_name, True, (255, 220, 100))
    screen.blit(name_surf, (gp_x + gp_w//2 - name_surf.get_width()//2, gp_y + 170))
    title_text = "Chief" if city.get("_is_chief") else "Governor"
    title_surf = pygame.font.SysFont(None, 16).render(title_text, True, (180, 180, 180))
    screen.blit(title_surf, (gp_x + gp_w//2 - title_surf.get_width()//2, gp_y + 188))
    # Separator
    pygame.draw.line(screen, (80, 80, 80), (gp_x + 15, gp_y + 205), (gp_x + gp_w - 15, gp_y + 205))
    # Traits below
    traits = _get_governor_traits(city)
    _tp_trait_font = pygame.font.SysFont(None, 17, bold=True)
    _tp_desc_font = pygame.font.SysFont(None, 14)
    ty = gp_y + 212
    # Governor ethnicity & religion
    gov_eth = city.get("_governor_ethnicity", "")
    gov_rel = city.get("_governor_religion", "")
    if gov_eth:
        eth_col = _DEMOG_GROUPS.get(gov_eth, (200, 200, 200))
        screen.blit(_tp_desc_font.render(f"{gov_eth}, {gov_rel}", True, eth_col), (gp_x + 12, ty))
        ty += 16
    # Popularity from ethnicity/religion match
    pop_bonus = _get_governor_popularity(city)
    if pop_bonus > 0:
        pop_text = f"Popularity: +{pop_bonus}"; pop_col = (100, 255, 100)
    elif pop_bonus < 0:
        pop_text = f"Popularity: {pop_bonus}"; pop_col = (255, 100, 100)
    else:
        pop_text = "Popularity: 0"; pop_col = (180, 180, 180)
    screen.blit(_tp_desc_font.render(pop_text, True, pop_col), (gp_x + 12, ty))
    ty += 20
    # Separator before traits
    pygame.draw.line(screen, (60, 60, 60), (gp_x + 12, ty - 4), (gp_x + gp_w - 12, ty - 4))
    if traits:
        for trait in traits:
            tdata = GOVERNOR_TRAITS.get(trait, {"desc": "", "color": (200, 200, 200)})
            screen.blit(_tp_trait_font.render(trait, True, tdata["color"]), (gp_x + 12, ty)); ty += 16
            screen.blit(_tp_desc_font.render(tdata["desc"], True, (150, 150, 150)), (gp_x + 16, ty)); ty += 18
    else:
        screen.blit(_tp_desc_font.render("No traits", True, (150, 150, 150)), (gp_x + 12, ty))

# === Demographics System ===
_DEMOG_GROUPS = {
    "British": (200, 50, 50),
    "French": (50, 100, 255),
    "Spanish": (255, 200, 0),
    "African": (100, 60, 30),
    "Indigenous": (150, 100, 50),
    "Russian": (0, 150, 0),
    "Danish": (0, 200, 0),
    "Latin American": (200, 120, 50),
    "American": (100, 150, 255),
}

# Historical demographics overrides: {city_name: {scenario_year: {group: pct}}}
# These represent historically accurate demographics for specific cities at specific times
from historical_data import _HISTORICAL_DEMOGRAPHICS, _HISTORICAL_RELIGION, _CITY_POP_OVERRIDES  # extracted tables

# Map faction to demographic group name
_FACTION_TO_DEMOG = {
    "Great Britain": "British", "France": "French", "Spain": "Spanish",
    "United States": "American", "Mexico": "Latin American", "Haiti": "African",
    "Russia": "Russian", "Denmark": "Danish", "Texas": "American",
    "Confederate States": "American",
    "Canada": "British", "Rupert's Land": "British",
    "Iroquois": "Indigenous", "Wabanaki": "Indigenous", "Comanche": "Indigenous",
    "Cree": "Indigenous", "Cherokee": "Indigenous", "Dakota": "Indigenous",
}

def _get_city_demographics(city):
    """Get or generate demographics based on historical data, owner, sovereign, and game year."""
    # If demographics already cached and year hasn't changed scenario, use cache
    if city.get("_demog_cache_year") == game_year and "_demographics" in city:
        return city["_demographics"]

    name = city.get("name", "")
    owner = city.get("owner", "Great Britain")
    sovereign = city.get("sovereign", owner)

    # Check if there's a historical override for this city
    if name in _HISTORICAL_DEMOGRAPHICS:
        hist = _HISTORICAL_DEMOGRAPHICS[name]
        # Find closest scenario year at or below current game_year
        available_years = sorted(hist.keys())
        chosen_year = available_years[0]
        for yr in available_years:
            if yr <= game_year:
                chosen_year = yr
        demos = dict(hist[chosen_year])

        # If owner changed AFTER the scenario start (during gameplay), shift demographics
        # toward the controlling faction over time
        owner_group = _FACTION_TO_DEMOG.get(owner, "British")
        if owner != sovereign and owner_group not in demos:
            # Add small initial presence of occupier
            demos[owner_group] = 2
        # Normalize
        total = sum(demos.values())
        if total > 0 and total != 100:
            demos = {k: max(1, int(v * 100 / total)) for k, v in demos.items() if v > 0}
            diff = 100 - sum(demos.values())
            if demos and diff != 0:
                key = max(demos, key=demos.get)
                demos[key] += diff
    else:
        # Generic demographics based on sovereign and owner
        demos = _generate_generic_demographics(city, owner, sovereign)

    city["_demographics"] = demos
    city["_demog_cache_year"] = game_year
    return demos

def _generate_generic_demographics(city, owner, sovereign):
    """Generate demographics for cities without historical overrides."""
    owner_group = _FACTION_TO_DEMOG.get(owner, "British")
    sovereign_group = _FACTION_TO_DEMOG.get(sovereign, "British")

    if sovereign in ("Iroquois", "Wabanaki", "Comanche", "Cree", "Cherokee", "Dakota"):
        # Native settlement — if still native-owned, almost all indigenous
        if owner == sovereign:
            demos = {"Indigenous": 100}
        else:
            # Colonized native settlement — big shift toward colonizer
            demos = {owner_group: random.randint(45, 65), "Indigenous": random.randint(15, 30),
                     "African": random.randint(5, 15)}
            if owner_group != "Spanish" and owner_group != "Latin American":
                demos["Latin American"] = random.randint(3, 10)
    elif sovereign == "France":
        if owner == sovereign:
            demos = {"French": random.randint(60, 75), "Indigenous": random.randint(10, 18),
                     "African": random.randint(8, 18)}
        else:
            # French city under other control — French still majority but shrinking
            yrs_occupied = max(0, game_year - 1754)
            french_pct = max(30, 70 - yrs_occupied // 5)
            owner_pct = min(40, 10 + yrs_occupied // 4)
            demos = {"French": french_pct, owner_group: owner_pct,
                     "Indigenous": random.randint(4, 10), "African": random.randint(5, 12)}
    elif sovereign in ("Spain", "Mexico"):
        if owner == sovereign:
            demos = {"Spanish": random.randint(30, 45), "Indigenous": random.randint(20, 35),
                     "Latin American": random.randint(15, 25), "African": random.randint(5, 12)}
        else:
            demos = {"Spanish": random.randint(15, 30), "Latin American": random.randint(15, 25),
                     owner_group: random.randint(20, 35), "Indigenous": random.randint(10, 20),
                     "African": random.randint(5, 12)}
    elif sovereign == "Haiti":
        demos = {"African": random.randint(80, 92), "French": random.randint(4, 12),
                 "Indigenous": random.randint(2, 6)}
    elif sovereign == "Russia":
        if owner == sovereign:
            demos = {"Russian": random.randint(45, 60), "Indigenous": random.randint(30, 45)}
        else:
            demos = {"Russian": random.randint(20, 35), owner_group: random.randint(25, 40),
                     "Indigenous": random.randint(20, 35)}
    elif sovereign == "Denmark":
        if owner == sovereign:
            demos = {"Danish": random.randint(45, 60), "Indigenous": random.randint(30, 45)}
        else:
            demos = {"Danish": random.randint(20, 35), owner_group: random.randint(25, 40),
                     "Indigenous": random.randint(20, 35)}
    elif sovereign in ("Great Britain", "United States", "Texas", "Confederate States", "Canada", "Rupert's Land"):
        if owner == sovereign:
            demos = {owner_group: random.randint(50, 65), "African": random.randint(15, 30),
                     "Indigenous": random.randint(3, 10)}
        else:
            demos = {sovereign_group: random.randint(25, 40), owner_group: random.randint(25, 40),
                     "African": random.randint(10, 20), "Indigenous": random.randint(3, 10)}
    else:
        demos = {"British": random.randint(40, 55), "Indigenous": random.randint(15, 30),
                 "African": random.randint(10, 20)}

    # Remove zero or negative entries and normalize to 100
    demos = {k: v for k, v in demos.items() if v > 0}
    total = sum(demos.values())
    if total > 0 and total != 100:
        demos = {k: max(1, int(v * 100 / total)) for k, v in demos.items()}
    diff = 100 - sum(demos.values())
    if demos and diff != 0:
        key = max(demos, key=demos.get)
        demos[key] += diff
    return demos

# === Religion System ===
_RELIGION_COLORS = {
    "Protestant": (180, 130, 255),
    "Catholic": (255, 215, 0),
    "Orthodox": (0, 200, 180),
    "Animism": (120, 180, 80),
    "Mesoamerican": (200, 80, 60),
}

# Map factions to their dominant religion
_FACTION_RELIGION = {
    "Great Britain": "Protestant",
    "United States": "Protestant",
    "Texas": "Protestant",
    "Confederate States": "Protestant",
    "France": "Catholic",
    "Spain": "Catholic",
    "Mexico": "Catholic",
    "Haiti": "Catholic",
    "Russia": "Orthodox",
    "Denmark": "Protestant",
    "Iroquois": "Animism",
    "Wabanaki": "Animism",
    "Comanche": "Animism",
    "Cree": "Animism",
    "Cherokee": "Animism",
    "Dakota": "Animism",
}

# Historical religion overrides for key cities: {city: {year: {religion: pct}}}

def _get_city_religion(city):
    """Get or generate religion breakdown for a city."""
    if city.get("_religion_cache_year") == game_year and "_religion" in city:
        return city["_religion"]

    name = city.get("name", "")
    owner = city.get("owner", "Great Britain")
    sovereign = city.get("sovereign", owner)

    # Check historical overrides
    if name in _HISTORICAL_RELIGION:
        hist = _HISTORICAL_RELIGION[name]
        available_years = sorted(hist.keys())
        chosen_year = available_years[0]
        for yr in available_years:
            if yr <= game_year:
                chosen_year = yr
        religion = dict(hist[chosen_year])
    else:
        # Generate based on demographics/faction
        religion = _generate_generic_religion(city, owner, sovereign)

    # Normalize
    total = sum(religion.values())
    if total > 0 and total != 100:
        religion = {k: max(1, int(v * 100 / total)) for k, v in religion.items() if v > 0}
        diff = 100 - sum(religion.values())
        if religion and diff != 0:
            key = max(religion, key=religion.get)
            religion[key] += diff

    city["_religion"] = religion
    city["_religion_cache_year"] = game_year
    return religion

def _generate_generic_religion(city, owner, sovereign):
    """Generate religion breakdown for cities without historical overrides."""
    owner_rel = _FACTION_RELIGION.get(owner, "Protestant")
    sovereign_rel = _FACTION_RELIGION.get(sovereign, "Protestant")

    if sovereign in ("Iroquois", "Wabanaki", "Comanche", "Cree", "Cherokee", "Dakota"):
        if owner == sovereign:
            rel = {"Animism": random.randint(88, 98)}
            remaining = 100 - sum(rel.values())
            # Slight missionary presence
            if sovereign_rel != "Animism":
                rel[sovereign_rel] = remaining
            else:
                rel["Catholic"] = remaining
        else:
            # Colonized — dominant religion of colonizer plus remaining animism
            rel = {owner_rel: random.randint(40, 60), "Animism": random.randint(20, 40)}
            if owner_rel == "Protestant":
                rel["Catholic"] = random.randint(5, 15)
            else:
                rel["Protestant"] = random.randint(3, 10)
    elif sovereign == "France" or sovereign == "Spain" or sovereign == "Mexico" or sovereign == "Haiti":
        # Catholic nations
        if owner == sovereign:
            rel = {"Catholic": random.randint(70, 88), "Animism": random.randint(5, 15)}
            remaining = 100 - sum(rel.values())
            rel["Protestant"] = remaining
        else:
            # Under Protestant control — mix shifts
            if owner_rel == "Protestant":
                yrs = max(0, game_year - 1754)
                cath_pct = max(35, 80 - yrs // 4)
                prot_pct = min(50, 10 + yrs // 4)
                rel = {"Catholic": cath_pct, "Protestant": prot_pct,
                       "Animism": random.randint(3, 10)}
            else:
                rel = {"Catholic": random.randint(60, 80), owner_rel: random.randint(10, 25),
                       "Animism": random.randint(3, 10)}
    elif sovereign == "Russia":
        if owner == sovereign:
            rel = {"Orthodox": random.randint(50, 65), "Animism": random.randint(28, 42)}
            remaining = 100 - sum(rel.values())
            rel["Protestant"] = remaining
        else:
            rel = {"Orthodox": random.randint(25, 40), owner_rel: random.randint(25, 40),
                   "Animism": random.randint(15, 30)}
    elif sovereign == "Denmark":
        if owner == sovereign:
            rel = {"Protestant": random.randint(50, 65), "Animism": random.randint(30, 45)}
            remaining = 100 - sum(rel.values())
            if remaining > 0:
                rel["Catholic"] = remaining
        else:
            rel = {"Protestant": random.randint(35, 50), owner_rel: random.randint(20, 35),
                   "Animism": random.randint(15, 30)}
    else:
        # British/American/etc — Protestant dominant
        if owner == sovereign:
            rel = {"Protestant": random.randint(65, 82), "Catholic": random.randint(8, 18),
                   "Animism": random.randint(3, 10)}
        else:
            rel = {"Protestant": random.randint(30, 50), owner_rel: random.randint(25, 40),
                   "Animism": random.randint(5, 15)}

    # Add tiny Mesoamerican presence for cities in Mexico/Central America area
    city_y = city.get("y", 0)
    if city_y > 350 and "Mesoamerican" not in rel:
        # Southern latitude — possible mesoamerican traces
        meso = random.randint(1, 5)
        rel["Mesoamerican"] = meso

    return rel

def _draw_demographics(city):
    """Draw combined demographics and religion panel to the right of the city panel."""
    if not _selected_city: return
    W, H = get_screen_size()
    panel_w = 700; panel_h = 320
    city_panel_x = (W - panel_w) // 2
    city_panel_y = H - panel_h - 10
    # Position to the right of city panel — expand to fill available space to screen edge
    dp_x = city_panel_x + panel_w + 10
    dp_y = city_panel_y
    dp_w = max(200, W - dp_x - 10); dp_h = panel_h
    # Background
    dp_surf = pygame.Surface((dp_w, dp_h), pygame.SRCALPHA); dp_surf.fill((15, 15, 15, 200))
    screen.blit(dp_surf, (dp_x, dp_y))
    _draw_panel_frame(dp_x, dp_y, dp_w, dp_h)
    _dp_title_font = pygame.font.SysFont(None, 20, bold=True)
    _dp_font = pygame.font.SysFont(None, 16)
    bar_w = dp_w // 2 - 20
    dy = dp_y + 6
    # --- Ethnicity ---
    pop = _get_city_population(city)
    screen.blit(_dp_title_font.render("Ethnicity", True, (255, 220, 100)), (dp_x + 8, dy))
    dy += 16
    demos = _get_city_demographics(city)
    for group, pct in sorted(demos.items(), key=lambda x: -x[1]):
        if pct <= 0: continue
        col = _DEMOG_GROUPS.get(group, (200, 200, 200))
        group_pop = int(pop * pct / 100)
        screen.blit(_dp_font.render(f"{group} {pct}% ({_format_population(group_pop)})", True, (220, 220, 220)), (dp_x + 8, dy))
        dy += 12
        pygame.draw.rect(screen, (40, 40, 40), (dp_x + 8, dy, bar_w, 7))
        pygame.draw.rect(screen, col, (dp_x + 8, dy, int(bar_w * pct / 100), 7))
        dy += 11
    # --- Separator ---
    dy += 4
    pygame.draw.line(screen, (80, 80, 80), (dp_x + 8, dy), (dp_x + dp_w - 8, dy))
    dy += 6
    # --- Religion ---
    screen.blit(_dp_title_font.render("Religion", True, (255, 220, 100)), (dp_x + 8, dy))
    dy += 16
    religion = _get_city_religion(city)
    for rel_name, pct in sorted(religion.items(), key=lambda x: -x[1]):
        if pct <= 0: continue
        col = _RELIGION_COLORS.get(rel_name, (200, 200, 200))
        screen.blit(_dp_font.render(f"{rel_name} {pct}%", True, (220, 220, 220)), (dp_x + 8, dy))
        dy += 12
        pygame.draw.rect(screen, (40, 40, 40), (dp_x + 8, dy, bar_w, 7))
        pygame.draw.rect(screen, col, (dp_x + 8, dy, int(bar_w * pct / 100), 7))
        dy += 11
    # --- Dominant ethnicity image (grassland + frame + icon) on the right side ---
    _init_laws_assets()
    if demos:
        dominant_group = max(demos, key=demos.get)
        img_size = 100
        img_x = dp_x + dp_w - img_size - 20
        img_y = dp_y + 20
        # Grassland background
        if _laws_grassland_bg:
            grass = pygame.transform.scale(_laws_grassland_bg, (img_size, img_size))
            screen.blit(grass, (img_x, img_y))
        # Ethnic icon centered on grassland
        icon = _ethnicity_icons.get(dominant_group)
        if icon:
            scaled_icon = pygame.transform.scale(icon, (img_size - 12, img_size - 12))
            screen.blit(scaled_icon, (img_x + 6, img_y + 6))
        # Frame wrapping around grassland
        if _laws_frame_img:
            frame = pygame.transform.scale(_laws_frame_img, (img_size + 16, img_size + 16))
            screen.blit(frame, (img_x - 8, img_y - 8))

# === Laws System ===
# Ethnic law statuses (4 tiers)
ETHNIC_LAW_STATUSES = ["Protected Rights", "Segregated", "Suppressed", "Enslaved"]
ETHNIC_LAW_COLORS = {
    "Protected Rights": (100, 255, 100),
    "Segregated": (255, 220, 50),
    "Suppressed": (255, 140, 50),
    "Enslaved": (255, 60, 60),
}
# Religion law statuses (4 tiers)
RELIGION_LAW_STATUSES = ["Free Faith", "Restricted", "Suppressed", "Banned"]
RELIGION_LAW_COLORS = {
    "Free Faith": (100, 255, 100),
    "Restricted": (255, 220, 50),
    "Suppressed": (255, 140, 50),
    "Banned": (255, 60, 60),
}
# All ethnic groups and religions that can have laws
_LAW_ETHNIC_GROUPS = ["British", "French", "Spanish", "African", "Indigenous", "Russian", "Danish", "Latin American", "American"]
_LAW_RELIGIONS = ["Protestant", "Catholic", "Orthodox", "Animism", "Mesoamerican"]

# Default laws per faction — historically accurate starting positions
# {faction: {"ethnic": {group: status_index}, "religion": {religion: status_index}}}
def _init_faction_laws():
    """Initialize default laws for all factions based on historical context."""
    laws = {}
    for faction in ["Great Britain", "United States", "France", "Spain", "Mexico",
                    "Haiti", "Russia", "Denmark", "Texas", "Confederate States"]:
        eth = {}; rel = {}
        # Default: own group protected, others vary
        for g in _LAW_ETHNIC_GROUPS:
            eth[g] = 0  # Protected Rights by default
        for r in _LAW_RELIGIONS:
            rel[r] = 0  # Free Faith by default

        if faction == "Great Britain":
            eth["African"] = 3  # Enslaved
            eth["Indigenous"] = 2  # Suppressed
            rel["Catholic"] = 1  # Restricted
            rel["Animism"] = 2  # Suppressed
        elif faction == "United States":
            eth["African"] = 3  # Enslaved
            eth["Indigenous"] = 2  # Suppressed
            rel["Animism"] = 2  # Suppressed
            rel["Mesoamerican"] = 3  # Banned
        elif faction == "France":
            eth["African"] = 3  # Enslaved
            eth["Indigenous"] = 1  # Segregated
            rel["Protestant"] = 1  # Restricted
            rel["Animism"] = 2  # Suppressed
        elif faction == "Spain":
            eth["African"] = 3  # Enslaved
            eth["Indigenous"] = 1  # Segregated
            rel["Protestant"] = 2  # Suppressed
            rel["Animism"] = 2  # Suppressed
            rel["Mesoamerican"] = 2  # Suppressed
        elif faction == "Mexico":
            eth["African"] = 0  # Protected (Mexico abolished slavery)
            eth["Indigenous"] = 1  # Segregated
            rel["Protestant"] = 1  # Restricted
            rel["Animism"] = 1  # Restricted
        elif faction == "Haiti":
            eth["French"] = 2  # Suppressed
            eth["British"] = 2  # Suppressed
            eth["Spanish"] = 1  # Segregated
            rel["Animism"] = 0  # Free Faith (Vodou)
        elif faction == "Russia":
            eth["Indigenous"] = 1  # Segregated
            rel["Catholic"] = 1  # Restricted
            rel["Protestant"] = 1  # Restricted
            rel["Animism"] = 1  # Restricted
        elif faction == "Denmark":
            eth["African"] = 3  # Enslaved
            eth["Indigenous"] = 1  # Segregated
            rel["Catholic"] = 1  # Restricted
            rel["Animism"] = 2  # Suppressed
        elif faction == "Texas":
            eth["African"] = 3  # Enslaved
            eth["Indigenous"] = 2  # Suppressed
            eth["Latin American"] = 1  # Segregated
            rel["Catholic"] = 0  # Free Faith
            rel["Animism"] = 2  # Suppressed
        elif faction == "Confederate States":
            eth["African"] = 3  # Enslaved
            eth["Indigenous"] = 2  # Suppressed
            rel["Animism"] = 3  # Banned

        laws[faction] = {"ethnic": eth, "religion": rel}
    return laws

_faction_laws = _init_faction_laws()
_laws_panel_open = False

def _get_faction_laws(faction):
    """Get laws for a faction, initializing defaults if needed."""
    if faction not in _faction_laws:
        eth = {g: 0 for g in _LAW_ETHNIC_GROUPS}
        rel = {r: 0 for r in _LAW_RELIGIONS}
        _faction_laws[faction] = {"ethnic": eth, "religion": rel}
    return _faction_laws[faction]

# === Production Policies ===
# Each faction picks a gold policy and food policy independently
# Format: (name, production_modifier, happiness_modifier)
GOLD_POLICIES = [
    ("Light Taxation", -0.10, +3),       # less gold, happier people
    ("Balanced Taxation", 0.0, 0),        # default
    ("Heavy Taxation", +0.20, -3),        # more gold, people unhappy
    ("Exploitative", +0.35, -8),          # max gold, serious unrest
]
FOOD_POLICIES = [
    ("Subsistence", -0.10, +2),           # less food, less pressure on workers
    ("Balanced Farming", 0.0, 0),         # default
    ("Intensive Farming", +0.20, -2),     # more food, workers strained
    ("Forced Production", +0.35, -6),     # max food, brutal labor conditions
]

# {faction: {"gold_policy": index, "food_policy": index}}
_faction_policies = {}

def _init_faction_policies():
    """Initialize all factions to balanced policies."""
    for f in ["Great Britain", "United States", "France", "Spain", "Mexico",
              "Haiti", "Russia", "Denmark", "Texas", "Confederate States"]:
        _faction_policies[f] = {"gold_policy": 1, "food_policy": 1}  # Balanced defaults
_init_faction_policies()

def _get_gold_policy(faction):
    """Get current gold policy tuple for a faction."""
    if faction not in _faction_policies:
        _faction_policies[faction] = {"gold_policy": 1, "food_policy": 1}
    idx = _faction_policies[faction]["gold_policy"]
    return GOLD_POLICIES[idx]

def _get_food_policy(faction):
    """Get current food policy tuple for a faction."""
    if faction not in _faction_policies:
        _faction_policies[faction] = {"gold_policy": 1, "food_policy": 1}
    idx = _faction_policies[faction]["food_policy"]
    return FOOD_POLICIES[idx]

def _ai_update_policies():
    """AI adjusts production policies based on economic situation. Runs monthly."""
    for faction in active_factions():
        if faction in NATIVE_FACTIONS: continue
        if faction not in _faction_policies:
            _faction_policies[faction] = {"gold_policy": 1, "food_policy": 1}
        mats = faction_materials.get(faction, {})
        gold = mats.get("Gold", 0)
        food = mats.get("Food", 0)
        unit_count = sum(1 for u in units if u.owner["owner"] == faction)
        city_count = sum(1 for c in cities if c["owner"] == faction)
        avg_happiness = 0
        faction_cities = [c for c in cities if c["owner"] == faction]
        if faction_cities:
            avg_happiness = sum(c.get("_happiness", 100) for c in faction_cities) // len(faction_cities)

        # Gold policy decision
        _is_at_war = any(pair(faction, f) in wars for f in active_factions() if f != faction)
        if gold < 20 and avg_happiness > 40:
            # Broke — raise taxes
            _faction_policies[faction]["gold_policy"] = min(3, _faction_policies[faction]["gold_policy"] + 1)
        elif gold > 500 and avg_happiness < 50:
            # Rich but unhappy — lower taxes
            _faction_policies[faction]["gold_policy"] = max(0, _faction_policies[faction]["gold_policy"] - 1)
        elif gold > 300 and not _is_at_war:
            # Comfortable peacetime — can afford to relax
            if _faction_policies[faction]["gold_policy"] > 1:
                _faction_policies[faction]["gold_policy"] -= 1
        elif gold < 50 and _is_at_war:
            # War is expensive — tax harder
            _faction_policies[faction]["gold_policy"] = min(3, _faction_policies[faction]["gold_policy"] + 1)

        # Food policy decision
        food_per_unit = 3 * unit_count  # rough daily food drain
        food_days_left = food / max(1, food_per_unit)  # how many days of food we have
        if food_days_left < 5 and avg_happiness > 35:
            # Running out of food — intensify
            _faction_policies[faction]["food_policy"] = min(3, _faction_policies[faction]["food_policy"] + 1)
        elif food_days_left > 30 and avg_happiness < 50:
            # Plenty of food, people are unhappy — ease off
            _faction_policies[faction]["food_policy"] = max(0, _faction_policies[faction]["food_policy"] - 1)
        elif food > 400 and not _is_at_war:
            # Lots of food in peacetime — relax
            if _faction_policies[faction]["food_policy"] > 1:
                _faction_policies[faction]["food_policy"] -= 1

        # Conscription policy decision
        if faction not in _faction_conscription: _faction_conscription[faction] = 1
        manpower = _faction_manpower.get(faction, 0)
        if _is_at_war and manpower < 200:
            # At war and running out of men — conscript harder
            _faction_conscription[faction] = min(3, _faction_conscription[faction] + 1)
        elif _is_at_war and manpower < 500:
            # At war, low manpower — standard or higher
            _faction_conscription[faction] = max(1, _faction_conscription[faction])
        elif not _is_at_war and manpower > 1000 and avg_happiness < 60:
            # Peacetime with plenty of men, people unhappy — ease off
            _faction_conscription[faction] = max(0, _faction_conscription[faction] - 1)
        elif not _is_at_war and avg_happiness > 70:
            # Peace and happy — can drop to volunteer
            if _faction_conscription[faction] > 0:
                _faction_conscription[faction] -= 1

# === Population System ===
# Realistic population sizes for city types at game start
_BASE_POPULATION = {
    "capital": 12000,
    "city": 5000,
    "village": 800,
    "fort": 200,
    "camp": 150,
}
# Historical population multipliers by scenario year (cities were smaller earlier)
_POP_YEAR_MULT = {1754: 1.0, 1790: 1.4, 1809: 1.7, 1835: 2.2, 1858: 3.0}
# Special city population overrides (approximate historical populations)

def _get_city_population(city):
    """Get or initialize population for a city. Grows over time."""
    if "_population" not in city:
        name = city.get("name", "")
        # Check override
        if name in _CITY_POP_OVERRIDES:
            hist = _CITY_POP_OVERRIDES[name]
            available_years = sorted(hist.keys())
            chosen_year = available_years[0]
            for yr in available_years:
                if yr <= game_year:
                    chosen_year = yr
            base_pop = hist[chosen_year]
        else:
            # Generic based on type
            if city.get("is_camp"): base_pop = _BASE_POPULATION["camp"]
            elif city.get("is_fort"): base_pop = _BASE_POPULATION["fort"]
            elif city.get("is_village"): base_pop = _BASE_POPULATION["village"]
            elif city["is_capital"]: base_pop = _BASE_POPULATION["capital"]
            else: base_pop = _BASE_POPULATION["city"]
            # Apply year multiplier
            available_years = sorted(_POP_YEAR_MULT.keys())
            chosen_year = available_years[0]
            for yr in available_years:
                if yr <= game_year:
                    chosen_year = yr
            base_pop = int(base_pop * _POP_YEAR_MULT[chosen_year])
        # Add some randomness (+/- 15%)
        base_pop = int(base_pop * random.uniform(0.85, 1.15))
        city["_population"] = max(50, base_pop)
        city["_pop_growth_acc"] = 0.0
    return city["_population"]

def _grow_city_population(city):
    """Monthly population growth. Capitals/cities grow faster, happiness affects growth."""
    if "_population" not in city: return
    pop = city["_population"]
    happiness = city.get("_happiness", 100)
    # Base monthly growth rate: 0.1% to 0.4% depending on city type
    if city["is_capital"]: rate = 0.003
    elif city.get("is_fort") or city.get("is_camp"): rate = 0.001
    elif city.get("is_village"): rate = 0.0015
    else: rate = 0.0025
    # Happiness modifier: unhappy cities shrink, happy cities grow faster
    if happiness >= 75: rate *= 1.3
    elif happiness >= 50: rate *= 1.0
    elif happiness >= 25: rate *= 0.5
    else: rate *= -0.2  # population decline when very unhappy
    # Governor popularity bonus
    pop_bonus = _get_governor_popularity(city)
    rate += pop_bonus * 0.0003
    growth = pop * rate
    city["_pop_growth_acc"] = city.get("_pop_growth_acc", 0.0) + growth
    # Only apply integer growth
    if abs(city["_pop_growth_acc"]) >= 1.0:
        add = int(city["_pop_growth_acc"])
        city["_population"] = max(50, city["_population"] + add)
        city["_pop_growth_acc"] -= add
    # Demographic shift: Suppressed groups slowly emigrate, shifting demographics
    owner = city.get("owner", "")
    if owner and owner not in NATIVE_FACTIONS and "_demographics" in city:
        laws = _get_faction_laws(owner)
        demos = city["_demographics"]
        owner_group = _FACTION_TO_DEMOG.get(owner, "British")
        shifted = False
        for group in list(demos.keys()):
            if group == owner_group: continue
            status = laws["ethnic"].get(group, 0)
            if status == 2 and demos.get(group, 0) > 2:  # Suppressed — slow emigration
                demos[group] = max(1, demos[group] - 1)
                demos[owner_group] = demos.get(owner_group, 0) + 1
                shifted = True
                break  # only shift one group per month
            elif status == 3 and demos.get(group, 0) > 5:  # Enslaved — no emigration (they can't leave)
                pass  # enslaved can't flee, population stays but unhappy
        # Protected groups with high happiness attract immigrants (+1% toward them slowly)
        if not shifted and happiness >= 80:
            for group in list(demos.keys()):
                if laws["ethnic"].get(group, 0) == 0 and group == owner_group and demos.get(group, 0) < 95:
                    # Find smallest non-owner group to shrink
                    smallest = min((g for g in demos if g != owner_group and demos[g] > 1), key=demos.get, default=None)
                    if smallest:
                        demos[smallest] -= 1
                        demos[owner_group] += 1
                    break
    # Religion conservation: Church building strengthens the owner's religion
    if owner and owner not in NATIVE_FACTIONS and "_religion" in city:
        if _city_has_building(city, "Church"):
            owner_rel = _FACTION_RELIGION.get(owner, "Protestant")
            rel = city["_religion"]
            # Church slowly converts population toward the state religion
            if rel.get(owner_rel, 0) < 90:
                # Find smallest non-state religion to shrink
                others = [(r, p) for r, p in rel.items() if r != owner_rel and p > 2]
                if others:
                    smallest_rel = min(others, key=lambda x: x[1])[0]
                    rel[smallest_rel] -= 1
                    rel[owner_rel] = rel.get(owner_rel, 0) + 1

def _format_population(pop):
    """Format population number nicely."""
    if pop >= 1000000: return f"{pop/1000000:.1f}M"
    elif pop >= 1000: return f"{pop/1000:.1f}K"
    return str(pop)

def _update_city_happiness(city):
    """Monthly happiness update. Balanced between gains and drains."""
    happiness = city.get("_happiness", 100)
    owner = city["owner"]
    if owner in NATIVE_FACTIONS: return

    # === GAINS ===
    # Base recovery: all cities trend toward 60 (natural equilibrium)
    if happiness < 60:
        happiness += 2  # slow recovery toward equilibrium
    elif happiness < 80:
        happiness += 1  # gentle upward drift

    # Peacetime bonus
    at_war_now = any(at_war(owner, f) for f in active_factions() if f != owner)
    under_siege = any(u.target is city and not u.retreating for u in units if u.owner["owner"] != owner)
    if not under_siege and not at_war_now:
        happiness += 2  # peace dividend

    # Capital bonus — seat of power is always more stable
    if city.get("is_capital"):
        happiness += 1

    # Building bonuses
    if _city_has_building(city, "Church"): happiness += 2
    if _city_has_building(city, "Market"): happiness += 1  # trade makes people content
    if _city_has_building(city, "Town Hall") or _city_has_building(city, "Council Lodge"): happiness += 1  # governance

    # Governor trait bonuses
    traits = city.get("_governor_traits", [])
    if "Charismatic" in traits: happiness += 2
    if "Devout" in traits:
        gov_rel = city.get("_governor_religion", "")
        rel = _get_city_religion(city)
        if rel and gov_rel and rel.get(gov_rel, 0) >= 40:
            happiness += 2

    # Protected ethnic groups give loyalty
    laws = _get_faction_laws(owner)
    demos = _get_city_demographics(city)
    protected_pct = sum(pct for g, pct in demos.items() if laws["ethnic"].get(g, 0) == 0)
    happiness += min(3, protected_pct // 30)  # up to +3 for mostly protected population

    # Free Faith religions give contentment
    religion = _get_city_religion(city)
    free_rel_pct = sum(pct for r, pct in religion.items() if laws["religion"].get(r, 0) == 0)
    happiness += min(2, free_rel_pct // 50)  # up to +2

    # === DRAINS ===
    # Governor negative traits
    if "Warmonger" in traits: happiness -= 1
    if "Tyrant" in traits: happiness -= 2

    # Production policy pressure (capped to prevent death spirals)
    _gp_name, _gp_mod, _gp_hap = _get_gold_policy(owner)
    _fp_name, _fp_mod, _fp_hap = _get_food_policy(owner)
    policy_drain = min(0, _gp_hap) + min(0, _fp_hap)  # only count negatives
    happiness += max(-6, policy_drain)  # cap total policy drain at -6/month
    # Conscription happiness drain
    _con_name, _con_rate, _con_hap = _get_conscription_policy(owner)
    happiness += _con_hap

    # Ethnic law drains (capped per group to prevent stacking doom)
    ethnic_drain = 0
    for group, pct in demos.items():
        status = laws["ethnic"].get(group, 0)
        if status == 1:  # Segregated
            ethnic_drain -= max(0, pct // 50)  # -1 per 50%
        elif status == 2:  # Suppressed
            ethnic_drain -= max(0, pct // 30)  # -1 per 30%
        elif status == 3:  # Enslaved
            ethnic_drain -= max(1, pct // 20)  # -1 per 20%
    happiness += max(-8, ethnic_drain)  # cap total ethnic drain at -8/month

    # Religion law drains
    rel_drain = 0
    for rel_name, pct in religion.items():
        status = laws["religion"].get(rel_name, 0)
        if status == 2:  # Suppressed
            rel_drain -= max(0, pct // 40)
        elif status == 3:  # Banned
            rel_drain -= max(0, pct // 25)
    happiness += max(-4, rel_drain)  # cap religion drain at -4/month

    # Wartime stress (only if at war, not stacking with siege)
    if at_war_now and not under_siege:
        happiness -= 1

    # Clamp 0-100
    city["_happiness"] = max(0, min(100, happiness))

faction_images = sprites.faction_images
_brit_anims = sprites.infantry_anims.get("Great Britain", {})
_fren_anims = sprites.infantry_anims.get("France", {})
_rus_anims = sprites.infantry_anims.get("Russia", {})
_spa_anims = sprites.infantry_anims.get("Spain", {})
_usa_anims = sprites.infantry_anims.get("United States", {})
_pir_anims = sprites.infantry_anims.get("Pirates", {})
_iro_anims = sprites.infantry_anims.get("Iroquois", {})
_wab_anims = sprites.infantry_anims.get("Wabanaki", {})
_cree_anims = sprites.infantry_anims.get("Cree", {})
_canoe_anims = sprites.canoe_anims
_brit_cav = sprites.cavalry_anims.get("Great Britain", {})
_fren_cav = sprites.cavalry_anims.get("France", {})
_rus_cav = sprites.cavalry_anims.get("Russia", {})
_spa_cav = sprites.cavalry_anims.get("Spain", {})
_usa_cav = sprites.cavalry_anims.get("United States", {})
_den_anims = sprites.infantry_anims.get("Denmark", {})
_den_cav = sprites.cavalry_anims.get("Denmark", {})
_ship_anims = sprites.ship_anims
_merch_anims = sprites.merchant_anims
_natmerch_anims = sprites.native_merchant_anims
_haiti_merch_anims = sprites.haiti_merchant_anims
explosion_frames = sprites.explosion_frames

# ====================================================================
# === ECONOMY & UNIT DEFINITIONS =====================================
# ====================================================================
explosion_frame_count = sprites.explosion_frame_count
explosion_anim_speed = sprites.explosion_anim_speed
_brit_anim_tick = 0
_explosion_tick = 0
def get_explosion_frame(size): return sprites.get_explosion_frame(size)
star_points = config.STAR_POINTS
strategies = config.STRATEGIES; formations = config.FORMATIONS
materials = config.MATERIALS; city_materials = config.CITY_MATERIALS
FORMATION_COSTS = config.FORMATION_COSTS
FORT_COST = config.FORT_COST; FORT_CAP = config.FORT_CAP

# === Building System ===
# tier_req: 0 = no tier needed, 1 = Tier I, 2 = Tier II, 3 = Tier III
BUILDINGS = {
    "Town Hall": {"cost": {"Gold": 200, "Lumber": 30}, "build_days": 30, "colonial_only": True, "tier_req": 0},
    "Council Lodge": {"cost": {"Food": 40, "Lumber": 20, "Hide": 10}, "build_days": 30, "native_only": True, "tier_req": 0},
    "Road": {"cost": {"Gold": 150, "Lumber": 15}, "build_days": 20, "colonial_only": True, "tier_req": 0},
    "Market": {"cost": {"Gold": 120, "Lumber": 20}, "build_days": 15, "colonial_only": False, "tier_req": 1},
    "Warehouse": {"cost": {"Gold": 80, "Lumber": 30, "Iron": 3}, "build_days": 20, "colonial_only": False, "tier_req": 1},
    "Lumber Mill": {"cost": {"Gold": 60, "Lumber": 15, "Iron": 2}, "build_days": 10, "colonial_only": False, "tier_req": 1},
    "Stables": {"cost": {"Gold": 150, "Lumber": 25, "Hide": 10}, "build_days": 20, "colonial_only": True, "tier_req": 1},
    "Barracks": {"cost": {"Gold": 180, "Lumber": 20, "Iron": 3}, "build_days": 25, "colonial_only": True, "tier_req": 1},
    "Church": {"cost": {"Gold": 100, "Lumber": 20}, "build_days": 15, "colonial_only": True, "tier_req": 1},
    "Plantation": {"cost": {"Gold": 300, "Lumber": 30, "Iron": 3}, "build_days": 30, "colonial_only": True, "tier_req": 2},
    "Fur Trading Post": {"cost": {"Gold": 250, "Lumber": 25}, "build_days": 25, "colonial_only": False, "tier_req": 2},
    "Shipyard": {"cost": {"Gold": 300, "Lumber": 40, "Iron": 5}, "build_days": 35, "colonial_only": True, "tier_req": 2},
    "Foundry": {"cost": {"Gold": 250, "Lumber": 20, "Iron": 8, "Coal": 5}, "build_days": 30, "colonial_only": True, "tier_req": 2},
    "Farm": {"cost": {"Gold": 80, "Lumber": 20, "Hide": 5}, "build_days": 12, "colonial_only": False, "tier_req": 0},
}

# === Road Network ===
_roads = []  # list of (city_a, city_b, path) — path is list of (x,y) waypoints

def _city_has_road(city):
    """Check if a city has a Road built."""
    return "Road" in city.get("buildings", [])

def _road_segment(ax, ay, bx, by, max_off_steps=6):
    """Connect two nearby points with a mostly-straight land path, nudging around water.
    Returns a list of points (excluding the very first, which the caller already has),
    or None if it can't stay on land between them."""
    seg_dist = math.hypot(bx-ax, by-ay)
    steps = max(2, int(seg_dist / 5))
    pts = []
    dx, dy = bx-ax, by-ay
    length = math.hypot(dx, dy) or 1
    perp_x, perp_y = -dy/length, dx/length
    for i in range(1, steps):
        t = i / steps
        px, py = ax + dx*t, ay + dy*t
        if not _is_water(px, py):
            pts.append((px, py)); continue
        found = False
        for offset in range(1, max_off_steps+1):
            for side in (1, -1):
                nx = px + perp_x * offset * side * 3
                ny = py + perp_y * offset * side * 3
                if 0 < nx < _MAP_NATIVE_W and 0 < ny < _MAP_NATIVE_H and not _is_water(nx, ny):
                    pts.append((nx, ny)); found = True; break
            if found: break
        if not found:
            return None
    pts.append((bx, by))
    return pts

def _find_road_path(ax, ay, bx, by):
    """Route a road from A to B *through the regions* of the map.

    Instead of a single straight line, the road walks the chain of adjacent territories
    between the two cities (region centroid to region centroid), so it visibly traverses
    the map region by region. Each hop between region centers is connected with a
    land-following segment. Falls back to a direct land route if region data is missing
    or the territories aren't connected on land."""
    straight_dist = math.hypot(bx-ax, by-ay)

    # Determine the region chain between the two endpoints.
    ra = _region_at(ax, ay)
    rb = _region_at(bx, by)
    waypoints = None
    if ra >= 0 and rb >= 0:
        route = _find_region_route(ra, rb)
        if route:
            waypoints = []
            for rid in route:
                cent = _region_centroids.get(rid)
                if cent:
                    wx, wy = _region_grid_to_world(*cent)
                    waypoints.append((wx, wy))

    # Build the anchor list: A -> region centroids -> B. Drop centroids that sit on water.
    anchors = [(ax, ay)]
    if waypoints:
        for wx, wy in waypoints:
            if not _is_water(wx, wy):
                anchors.append((wx, wy))
    anchors.append((bx, by))
    # Remove near-duplicate consecutive anchors
    dedup = [anchors[0]]
    for p in anchors[1:]:
        if math.hypot(p[0]-dedup[-1][0], p[1]-dedup[-1][1]) > 4:
            dedup.append(p)
    anchors = dedup

    # Stitch land-following segments between consecutive anchors.
    path = [anchors[0]]
    for i in range(len(anchors)-1):
        seg = _road_segment(*anchors[i], *anchors[i+1])
        if seg is None:
            # Region hop crossed water — try a direct segment as a fallback for this leg
            seg = _road_segment(*anchors[i], *anchors[i+1], max_off_steps=10)
            if seg is None:
                return None
        path.extend(seg)

    # Reject absurdly long routes (guards against pathological region detours).
    total_len = sum(math.hypot(path[j+1][0]-path[j][0], path[j+1][1]-path[j][1]) for j in range(len(path)-1))
    if straight_dist > 1 and total_len > straight_dist * 4.0:
        return None
    return path

def _get_road_connections():
    """Build list of road connections between neighboring cities with roads. Max 2 roads per city."""
    global _roads
    _roads = []
    road_cities = [c for c in cities if _city_has_road(c) and c["owner"] not in NATIVE_FACTIONS]
    # Track how many connections each city has (max 2)
    connection_count = {id(c): 0 for c in road_cities}
    # Sort potential connections by distance (shortest first) so cities connect to nearest neighbors
    candidates = []
    for i, a in enumerate(road_cities):
        for b in road_cities[i+1:]:
            dist = math.hypot(a["x"]-b["x"], a["y"]-b["y"])
            if dist > 100: continue
            candidates.append((dist, a, b))
    candidates.sort(key=lambda x: x[0])
    for dist, a, b in candidates:
        if connection_count[id(a)] >= 2 or connection_count[id(b)] >= 2: continue
        path = _find_road_path(a["x"], a["y"], b["x"], b["y"])
        if path and len(path) >= 2:
            _roads.append((a, b, path))
            connection_count[id(a)] += 1
            connection_count[id(b)] += 1

def _on_road(wx, wy):
    """Check if a world position is near a road path (+25% speed bonus)."""
    for a, b, path in _roads:
        for j in range(len(path)-1):
            x1, y1 = path[j]; x2, y2 = path[j+1]
            dx, dy = x2-x1, y2-y1
            seg_len_sq = dx*dx + dy*dy
            if seg_len_sq < 1: continue
            t = max(0, min(1, ((wx-x1)*dx + (wy-y1)*dy) / seg_len_sq))
            px, py = x1 + t*dx, y1 + t*dy
            if math.hypot(wx-px, wy-py) < 5:
                return True
    return False

def _draw_roads():
    """Draw brown road paths between connected cities, weaving around obstacles."""
    for a, b, path in _roads:
        for j in range(len(path)-1):
            sx1, sy1 = world_to_screen(path[j][0], path[j][1])
            sx2, sy2 = world_to_screen(path[j+1][0], path[j+1][1])
            pygame.draw.line(screen, (139, 90, 43), (sx1, sy1), (sx2, sy2), max(1, int(2*zoom)))
# Native Market cost override (no gold)
_NATIVE_MARKET_COST = {"Food": 20, "Lumber": 15, "Hide": 5}
_NATIVE_WAREHOUSE_COST = {"Lumber": 30, "Hide": 10}
_NATIVE_LUMBER_MILL_COST = {"Lumber": 15, "Hide": 5}
_NATIVE_PLANTATION_COST = {"Food": 60, "Lumber": 30, "Hide": 15}
_NATIVE_FUR_POST_COST = {"Food": 40, "Lumber": 20, "Hide": 10}

# Building descriptions for UI
_BUILDING_DESCS = {
    "Town Hall": "+25% gold tax, enables upgrades",
    "Council Lodge": "+25% gold tax, enables upgrades",
    "Road": "+25% unit speed, connects cities",
    "Market": "+25% merchant income, +1 merchant cap",
    "Warehouse": "+100 resource storage",
    "Lumber Mill": "2x lumber production",
    "Plantation": "Produces luxury (Sugar/Tobacco/Coffee/Cotton/Cocoa)",
    "Fur Trading Post": "Produces luxury (Fur)",
}

# === Luxury Resources ===
LUXURY_RESOURCES = ["Sugar", "Tobacco", "Coffee", "Cotton", "Cocoa", "Fur"]
# Which luxury a Plantation produces based on biome
_PLANTATION_LUXURY = {
    "tropical": random.choice(["Sugar", "Coffee", "Cocoa"]),  # determined per-city at runtime
    "swamp": "Sugar",
    "temperate": random.choice(["Tobacco", "Cotton"]),
    "grassland": "Cotton",
}
def _get_plantation_luxury(biome):
    """Get which luxury resource a Plantation produces in a given biome."""
    if biome == "tropical": return random.choice(["Sugar", "Coffee", "Cocoa"])
    if biome == "swamp": return "Sugar"
    if biome == "temperate": return random.choice(["Tobacco", "Cotton"])
    if biome == "grassland": return "Cotton"
    return None  # can't build plantation here

def _get_fur_post_luxury(biome):
    """Fur Trading Post only works in taiga/tundra."""
    if biome in ("taiga", "tundra"): return "Fur"
    return None

def _city_luxury_resource(city):
    """Get the luxury resource produced by a city, or None. Caches result."""
    if "_luxury_cache" in city:
        return city["_luxury_cache"]
    biome = _get_biome(city["x"], city["y"])
    lux = None
    if _city_has_building(city, "Plantation"):
        lux = _get_plantation_luxury(biome)
    if not lux and _city_has_building(city, "Fur Trading Post"):
        lux = _get_fur_post_luxury(biome)
    city["_luxury_cache"] = lux
    return lux

def _can_build_plantation(city):
    """Check if biome supports a Plantation."""
    biome = _get_biome(city["x"], city["y"])
    return biome in ("tropical", "swamp", "temperate", "grassland")

def _can_build_fur_post(city):
    """Check if biome supports a Fur Trading Post."""
    biome = _get_biome(city["x"], city["y"])
    return biome in ("taiga", "tundra")

# === Storage System ===
BASE_STORAGE_CAP = 100

def _get_storage_cap(faction):
    """Get total storage cap for a faction: 100 base + 100 per Warehouse."""
    warehouse_count = sum(1 for c in cities if c["owner"]==faction and "Warehouse" in c.get("buildings", []))
    return BASE_STORAGE_CAP + warehouse_count * 100

def _clamp_resources(faction):
    """Cap resources at storage limit. Gold and Food cap at 2000."""
    cap = _get_storage_cap(faction)
    mats = faction_materials[faction]
    for mat in materials:
        if mat == "Gold" or mat == "Food":
            if mats[mat] > 2000: mats[mat] = 2000
        else:
            if mats[mat] > cap: mats[mat] = cap

def _city_has_building(city, name):
    return name in city.get("buildings", [])

def _start_construction(city, building_name):
    """Start constructing a building in a city."""
    if city.get("is_fort") or city.get("is_camp"): return False  # forts/camps can't build
    if city.get("construction"): return False  # already building something
    if _city_has_building(city, building_name): return False  # already built
    bdata = BUILDINGS[building_name]
    owner = city["owner"]
    is_native = owner in NATIVE_FACTIONS
    # Colonial-only buildings blocked for natives, native-only blocked for colonials
    if bdata.get("colonial_only") and is_native: return False
    if bdata.get("native_only") and not is_native: return False
    # Prerequisite: Town Hall (colonial) or Council Lodge (native) required for non-tier-0 buildings
    has_hall = _city_has_building(city, "Council Lodge") if is_native else _city_has_building(city, "Town Hall")
    if building_name not in ("Town Hall", "Council Lodge", "Road") and not has_hall: return False
    # Tier requirement check
    tier_req = bdata.get("tier_req", 0)
    if city.get("tier", 0) < tier_req: return False
    # Biome restrictions for Plantation and Fur Trading Post
    if building_name == "Plantation" and not _can_build_plantation(city): return False
    if building_name == "Fur Trading Post" and not _can_build_fur_post(city): return False
    # Roads can't be built on islands
    if building_name == "Road" and city["name"] in _ISLAND_CITY_NAMES: return False
    # Lumber Mill only for settlements producing lumber
    if building_name == "Lumber Mill" and city["material"] != "Lumber": return False
    # Determine cost (natives use alternate costs for Market/Warehouse/Plantation/Fur Post)
    if is_native and building_name == "Market":
        cost = _NATIVE_MARKET_COST
    elif is_native and building_name == "Warehouse":
        cost = _NATIVE_WAREHOUSE_COST
    elif is_native and building_name == "Lumber Mill":
        cost = _NATIVE_LUMBER_MILL_COST
    elif is_native and building_name == "Plantation":
        cost = _NATIVE_PLANTATION_COST
    elif is_native and building_name == "Fur Trading Post":
        cost = _NATIVE_FUR_POST_COST
    else:
        cost = bdata["cost"]
    if can_afford(owner, cost):
        pay(owner, cost)
        build_ticks = bdata["build_days"] * DAY_TICKS
        # Architect trait: 25% less construction time
        traits = city.get("_governor_traits", [])
        if "Architect" in traits: build_ticks = int(build_ticks * 0.75)
        # Lazy trait: 25% more construction time
        if "Lazy" in traits: build_ticks = int(build_ticks * 1.25)
        city["construction"] = {"name": building_name, "ticks_left": build_ticks}
        return True
    return False

def update_construction():
    """Tick construction progress for all cities."""
    for city in cities:
        if not city.get("construction"): continue
        city["construction"]["ticks_left"] -= game_speed
        if city["construction"]["ticks_left"] <= 0:
            bname = city["construction"]["name"]
            if "buildings" not in city: city["buildings"] = []
            city["buildings"].append(bname)
            city["construction"] = None
            # Rebuild road network when a Road is completed
            if bname == "Road": _get_road_connections()

def _ai_try_build(city):
    """AI logic: try to build useful buildings based on tier."""
    owner = city["owner"]
    if city.get("construction"): return
    if city.get("is_fort") or city.get("is_camp"): return
    tier = city.get("tier", 0)
    if owner in NATIVE_FACTIONS:
        # Natives: Council Lodge (tier 0), then tier 1+ buildings
        if not _city_has_building(city, "Council Lodge"):
            _start_construction(city, "Council Lodge")
        elif tier >= 1 and not _city_has_building(city, "Lumber Mill") and city["material"] == "Lumber":
            _start_construction(city, "Lumber Mill")
        elif tier >= 1 and not _city_has_building(city, "Market"):
            _start_construction(city, "Market")
        elif tier >= 1 and not _city_has_building(city, "Warehouse"):
            _start_construction(city, "Warehouse")
        elif tier >= 2 and not _city_has_building(city, "Fur Trading Post") and _can_build_fur_post(city):
            _start_construction(city, "Fur Trading Post")
    else:
        # Colonial: tier 0 buildings first, then tier 1, then tier 2
        if not _city_has_building(city, "Town Hall"):
            _start_construction(city, "Town Hall")
        elif not _city_has_building(city, "Farm"):
            _start_construction(city, "Farm")
        elif not _city_has_building(city, "Road") and city["name"] not in _ISLAND_CITY_NAMES:
            _start_construction(city, "Road")
        elif tier >= 1 and not _city_has_building(city, "Lumber Mill") and city["material"] == "Lumber":
            _start_construction(city, "Lumber Mill")
        elif tier >= 1 and not _city_has_building(city, "Market"):
            _start_construction(city, "Market")
        elif tier >= 1 and not _city_has_building(city, "Stables"):
            _start_construction(city, "Stables")
        elif tier >= 1 and not _city_has_building(city, "Barracks"):
            _start_construction(city, "Barracks")
        elif tier >= 1 and not _city_has_building(city, "Church"):
            _start_construction(city, "Church")
        elif tier >= 1 and not _city_has_building(city, "Warehouse"):
            _start_construction(city, "Warehouse")
        elif tier >= 2 and not _city_has_building(city, "Foundry"):
            _start_construction(city, "Foundry")
        elif tier >= 2 and not _city_has_building(city, "Plantation") and _can_build_plantation(city):
            _start_construction(city, "Plantation")
        elif tier >= 2 and not _city_has_building(city, "Fur Trading Post") and _can_build_fur_post(city):
            _start_construction(city, "Fur Trading Post")
        elif tier >= 2 and not _city_has_building(city, "Shipyard"):
            _start_construction(city, "Shipyard")

faction_materials = {f: {m: 0 for m in materials + LUXURY_RESOURCES} for f in FACTIONS}
# Starting resources so factions can act immediately
for _f in FACTIONS:
    for _mat, _val in config.STARTING_RESOURCES.items():
        faction_materials[_f][_mat] = _val

# === Nation Focus/Personality System ===
# Focuses: "Aggressive" (military), "Economic" (building/trade), "Neutral" (balanced)
FOCUSES = ["Aggressive", "Economic", "Neutral"]
_faction_focus = {}  # faction -> current focus
_faction_focus_timer = {}  # faction -> ticks until next focus switch

def _init_faction_focuses():
    for f in FACTIONS:
        _faction_focus[f] = random.choice(FOCUSES)
        _faction_focus_timer[f] = random.randint(DAY_TICKS * 30, DAY_TICKS * 1095)  # 30 days to 3 years
_init_faction_focuses()

def _get_faction_focus(faction):
    return _faction_focus.get(faction, "Neutral")

def update_faction_focuses():
    """Tick focus timers and randomly switch focus when timer expires."""
    for f in list(_faction_focus_timer):
        if f in eliminated_factions: continue
        _faction_focus_timer[f] -= game_speed
        if _faction_focus_timer[f] <= 0:
            _faction_focus[f] = random.choice(FOCUSES)
            _faction_focus_timer[f] = random.randint(DAY_TICKS * 30, DAY_TICKS * 1095)

def _get_unit_cap(faction): return sum(1 for c in cities if c["owner"]==faction and not c.get("is_fort") and not c.get("is_camp"))
MERCHANT_CAP = config.MERCHANT_CAP; NATIVE_MERCHANT_CAP = config.NATIVE_MERCHANT_CAP
def _get_merchant_cap(faction):
    if faction in NATIVE_FACTIONS: return 1
    # Base cap is 1, +1 per Market building in faction cities, +1 per Merchant governor trait
    market_count = sum(1 for c in cities if c["owner"]==faction and _city_has_building(c, "Market"))
    merchant_trait_bonus = sum(1 for c in cities if c["owner"]==faction and "Merchant" in c.get("_governor_traits", []))
    return 1 + market_count + merchant_trait_bonus
def _gen_regiment_name(faction):
    if faction == "Comanche": return "Horsemen"
    if faction == "Dakota": return random.choice(["Warrior", "Bowmen"])
    if faction == "Cree": return "Warrior"
    if faction in NATIVE_FACTIONS: return "Warrior"
    if faction == "France": return random_french_regiment()
    if faction == "Spain": return random_spanish_regiment()
    if faction == "Great Britain": return random_british_regiment()
    if faction == "Russia": return random_russian_regiment()
    if faction == "United States": return random_american_regiment()
    if faction == "Denmark": return random_danish_regiment()
    if faction == "Mexico": return f"{random.randint(1,20)}o Regimiento de México"
    if faction == "Haiti": return f"{random.randint(1,15)}e Régiment d'Haïti"
    if faction == "Texas": return f"{random.randint(1,10)}th Texas Rangers"
    if faction == "Confederate States": return f"{random.randint(1,30)}th Confederate Regiment"
    if faction == "Pirates": return random.choice(["Blackbeard's Crew","Jolly Roger Band","Sea Dogs","Corsairs","Buccaneers","Marauders","Cutthroats","Freebooters"])

# ====================================================================
# === NEWS SYSTEM ====================================================
# ====================================================================
    return "Unknown Regiment"
factions_panel_open = True; news_panel_open = True; news_log = []; NEWS_VISIBLE = 5; news_scroll = 0
def news(msg):
    global news_scroll; news_log.append(msg); news_scroll = 0
def draw_news():
    W, H = get_screen_size(); panel_w, line_h, header_h = 420, 20, 24
    panel_h = (header_h + NEWS_VISIBLE*line_h + 10) if news_panel_open else header_h
    npx, npy = W-panel_w-6, 6
    panel = pygame.Surface((panel_w, panel_h), pygame.SRCALPHA); panel.fill((0,0,0,170)); screen.blit(panel, (npx, npy))
    _draw_panel_frame(npx, npy, panel_w, panel_h)
    draw_outlined_text(news_font, "NEWS:", (255,220,50), (npx+6, npy+4))
    arrow = "\u25BC" if news_panel_open else "\u25B2"
    btn_rect = pygame.Rect(npx+panel_w-22, npy+4, 18, 18)
    pygame.draw.rect(screen, (60,60,60), btn_rect, border_radius=3)
    draw_outlined_text(news_font, arrow, (255,220,50), (btn_rect.centerx, btn_rect.centery), anchor="center")
    if news_panel_open:
        total = len(news_log); max_scroll = max(0, total-NEWS_VISIBLE)
        scroll = max(0, min(news_scroll, max_scroll)); start_idx = max(0, total-NEWS_VISIBLE-scroll)
        for i, msg in enumerate(news_log[start_idx:start_idx+NEWS_VISIBLE]):
            draw_outlined_text(news_font, msg, (220,220,220), (npx+6, npy+header_h+i*line_h))

# ====================================================================
# === DISASTERS & WEATHER ============================================
# ====================================================================
        if max_scroll > 0: draw_outlined_text(small_font, f"scroll \u2191\u2193 ({scroll}/{max_scroll})", (160,160,160), (npx+6, npy+panel_h-14))
    return btn_rect
DISASTERS = [("Hurricane",60,40,"\U0001f300"),("Tornado",40,25,"\U0001f32a"),("Storm",25,35,"\u26c8"),("Earthquake",50,30,"\U0001f4a5"),("Blizzard",20,45,"\u2744")]
DISASTER_CHANCE = config.DISASTER_CHANCE; SPRING_FLOOD_CHANCE = config.SPRING_FLOOD_CHANCE; SUMMER_STORM_CHANCE = config.SUMMER_STORM_CHANCE; flooded_cities = {}
def trigger_disaster(city):
    kind, troop_dmg, radius, icon = random.choice(DISASTERS)
    troop_dmg = random.randint(troop_dmg//2, troop_dmg); city["troops"] = max(1, city["troops"]-troop_dmg); killed = 0
    for u in units[:]:
        if math.hypot(u.x-city["x"], u.y-city["y"]) < radius and u in units: units.remove(u); killed += 1
    # Population loss from disaster (2-8%)
    if "_population" in city:
        loss_pct = random.uniform(0.02, 0.08)
        city["_population"] = max(50, int(city["_population"] * (1 - loss_pct)))
    # Happiness loss from disaster
    city["_happiness"] = max(0, city.get("_happiness", 100) - random.randint(5, 15))
    msg = f"{icon} {kind} strikes {city['name']}! -{troop_dmg} troops"
    if killed: msg += f", {killed} units lost"
    news(msg)
def trigger_flood(city):
    flooded_cities[id(city)] = DAY_TICKS * 30; city["troops"] = max(1, city["troops"]-random.randint(10, 30))
    city["_happiness"] = max(0, city.get("_happiness", 100) - random.randint(5, 12))
    news(f"\U0001f30a Flood hit {city['name']}! Production halted for 1 month.")
def trigger_summer_storm(city):
    dmg = random.randint(15, 40); city["troops"] = max(1, city["troops"]-dmg); killed = 0
    for u in units[:]:
        if math.hypot(u.x-city["x"], u.y-city["y"]) < 40:
            u.hp -= random.randint(10, 25)
            if u.hp <= 0: u.hp = max(1, u.hp); u._start_retreat_to_city(); killed += 1
    city["_happiness"] = max(0, city.get("_happiness", 100) - random.randint(3, 8))
    msg = f"\u26c8 Summer storm hits {city['name']}! -{dmg} troops"
    if killed: msg += f", {killed} units damaged"
    news(msg)
def update_floods():
    for cid in list(flooded_cities):
        flooded_cities[cid] -= 1
        if flooded_cities[cid] <= 0: del flooded_cities[cid]
def is_flooded(city): return id(city) in flooded_cities
PIRATE_TARGETS = ["Havana","Nassau","Kingston","Port-au-Prince","Caracas"]
_pirate_spawn_cd = 0
_pirates_have_spawned = False

# ====================================================================
# === PIRATES ========================================================
# ====================================================================
def _check_pirate_spawn():
    global _pirate_spawn_cd, _pirates_have_spawned
    if game_year >= 1830: return  # pirates stop appearing after 1830
    if _pirate_spawn_cd > 0: _pirate_spawn_cd -= 1; return
    # Pirates can spawn if eliminated or not yet in game
    pirates_alive = "Pirates" not in eliminated_factions and any(c["owner"]=="Pirates" for c in cities)
    if pirates_alive: return
    # Random chance to spawn each check (1-5% — very unpredictable timing)
    if random.random() > random.uniform(0.01, 0.05): return
    # Pick a target city to take over
    target_city = None
    random.shuffle(PIRATE_TARGETS)
    for name in PIRATE_TARGETS:
        c = next((c for c in cities if c["name"]==name), None)
        if c and c["owner"] != "Pirates":
            target_city = c; break
    if not target_city: return
    # Remove from eliminated if respawning
    eliminated_factions.discard("Pirates"); _surrendered.discard("Pirates")
    # Add Pirates to factions if not already
    if "Pirates" not in FACTIONS: FACTIONS.append("Pirates")
    if "Pirates" not in faction_materials: faction_materials["Pirates"] = {m: 0 for m in materials}
    faction_materials["Pirates"]["Gold"] = 15; faction_materials["Pirates"]["Food"] = 15; faction_materials["Pirates"]["Hide"] = 10
    # Seize the city
    old_owner = target_city["owner"]
    target_city["owner"] = "Pirates"
    target_city["occupier"] = None; target_city["color"] = (20,20,20); target_city["is_capital"] = True
    # Spawn two Tier III infantry units
    for _ in range(2):
        u = Unit(target_city, target_city, "Line"); u.tier = 3; u.power += 6; u.max_hp += 30; u.hp = u.max_hp; u.idle = True; units.append(u)
    # Declare war only on the faction whose city was taken
    if old_owner and old_owner != "Pirates":
        declare_war("Pirates", old_owner, "")
    news(f"Pirates seize {target_city['name']} from {old_owner}!")
    _pirates_have_spawned = True
    _pirate_spawn_cd = DAY_TICKS * random.randint(100, 600)  # very random respawn cooldown
def check_disasters():
    for city in cities:
        if random.random() < DISASTER_CHANCE: trigger_disaster(city)
treaties, war_pressure, treaty_cooldowns = {}, {}, {}
# ====================================================================
# === WARFARE & DIPLOMACY SYSTEM (see warfare.py for documentation) ===
# ====================================================================
TREATY_MIN, TREATY_MAX, TREATY_COOLDOWN, STALEMATE_THRESHOLD = config.TREATY_MIN_TICKS, config.TREATY_MAX_TICKS, config.TREATY_COOLDOWN_TICKS, config.STALEMATE_THRESHOLD_TICKS
def pair(a, b): return frozenset({a, b})
wars = set(); WAR_CHECK_TIMER_HZ = 60*10; WAR_DECLARE_BASE = 0.008
game_state.wars = wars; game_state.treaties = treaties; game_state.war_pressure = war_pressure
def at_war(a, b):
    if _puppets.get(a) == b or _puppets.get(b) == a: return False  # bloc never at war
    oa, ob = _puppets.get(a), _puppets.get(b)
    if oa is not None and oa == ob: return False
    return pair(a, b) in wars
def at_peace(a, b):
    if _puppets.get(a) == b or _puppets.get(b) == a: return True
    oa, ob = _puppets.get(a), _puppets.get(b)
    if oa is not None and oa == ob: return True
    if pair(a, b) in wars: return False
    return pair(a, b) in treaties or pair(a, b) in alliances or not at_war(a, b)
# === Puppet System ===
# A puppet is a subordinate state controlled by an overlord. Puppets never fight their
# overlord (or fellow puppets of the same overlord) and are dragged into the overlord's
# peace deals. { puppet_faction : overlord_faction }
_puppets = {}
game_state.puppets = _puppets

def set_puppet(puppet, overlord):
    """Make `puppet` a puppet of `overlord`."""
    _puppets[puppet] = overlord
    # Ensure they aren't at war and are aligned
    wars.discard(pair(puppet, overlord))

def overlord_of(faction):
    """Return the overlord of a faction, or None if it isn't a puppet."""
    return _puppets.get(faction)

def puppets_of(overlord):
    """Return the list of factions puppeted by `overlord`."""
    return [p for p, o in _puppets.items() if o == overlord]

def same_bloc(a, b):
    """True if a and b are the same faction, overlord+puppet, or two puppets of one overlord."""
    if a == b: return True
    if _puppets.get(a) == b or _puppets.get(b) == a: return True
    oa, ob = _puppets.get(a), _puppets.get(b)
    if oa is not None and oa == ob: return True
    return False

def declare_war(a, b, reason=""):
    key = pair(a, b)
    # Puppets never go to war within their own bloc (overlord / fellow puppets).
    if same_bloc(a, b): return
    # Max 3 wars per nation
    wars_a = sum(1 for w in wars if a in w)
    wars_b = sum(1 for w in wars if b in w)
    if wars_a >= 3 or wars_b >= 3: return
    if key in alliances: del alliances[key]; news(f"Alliance between {a} & {b} broken!")
    wars.add(key)
    if reason: news(reason)
def end_war(a, b):
    wars.discard(pair(a, b))
    # When an overlord makes peace, its puppets end that war too (and vice-versa).
    bloc_a = [a] + ([overlord_of(a)] if overlord_of(a) else []) + puppets_of(a) + puppets_of(overlord_of(a) or a)
    bloc_b = [b] + ([overlord_of(b)] if overlord_of(b) else []) + puppets_of(b) + puppets_of(overlord_of(b) or b)
    for x in set(filter(None, bloc_a)):
        for y in set(filter(None, bloc_b)):
            wars.discard(pair(x, y))
# Territorial demand system — factions can demand their original lands back
refused_demands = {}  # frozenset({a,b}) -> count of refused demands (feeds war score)
def check_territorial_demands():
    """Factions demand return of cities that were originally theirs but now held by another (non-war) faction."""
    af = active_factions()
    for demander in af:
        if demander == "Pirates": continue
        for city in cities:
            # City's sovereign is the demander but someone else owns it (was taken in a past war)
            if city["sovereign"] == demander and city["owner"] != demander and city["owner"] != "Pirates":
                holder = city["owner"]
                if holder not in af: continue
                key = pair(demander, holder)
                if key in wars or key in treaties: continue
                # Only demand occasionally (0.5% per check per city)
                if random.random() > 0.005: continue
                # Holder decides: return or refuse based on relative strength
                holder_units = sum(1 for u in units if u.owner["owner"]==holder)
                demander_units = sum(1 for u in units if u.owner["owner"]==demander)
                # More likely to return if demander is stronger or if holder has many cities
                holder_cities = sum(1 for c in cities if c["owner"]==holder)
                return_chance = 0.3
                if demander_units > holder_units * 1.5: return_chance += 0.3
                if holder_cities > 5: return_chance += 0.2
                if key in alliances: return_chance += 0.3
                if random.random() < return_chance:
                    # Accept — return the city
                    city["owner"] = demander; city["sovereign"] = demander
                    city["color"] = FACTION_COLORS[demander]; city["occupier"] = None
                    news(f"{holder} returns {city['name']} to {demander}.")
                    refused_demands.pop(key, None)
                else:
                    # Refuse — adds to war justification score
                    refused_demands[key] = refused_demands.get(key, 0) + 1
                    news(f"{holder} refuses to return {city['name']} to {demander}!")
                break  # only one demand per faction per check
def check_war_declarations():
    """Point-based war decision. Score must exceed 50 to declare war."""
    af = active_factions()
    for i, a in enumerate(af):
        for b in af[i+1:]:
            key = pair(a, b)
            if key in wars or key in treaties or key in alliances: continue
            cities_a = [c for c in cities if c["owner"]==a]; cities_b = [c for c in cities if c["owner"]==b]
            if not cities_a or not cities_b: continue
            min_dist = min(math.hypot(ca["x"]-cb["x"], ca["y"]-cb["y"]) for ca in cities_a for cb in cities_b)
            war_range = 100 if (a in NATIVE_FACTIONS or b in NATIVE_FACTIONS) else 600
            if min_dist > war_range: continue
            units_a = sum(1 for u in units if u.owner["owner"]==a)
            units_b = sum(1 for u in units if u.owner["owner"]==b)
            for attacker, defender, u_atk, u_def in [(a,b,units_a,units_b),(b,a,units_b,units_a)]:
                if u_atk == 0: continue
                atk_cities = [c for c in cities if c["owner"]==attacker]
                def_cities = [c for c in cities if c["owner"]==defender]
                score = 0; reason = ""
                # Territorial claim — nearby enemy city (+30)
                claimed_city = None
                for ca in atk_cities:
                    for cb in def_cities:
                        if math.hypot(ca["x"]-cb["x"], ca["y"]-cb["y"]) < 150:
                            score += 30; claimed_city = cb["name"]; break
                    if claimed_city: break
                # Resource scarcity — need what they have (+25)
                for mat in city_materials:
                    if faction_materials[attacker][mat] < 3 and any(c["material"]==mat for c in def_cities):
                        score += 25; break
                # Revenge — they hold our sovereign territory (+35)
                if any(c["sovereign"]==attacker and c["owner"]==defender for c in cities): score += 35
                # Refused territorial demands (+25 per refusal)
                demand_key = pair(attacker, defender)
                score += refused_demands.get(demand_key, 0) * 25
                # Defender already at war with someone else (+30)
                if any(at_war(defender, x) for x in af if x != attacker and x != defender): score += 30
                # Attacker has allies (+15)
                if any(pair(attacker, x) in alliances for x in af if x != defender): score += 15
                # Military advantage: more units (+20), fewer units (-30)
                if u_atk > u_def: score += 20
                elif u_def >= u_atk * 2: score -= 40
                elif u_def > u_atk: score -= 20
                # Defender army is much stronger (-30)
                if u_def >= u_atk * 3: score -= 30
                # Low resources penalizes aggression (-15)
                if faction_materials[attacker]["Gold"] < 5 or faction_materials[attacker]["Food"] < 5: score -= 15
                # Border forts/camps threatening (+15)
                if any((c.get("is_fort") or c.get("is_camp")) and any(math.hypot(c["x"]-own["x"], c["y"]-own["y"])<100 for own in atk_cities) for c in def_cities): score += 15
                # Only declare if score > threshold AND random chance
                if score > warfare.WAR_THRESHOLD and random.random() < warfare.WAR_DECLARE_CHANCE:
                    if claimed_city: reason = f"{attacker} claims {claimed_city} (score:{score})"
                    elif any(c["sovereign"]==attacker and c["owner"]==defender for c in cities): reason = f"{attacker} seeks revenge on {defender} (score:{score})"
                    else: reason = f"{attacker} declares war on {defender} (score:{score})"
                    declare_war(attacker, defender, reason)
                    for ally in af:
                        if ally == attacker or ally == defender: continue
                        if pair(attacker, ally) in alliances:
                            if random.random() < 0.5: declare_war(ally, defender, f"{ally} joins {attacker}'s war against {defender}!")
                            else:
                                ag, ah = random.randint(5, 20), random.randint(3, 10)
                                faction_materials[attacker]["Gold"] += ag; faction_materials[attacker]["Hide"] += ah
                                faction_materials[ally]["Gold"] -= min(ag, faction_materials[ally]["Gold"])
                                faction_materials[ally]["Hide"] -= min(ah, faction_materials[ally]["Hide"])
                    break
def resolve_treaty(a, b):
    def power(f): return sum(1 for c in cities if c["owner"]==f) + sum(c["troops"] for c in cities if c["owner"]==f)/100.0
    pa, pb = power(a), power(b); victor = a if pa >= pb else b; loser = b if victor == a else a
    occupied = [c for c in cities if c["occupier"] is not None and {c["occupier"], c["sovereign"]} <= {a, b}]
    if not occupied: return
    loser_total_cities = sum(1 for c in cities if c["sovereign"]==loser)
    # Big nations can't be fully annexed — force mix/reparations instead
    if loser_total_cities > 3:
        outcome = random.choice(["return","reparations","mix","mix"])  # no full annex for big nations
    else:
        outcome = random.choice(["return","reparations","annex","mix"])
    if outcome == "return":
        for c in occupied: sov=c["sovereign"]; c["owner"]=c["sovereign"]=sov; c["occupier"]=None; c["color"]=FACTION_COLORS[sov]; c["strategy"]=random.choice(strategies)
        news(f"{victor} returns all cities to {loser} — white peace.")
    elif outcome == "reparations":
        for c in occupied: sov=c["sovereign"]; c["owner"]=c["sovereign"]=sov; c["occupier"]=None; c["color"]=FACTION_COLORS[sov]; c["strategy"]=random.choice(strategies)
        gd = random.randint(20, 80); paid = min(gd, faction_materials[loser]["Gold"])
        faction_materials[loser]["Gold"] -= paid; faction_materials[victor]["Gold"] += paid
        news(f"{victor} demands {gd} Gold from {loser} — paid {paid}.")
    elif outcome == "annex":
        for c in occupied: c["sovereign"]=c["occupier"]; c["occupier"]=None
        news(f"{victor} annexes all occupied territory from {loser}!")
    elif outcome == "mix":
        random.shuffle(occupied)
        # Limit annexation: max 2 cities for big nations, half for small
        max_annex = 2 if loser_total_cities > 3 else max(1, len(occupied)//2)
        split = min(max_annex, len(occupied))
        for c in occupied[:split]: c["sovereign"]=c["occupier"]; c["occupier"]=None
        for c in occupied[split:]: sov=c["sovereign"]; c["owner"]=c["sovereign"]=sov; c["occupier"]=None; c["color"]=FACTION_COLORS[sov]; c["strategy"]=random.choice(strategies)
        gd = random.randint(10, 50); paid = min(gd, faction_materials[loser]["Gold"])
        faction_materials[loser]["Gold"] -= paid; faction_materials[victor]["Gold"] += paid
        news(f"{victor} annexes {split} cities, returns {len(occupied)-split}, takes {paid} Gold.")
def make_treaty(a, b, reason=""):
    key = pair(a, b)
    if key in treaty_cooldowns: return
    treaties[key] = random.randint(TREATY_MIN, TREATY_MAX); war_pressure[key] = 0; end_war(a, b)
    # Remove any stale alliance (war ending = neutral, not allied)
    if key in alliances: del alliances[key]
    # Units don't disappear — they go idle instead
    for u in units[:]:
        if u.owner["owner"] in (a,b) and u.target["owner"] in (a,b):
            u._go_idle()
    resolve_treaty(a, b)
    # After revolution treaty, US demands Charleston + Williamsburg and flag changes to usa.webp
    if "United States" in (a, b) and _revolution_fired:
        other = b if a == "United States" else a
        # US demands Charleston and Williamsburg
        for city in cities:
            if city["name"] in ("Charleston", "Williamsburg", "Richmond") and city["owner"] == other:
                city["owner"] = "United States"; city["sovereign"] = "United States"
                city["occupier"] = None; city["color"] = (100, 180, 255)
                news(f"United States secures {city['name']} in peace treaty!")
        # Flag changes to usa.webp
        try: faction_images["United States"] = pygame.transform.scale(pygame.image.load("images/flags/usa.webp"), (30, 30))
        except: pass
    # After Mexican Independence treaty, Mexico demands territory from Spain
    if "Mexico" in (a, b) and _mexico_fired:
        other = b if a == "Mexico" else a
        if other == "Spain":
            for city in cities:
                if city["name"] in ("Merida", "Albuquerque", "San Diego", "Monterey", "San Antonio") and city["owner"] == "Spain":
                    city["owner"] = "Mexico"; city["sovereign"] = "Mexico"
                    city["occupier"] = None; city["color"] = (0, 100, 50)
                    news(f"Mexico secures {city['name']} in peace treaty!")
    # After Haitian Independence treaty — Haiti just keeps Port-au-Prince, no further demands
    # After American Civil War treaty
    if "Confederate States" in (a, b) and "United States" in (a, b) and _confederate_fired:
        if a == "United States" or b == "United States":
            # Check who won based on city count
            us_cities = sum(1 for c in cities if c["owner"] == "United States")
            confed_cities_count = sum(1 for c in cities if c["owner"] == "Confederate States")
            if us_cities >= confed_cities_count:
                # Union wins — annex all Confederate cities
                for c in cities:
                    if c["owner"] == "Confederate States":
                        c["owner"] = "United States"; c["sovereign"] = "United States"
                        c["color"] = FACTION_COLORS.get("United States", (100, 180, 255))
                        c["occupier"] = None
                news("The Union is restored! All Confederate territory returned to the United States.")
                eliminated_factions.add("Confederate States")
            else:
                # Confederacy wins — they keep their cities (no demands, just independence)
                news("The Confederate States maintain their independence!")
    if reason: news(reason)
def update_treaties():
    for key in list(treaties):
        treaties[key] -= game_speed
        if treaties[key] <= 0: del treaties[key]; treaty_cooldowns[key] = TREATY_COOLDOWN; a,b=tuple(key); news(f"Treaty expired: {a} vs {b} — now neutral.")
    for key in list(treaty_cooldowns):
        treaty_cooldowns[key] -= game_speed
        if treaty_cooldowns[key] <= 0: del treaty_cooldowns[key]
def check_stalemate_treaties():
    # Increment war pressure for all active wars (time-based stalemate buildup)
    for key in wars:
        war_pressure[key] = war_pressure.get(key, 0) + game_speed
    for key, pressure in list(war_pressure.items()):
        if key in wars and key not in treaties and pressure >= STALEMATE_THRESHOLD: a,b=tuple(key); make_treaty(a, b, f"Stalemate! {a} & {b} sign a peace treaty.")
def check_forced_treaties():
    city_counts = {f: sum(1 for c in cities if c["owner"]==f) for f in FACTIONS}; total = len(cities)
    if total == 0: return
    af = active_factions()
    for i, a in enumerate(af):
        for b in af[i+1:]:
            if pair(a,b) in treaties: continue
            ca, cb = city_counts[a]/total, city_counts[b]/total
            if ca >= 0.70 and cb <= 0.15: make_treaty(a, b, f"{a} imposes peace on {b}.")
            elif cb >= 0.70 and ca <= 0.15: make_treaty(a, b, f"{b} imposes peace on {a}.")
# War fatigue tracking: counts unit losses per war
war_fatigue = {}  # frozenset({a,b}) -> {a: losses, b: losses}
def _add_war_fatigue(faction, enemy):
    key = pair(faction, enemy)
    if key not in war_fatigue: war_fatigue[key] = {faction: 0, enemy: 0}
    if faction in war_fatigue[key]: war_fatigue[key][faction] += 1
def check_war_fatigue():
    """Wars end from fatigue: too many losses, drained resources, or capital captured."""
    for key in list(wars):
        a, b = tuple(key)
        if key in treaties or key in treaty_cooldowns: continue
        fa = war_fatigue.get(key, {}).get(a, 0)
        fb = war_fatigue.get(key, {}).get(b, 0)
        mats_a, mats_b = faction_materials.get(a, {}), faction_materials.get(b, {})
        cities_a = sum(1 for c in cities if c["owner"]==a)
        cities_b = sum(1 for c in cities if c["owner"]==b)
        cap_a_lost = not any(c["owner"]==a and c["is_capital"] for c in cities)
        cap_b_lost = not any(c["owner"]==b and c["is_capital"] for c in cities)
        def exhaustion(faction, losses, mats, city_count, cap_lost):
            score = losses * 5
            if mats.get("Gold", 0) < 5: score += 20
            if mats.get("Food", 0) < 5: score += 20
            if city_count <= 1: score += 30
            if cap_lost: score += 40
            return score
        ex_a = exhaustion(a, fa, mats_a, cities_a, cap_a_lost)
        ex_b = exhaustion(b, fb, mats_b, cities_b, cap_b_lost)
        # Surrender: one side is overwhelmed
        if ex_a > 80 and ex_a > ex_b * 1.5:
            news(f"{a} surrenders to {b} — war exhaustion!")
            make_treaty(b, a, f"{b} dictates terms to exhausted {a}.")
            war_fatigue.pop(key, None); continue
        if ex_b > 80 and ex_b > ex_a * 1.5:
            news(f"{b} surrenders to {a} — war exhaustion!")
            make_treaty(a, b, f"{a} dictates terms to exhausted {b}.")
            war_fatigue.pop(key, None); continue
        # White peace: both sides tired
        if ex_a > 40 and ex_b > 40:
            accept_chance = 0.03 + (ex_a + ex_b) * 0.001
            if random.random() < accept_chance:
                news(f"{a} & {b} agree to white peace — war fatigue.")
                make_treaty(a, b, f"White peace between {a} & {b}.")
                war_fatigue.pop(key, None)
def draw_active_treaties():
    W, H = get_screen_size(); ty = H-10
    for key in wars:
        a,b = tuple(key)
        if a in eliminated_factions or b in eliminated_factions: continue
        msg = f"WAR: {a} vs {b}"; ty -= treaty_font.render(msg,True,(255,80,80)).get_height()+2; draw_outlined_text(treaty_font, msg, (255,80,80), (10, ty))
    for key, frames in treaties.items():
        a,b = tuple(key); msg = f"TREATY: {a} \u2194 {b}  ({frames//DAY_TICKS}d)"; ty -= treaty_font.render(msg,True,(255,220,50)).get_height()+2; draw_outlined_text(treaty_font, msg, (255,220,50), (10, ty))
    for key, frames in alliances.items():
        a,b = tuple(key); msg = f"ALLIANCE: {a} & {b}  ({frames//DAY_TICKS}d)"; ty -= treaty_font.render(msg,True,(100,255,100)).get_height()+2; draw_outlined_text(treaty_font, msg, (100,255,100), (10, ty))
alliances = {frozenset({"Great Britain","Iroquois"}): DAY_TICKS*1095, frozenset({"France","Wabanaki"}): DAY_TICKS*1095}
ALLIANCE_MIN, ALLIANCE_MAX = DAY_TICKS*50, DAY_TICKS*1825
def allied(a, b): return pair(a, b) in alliances
def form_alliance(a, b, reason=""):
    key = pair(a, b)
    if key in alliances or key in treaties or key in wars: return  # can't ally enemies or existing relations
    alliances[key] = random.randint(ALLIANCE_MIN, ALLIANCE_MAX)
    if reason: news(reason)
def break_alliance(a, b, reason=""):
    key = pair(a, b)
    if key in alliances: del alliances[key]
    if reason: news(reason)
def update_alliances():
    for key in list(alliances):
        alliances[key] -= game_speed
        if alliances[key] <= 0: del alliances[key]; a,b=tuple(key); news(f"Alliance between {a} & {b} has ended — now neutral.")
def check_alliance_opportunities():
    af = active_factions()
    for i, a in enumerate(af):
        for b in af[i+1:]:
            if pair(a,b) in alliances or pair(a,b) in treaties or pair(a,b) in wars: continue
            enemies_a = {c["owner"] for c in cities if c["owner"]!=a and not at_peace(a,c["owner"]) and not allied(a,c["owner"])}
            enemies_b = {c["owner"] for c in cities if c["owner"]!=b and not at_peace(b,c["owner"]) and not allied(b,c["owner"])}
            common = enemies_a & enemies_b
            if common and random.random() < 0.02: form_alliance(a, b, f"{a} & {b} form alliance against {random.choice(list(common))}!")

# ====================================================================
# === CITY DEFINITIONS ===============================================
# ====================================================================
import cities_data
cities, _FOUNDED_CITIES = cities_data.build_cities(_COORD_SCALE, strategies, city_materials)

# Cities founded at specific dates (hidden until their year)
# Scale founded-city coordinates to the enlarged native space
for _fc in _FOUNDED_CITIES:
    _fc["x"] *= _COORD_SCALE; _fc["y"] *= _COORD_SCALE; _fc["_scaled"] = True
for _faction in FACTIONS:
    fc = [c for c in cities if c["owner"]==_faction]
    if fc and not any(c["material"]=="Hide" for c in fc): random.choice(fc)["material"] = "Hide"
    if fc and not any(c["material"]=="Lumber" for c in fc):
        remaining = [c for c in fc if c["material"] != "Hide"]
        if remaining: random.choice(remaining)["material"] = "Lumber"
        elif len(fc) > 1: random.choice(fc)["material"] = "Lumber"
game_state.cities = cities; game_state.faction_materials = faction_materials
# Scale coordinates of inline city dicts (forts/camps) that weren't made via make_city/make_village
for _c in cities:
    if not _c.get("_scaled"):
        _c["x"] *= _COORD_SCALE; _c["y"] *= _COORD_SCALE; _c["_scaled"] = True
# Ensure all cities have building fields
for _c in cities:
    if "buildings" not in _c: _c["buildings"] = []
    if "construction" not in _c: _c["construction"] = None
    if "_original_sovereign" not in _c: _c["_original_sovereign"] = _c.get("sovereign", _c["owner"])
    _c["_regen_cooldown"] = 0
    if "_happiness" not in _c: _c["_happiness"] = 100
# Assign each city to its region/territory based on coordinates
_assign_cities_to_regions()
# Build navigation grid
_build_nav_grid()
units, merchants, settlers = [], [], []
game_state.units = units; game_state.merchants = merchants; game_state.settlers = settlers
# ====================================================================
# === MANPOWER & CONSCRIPTION SYSTEM =================================
# ====================================================================
# National manpower pool: available men ready to be recruited
_faction_manpower = {}  # {faction: available_manpower}

# Conscription policies: controls what % of population feeds into manpower monthly
CONSCRIPTION_POLICIES = [
    ("Volunteer Service", 0.005, 0),        # 0.5% of population/month, no happiness cost
    ("Selective Service", 0.015, -1),        # 1.5%/month, slight unrest
    ("Required Service", 0.03, -4),          # 3%/month, serious unrest
    ("Total Mobilization", 0.05, -8),        # 5%/month, crippling unrest (wartime only)
]
_faction_conscription = {}  # {faction: policy_index}

def _init_manpower():
    """Initialize manpower pools based on starting city populations."""
    for f in FACTIONS:
        if f in NATIVE_FACTIONS:
            # Natives have smaller but ready warrior pools
            total_pop = sum(_get_city_population(c) for c in cities if c["owner"] == f)
            _faction_manpower[f] = int(total_pop * 0.05)  # 5% immediately available
        else:
            total_pop = sum(_get_city_population(c) for c in cities if c["owner"] == f)
            _faction_manpower[f] = int(total_pop * 0.03)  # 3% starting manpower
        _faction_conscription[f] = 1  # Standard Levy default

def _get_conscription_policy(faction):
    if faction not in _faction_conscription: _faction_conscription[faction] = 1
    return CONSCRIPTION_POLICIES[_faction_conscription[faction]]

def _update_manpower_monthly():
    """Monthly: cities contribute manpower based on population and conscription policy."""
    for faction in active_factions():
        if faction not in _faction_manpower: _faction_manpower[faction] = 0
        policy_name, rate, hap_cost = _get_conscription_policy(faction)
        # Each city contributes population * rate to the national pool
        for city in cities:
            if city["owner"] != faction: continue
            pop = _get_city_population(city)
            contribution = int(pop * rate)
            _faction_manpower[faction] += contribution
        # Cap manpower at total faction population * 10%
        total_pop = sum(_get_city_population(c) for c in cities if c["owner"] == faction)
        max_manpower = int(total_pop * 0.10)
        _faction_manpower[faction] = min(_faction_manpower[faction], max(500, max_manpower))

def _get_regiment_size(faction):
    """Determine how many men a new regiment gets based on faction size and available manpower."""
    available = _faction_manpower.get(faction, 0)
    if faction in NATIVE_FACTIONS:
        # Native warbands: 100-300 warriors
        return min(available, random.randint(100, 300))
    else:
        # Colonial regiments: 400-1000 men, limited by available manpower
        ideal = random.randint(400, 1000)
        return min(available, max(200, ideal))

def _recruit_manpower(faction, amount):
    """Deduct manpower from the national pool when recruiting."""
    _faction_manpower[faction] = max(0, _faction_manpower.get(faction, 0) - amount)

def _return_manpower(faction, amount):
    """Return surviving troops to manpower pool when a unit is disbanded/retreats home."""
    _faction_manpower[faction] = _faction_manpower.get(faction, 0) + int(amount * 0.5)  # 50% return (rest desert/injured)

# ====================================================================
# === ENTITY CLASSES (Unit, Merchant, Settler, Ship) =================
# ====================================================================
RETREAT_RADIUS, RETREAT_CHANCE, RETREAT_DIST = config.RETREAT_RADIUS, config.RETREAT_CHANCE, config.RETREAT_DIST
class Unit:
    def __init__(self, owner, target, formation=None):
        self.owner, self.target = owner, target; self.x, self.y = float(owner["x"]), float(owner["y"])
        self.formation = formation or random.choice(formations)
        self.speed, self.power, self.size = {"Line":(0.36,5,3),"Column":(0.54,3,2),"Square":(0.75,8,4)}[self.formation]
        if owner["owner"] in NATIVE_FACTIONS:
            self.power = max(1, self.power // 2)
            if self.formation == "Square": self.power = max(1, self.power // 2)
        else: self.power *= 2
        # Manpower-based troop count replaces flat HP
        faction = owner["owner"]
        self.men = _get_regiment_size(faction)
        _recruit_manpower(faction, self.men)
        self.max_men = self.men
        self.hp = self.men; self.max_hp = self.max_men  # keep hp/max_hp as aliases for compatibility
        self.retreating = False; self.retreat_x = self.retreat_y = None
        self.regiment_name = _gen_regiment_name(owner["owner"]); self.siege_timer = 0
        self.idle = False; self.fighting = None; self.patrolling = False; self.battle_cd = 0; self.tier = 0
        self.is_ship = False; self.ship_timer = 0
        self._stuck_x = self.x; self._stuck_y = self.y; self._stuck_ticks = 0; self._trace_side = 1
        self._land_waypoints = []
    def _take_casualties(self, losses):
        """Remove men from this unit. Syncs hp with men count."""
        self.men = max(1, self.men - losses)
        self.hp = self.men
    def _is_routed(self):
        """Unit routs when below 25% strength."""
        return self.men <= self.max_men * 0.25
    def _spd(self):
        spd = self.speed * game_speed
        if self.is_ship: return spd * 2  # ships/canoes move 2x faster
        if self.owner["owner"] == "Comanche": spd *= 1.4
        # Biome movement modifier
        is_cav = self.formation == "Square"
        spd *= _get_biome_move_mult(self.x, self.y, is_cavalry=is_cav)
        # (mountain slowdown already applied via the "mountains" biome move multiplier)
        # Road speed bonus: +25%
        if _on_road(self.x, self.y): spd *= 1.25
        return spd
    def _nearest_friendly_city(self):
        friendly = [c for c in cities if c["owner"]==self.owner["owner"]]
        return min(friendly, key=lambda c: math.hypot(c["x"]-self.x, c["y"]-self.y)) if friendly else None
    def _start_retreat_to_city(self):
        dest = self._nearest_friendly_city()
        if dest: self.retreating = True; self.retreat_x = dest["x"]; self.retreat_y = dest["y"]
        elif self in units: units.remove(self)
    def _check_retreat(self):
        pass  # Units no longer retreat from outnumbering — they fight to low HP instead
    def _go_idle(self):
        dest = self._nearest_friendly_city()
        if dest:
            friendly = [c for c in cities if c["owner"]==self.owner["owner"] and c is not dest and math.hypot(c["x"]-self.x, c["y"]-self.y) < 120]
            if friendly: self.patrolling = True; self.idle = False; self.target = random.choice(friendly)
            else: self.idle = True; self.patrolling = False; self.target = dest
        elif self in units: units.remove(self)
    def _find_enemies(self):
        max_range = NATIVE_MAX_RANGE if self.owner["owner"] in NATIVE_FACTIONS else UNIT_MAX_RANGE
        candidates = [c for c in cities if c["owner"]!=self.owner["owner"] and not at_peace(self.owner["owner"], c["owner"]) and math.hypot(c["x"]-self.x, c["y"]-self.y) <= max_range]
        return candidates
    def update(self):
        # Auto-detect ship/canoe mode based on current position
        # Force disembark when near a city (landing at settlement)
        near_city = any(math.hypot(c["x"]-self.x, c["y"]-self.y) < 10 for c in cities)
        if near_city:
            self.is_ship = False
        else:
            self.is_ship = _is_water(self.x, self.y)
        if self.battle_cd > 0 and self.fighting is None: self.battle_cd -= 1
        if self.fighting is not None: self._update_battle(); return
        # Stuck detection — reroute through a different path if not making progress
        self._stuck_ticks += 1
        if self._stuck_ticks >= 40:
            moved = math.hypot(self.x - self._stuck_x, self.y - self._stuck_y)
            if moved < 3 and not self.idle:
                self._trace_side *= -1
                # Try a random nudge to get unstuck
                for _ in range(8):
                    angle = random.uniform(0, 2*math.pi); nudge = self._spd() * random.uniform(3, 8)
                    nx, ny = self.x + math.cos(angle)*nudge, self.y + math.sin(angle)*nudge
                    if 0 < nx < _MAP_NATIVE_W and 0 < ny < _MAP_NATIVE_H: self.x, self.y = nx, ny; break
            self._stuck_x = self.x; self._stuck_y = self.y; self._stuck_ticks = 0
        if self.idle:
            if random.random() < 0.02:
                enemies = self._find_enemies()
                if enemies: self.idle = False; self.target = random.choice(enemies); return
            dx, dy = self.target["x"]-self.x, self.target["y"]-self.y; dist = math.hypot(dx, dy)
            if dist > 5:
                ndx, ndy = dx/dist, dy/dist; spd = self._spd()
                self.x += ndx*spd; self.y += ndy*spd
            return
        if self.patrolling:
            enemies = self._find_enemies()
            if enemies: self.patrolling = False; self.target = random.choice(enemies); return
            dx, dy = self.target["x"]-self.x, self.target["y"]-self.y; dist = math.hypot(dx, dy)
            if dist > 5:
                ndx, ndy = dx/dist, dy/dist; spd = self._spd()
                self.x += ndx*spd; self.y += ndy*spd
            else:
                # Pick next patrol target
                friendly = [c for c in cities if c["owner"]==self.owner["owner"] and c is not self.target and math.hypot(c["x"]-self.x, c["y"]-self.y) < 120]
                if friendly: self.target = random.choice(friendly)
                else: self._go_idle()
            return
        if at_peace(self.owner["owner"], self.target["owner"]): self._go_idle(); return
        if self.retreating:
            dx, dy = self.retreat_x-self.x, self.retreat_y-self.y; dist = math.hypot(dx, dy)
            if dist > 8:
                ndx, ndy = dx/dist, dy/dist; spd = self._spd()*1.3
                self.x += ndx*spd; self.y += ndy*spd
            else:
                city = self._nearest_friendly_city()
                # Heal: takes 10 population from settlement to restore up to 100 HP
                if city and city["troops"] > 10 and self.hp < self.max_hp:
                    city["troops"] -= 10
                    self.hp = min(self.max_hp, self.hp + 100)
                still_need = self.max_hp - self.hp
                if still_need > 0:
                    for alt in sorted([c for c in cities if c["owner"]==self.owner["owner"] and c is not city and c["troops"]>10], key=lambda c: math.hypot(c["x"]-self.x, c["y"]-self.y)):
                        give = min(still_need, alt["troops"]-1)
                        if give > 0: self.hp += give; alt["troops"] -= give; still_need -= give
                        if still_need <= 0: break
                if self.hp < self.max_hp * 0.5:
                    # Look for a city with troops to heal from
                    heal_found = False
                    for cand in sorted([c for c in cities if c["owner"]==self.owner["owner"] and c["troops"]>20], key=lambda c: math.hypot(c["x"]-self.x, c["y"]-self.y)):
                        if math.hypot(cand["x"]-self.x, cand["y"]-self.y) > 10: self.retreat_x = cand["x"]; self.retreat_y = cand["y"]; heal_found = True; break
                    if not heal_found:
                        # No city can heal — defend at current position until troops regen
                        self.retreating = False; self.idle = True
                        dest = self._nearest_friendly_city()
                        if dest: self.target = dest
                        return
                    return
                # Healed enough — resume
                self.retreating = False
                if self.hp < self.max_hp * 0.3:
                    # Still too low to fight — idle/defend
                    self._go_idle()
                else:
                    new_enemies = self._find_enemies()
                    if new_enemies: self.target = random.choice(new_enemies)
                    else: self._go_idle()
            return
        self._check_retreat()
        if self.retreating: return
        # Low HP — avoid fighting, retreat to heal instead
        if self.hp < self.max_hp * 0.3:
            self._start_retreat_to_city(); return
        if random.random() < 0.02:
            for fc in [c for c in cities if c["owner"]==self.owner["owner"]]:
                if math.hypot(fc["x"]-self.x, fc["y"]-self.y) < 150:
                    if [u for u in units if u is not self and u.owner["owner"]!=self.owner["owner"] and math.hypot(u.x-fc["x"], u.y-fc["y"])<10] and random.random()<0.6: self.target = fc; break
        dx, dy = self.target["x"]-self.x, self.target["y"]-self.y; dist = math.hypot(dx, dy)
        if dist > 5:
            ndx, ndy = dx/dist, dy/dist; spd = self._spd()
            self.x += ndx*spd; self.y += ndy*spd
        else:
            if self.target.get("owner") == self.owner["owner"]:
                ne = [c for c in cities if c["owner"]!=self.owner["owner"] and not at_peace(self.owner["owner"],c["owner"]) and math.hypot(c["x"]-self.x,c["y"]-self.y)<=UNIT_MAX_RANGE]
                if ne: self.target = random.choice(ne)
                else: self._go_idle()
                return
            self.siege_timer += 1
            # Siege phases: Encirclement → Bombardment → Assault → Breach
            if not hasattr(self, '_siege_phase'): self._siege_phase = "encirclement"; self._siege_phase_timer = 0
            self._siege_phase_timer = getattr(self, '_siege_phase_timer', 0) + 1

            if self._siege_phase == "encirclement":
                # Surrounding the city, cutting supply — no damage, happiness drains
                if random.random() < 0.2:
                    self.target["_happiness"] = max(0, self.target.get("_happiness", 100) - 1)
                if self._siege_phase_timer >= DAY_TICKS * 5:
                    self._siege_phase = "bombardment"; self._siege_phase_timer = 0

            elif self._siege_phase == "bombardment":
                # Artillery/ranged attacks — damages garrison, civilian casualties
                if self.siege_timer >= DAY_TICKS:
                    self.siege_timer = 0
                    siege_dmg = max(1, self.power // 3)
                    self.target["troops"] -= siege_dmg
                    self.target["_regen_cooldown"] = 600
                    if "_population" in self.target and random.random() < 0.3:
                        self.target["_population"] = max(50, self.target["_population"] - random.randint(10, 40))
                    if random.random() < 0.3:
                        self.target["_happiness"] = max(0, self.target.get("_happiness", 100) - 1)
                if self._siege_phase_timer >= DAY_TICKS * 7:
                    self._siege_phase = "assault"; self._siege_phase_timer = 0

            elif self._siege_phase == "assault":
                # Direct attack on walls — heavy damage to both sides
                if self.siege_timer >= DAY_TICKS:
                    self.siege_timer = 0
                    siege_dmg = max(2, self.power)
                    self.target["troops"] -= siege_dmg
                    self.target["_regen_cooldown"] = 600
                    # Attacker takes losses storming walls
                    attacker_losses = max(1, random.randint(2, max(2, self.target["troops"] // 10)))
                    self._take_casualties(attacker_losses)
                    if "_population" in self.target and random.random() < 0.4:
                        self.target["_population"] = max(50, self.target["_population"] - random.randint(15, 60))
                    if random.random() < 0.5:
                        self.target["_happiness"] = max(0, self.target.get("_happiness", 100) - 2)
                if self._siege_phase_timer >= DAY_TICKS * 5:
                    self._siege_phase = "breach"; self._siege_phase_timer = 0

            elif self._siege_phase == "breach":
                # Walls breached — overwhelming force, fast garrison depletion
                if self.siege_timer >= DAY_TICKS:
                    self.siege_timer = 0
                    siege_dmg = max(3, self.power * 2)
                    self.target["troops"] -= siege_dmg
                    self.target["_regen_cooldown"] = 600
                    attacker_losses = max(1, random.randint(1, max(1, self.target["troops"] // 15)))
                    self._take_casualties(attacker_losses)
                    if "_population" in self.target:
                        self.target["_population"] = max(50, self.target["_population"] - random.randint(20, 80))
                    self.target["_happiness"] = max(0, self.target.get("_happiness", 100) - 3)

            # Attacker routs if too many losses
            if self._is_routed():
                self._siege_phase = None; self._start_retreat_to_city(); return
            if self.target["troops"] <= 0:
                if self.target.get("is_fort"):
                    news(f"{self.target['name']} destroyed by {self.owner['owner']}!")
                    if self.target in cities: cities.remove(self.target)
                    ne = [c for c in cities if c["owner"]!=self.owner["owner"] and not at_peace(self.owner["owner"],c["owner"]) and math.hypot(c["x"]-self.x,c["y"]-self.y)<=UNIT_MAX_RANGE]
                    if ne: self.target = random.choice(ne)
                    else: self._go_idle()
                    return
                base_troops = 30 if self.target.get("is_camp") else (50 if self.target.get("is_village") else (200 if self.target["is_capital"] else 100))
                self.target["troops"] = base_troops + self.target.get("tier", 0)*25
                attacker, old_owner = self.owner["owner"], self.target["owner"]
                self.target["occupier"] = attacker; self.target["owner"] = attacker
                self.target["color"] = FACTION_COLORS[attacker]; self.target["strategy"] = random.choice(strategies)
                # Moosonee rename logic — becomes Moose Factory under Great Britain, reverts otherwise
                if self.target.get("_orig_name") == "Moosonee" or self.target["name"] in ("Moosonee", "Moose Factory"):
                    if "_orig_name" not in self.target: self.target["_orig_name"] = "Moosonee"
                    self.target["name"] = "Moose Factory" if attacker == "Great Britain" else "Moosonee"
                war_pressure[pair(attacker, old_owner)] = 0; news(f"{attacker} occupies {self.target['name']}!")
                # Population loss from conquest (10-25% flee/die)
                if "_population" in self.target:
                    loss_pct = random.uniform(0.10, 0.25)
                    self.target["_population"] = max(50, int(self.target["_population"] * (1 - loss_pct)))
                # Happiness drops from conquest
                self.target["_happiness"] = max(0, self.target.get("_happiness", 100) - random.randint(20, 40))
                # Replace governor/chief with new one from conquering faction
                for _gk in ("_governor", "_governor_type", "_governor_traits", "_governor_ethnicity", "_governor_religion", "_is_chief"):
                    self.target.pop(_gk, None)
                # Pillage: steal random resources from the defeated faction
                pillage_mats = random.sample(materials, random.randint(2, 4))
                pillage_report = []
                for pmat in pillage_mats:
                    amount = random.randint(5, 25)
                    stolen = min(amount, faction_materials[old_owner].get(pmat, 0))
                    if stolen > 0:
                        faction_materials[old_owner][pmat] -= stolen
                        faction_materials[attacker][pmat] += stolen
                        pillage_report.append(f"{stolen} {pmat}")
                if pillage_report: news(f"{attacker} pillages {self.target['name']}: {', '.join(pillage_report)}")
                # If capital was captured, relocate capital for the loser and demote old capital
                if self.target.get("is_capital"):
                    self.target["is_capital"] = False  # old capital becomes normal city
                    # Find a new capital for the old owner — any city, or fort if nothing else
                    remaining = [c for c in cities if c["owner"]==old_owner and not c.get("is_fort") and not c.get("is_camp")]
                    if not remaining: remaining = [c for c in cities if c["owner"]==old_owner]
                    if remaining:
                        new_cap = random.choice(remaining); new_cap["is_capital"] = True
                        news(f"{old_owner} moves capital to {new_cap['name']}!")
                    # 50% chance of surrender
                    if random.random() < 0.5:
                        for c in cities:
                            if c["owner"]==old_owner: c["occupier"]=attacker; c["owner"]=attacker; c["color"]=FACTION_COLORS[attacker]
                        units[:] = [u for u in units if u.owner["owner"]!=old_owner]
                        news(f"{old_owner} surrenders to {attacker}!")
                        make_treaty(attacker, old_owner, f"{attacker} dictates peace terms to {old_owner}.")
                for u in units[:]:
                    if u is not self and u.owner is self.target: units.remove(u)
                ne = [c for c in cities if c["owner"]!=attacker and not at_peace(attacker,c["owner"]) and math.hypot(c["x"]-self.x,c["y"]-self.y)<=UNIT_MAX_RANGE]
                if ne: self.target = random.choice(ne)
                else: self._go_idle()
    def fight(self, enemy):
        if at_peace(self.owner["owner"], enemy.owner["owner"]): return
        # Natives in canoe on open water cannot attack (but can fight near cities since they disembark)
        if self.is_ship and self.owner["owner"] in NATIVE_FACTIONS: return
        if enemy.is_ship and enemy.owner["owner"] in NATIVE_FACTIONS: return
        if self.fighting is None and enemy.fighting is None:
            self.fighting = enemy; enemy.fighting = self
            self.battle_cd = 0; enemy.battle_cd = 0
            # Initialize battle phase
            self._battle_phase = "encampment"; enemy._battle_phase = "encampment"
            self._phase_timer = 0; enemy._phase_timer = 0
    def _update_battle(self):
        enemy = self.fighting
        if enemy is None or enemy not in units: self.fighting = None; self._battle_phase = None; return
        if not hasattr(self, '_battle_phase') or not self._battle_phase:
            self._battle_phase = "battle"
        if not hasattr(self, '_phase_timer'): self._phase_timer = 0
        self._phase_timer += 1

        # Phase durations: Encampment(7 days) → Approach(5 days) → Engagement(5 days) → Battle(ongoing, reduced dmg) → Retreat
        if self._battle_phase == "encampment":
            # Units are setting up camp, scouting, preparing formations — no damage
            if self._phase_timer >= DAY_TICKS * 7:
                self._battle_phase = "approach"; self._phase_timer = 0
                enemy._battle_phase = "approach"; enemy._phase_timer = 0

        elif self._battle_phase == "approach":
            # Units closing distance — light skirmish casualties every 2 days
            if self._phase_timer >= DAY_TICKS * 2:
                self._phase_timer = 0
                # Skirmish kills: power/5, scaled by attacker's men ratio
                skirmish_kills = max(1, int(self.power * self.men / self.max_men / 5))
                enemy_skirmish = max(1, int(enemy.power * enemy.men / enemy.max_men / 5))
                enemy._take_casualties(skirmish_kills); self._take_casualties(enemy_skirmish)
            if self._phase_timer == 0 and hasattr(self, '_approach_ticks'):
                self._approach_ticks += 1
            elif not hasattr(self, '_approach_ticks'):
                self._approach_ticks = 0
            if getattr(self, '_approach_ticks', 0) >= 3:
                self._battle_phase = "engagement"; self._phase_timer = 0
                enemy._battle_phase = "engagement"; enemy._phase_timer = 0
                self._approach_ticks = 0

        elif self._battle_phase == "engagement":
            # Heavy skirmishing — moderate casualties every 2 days
            if self._phase_timer >= DAY_TICKS * 2:
                self._phase_timer = 0
                my_kills = max(2, int(self.power * random.randint(1, 2) * self.men / self.max_men / 3))
                enemy_kills = max(2, int(enemy.power * random.randint(1, 2) * enemy.men / enemy.max_men / 3))
                my_def = _get_biome_defense_mult(self.x, self.y)
                enemy_def = _get_biome_defense_mult(enemy.x, enemy.y)
                enemy._take_casualties(int(my_kills / enemy_def))
                self._take_casualties(int(enemy_kills / my_def))
            if self._phase_timer == 0 and hasattr(self, '_engage_ticks'):
                self._engage_ticks += 1
            elif not hasattr(self, '_engage_ticks'):
                self._engage_ticks = 0
            if getattr(self, '_engage_ticks', 0) >= 3:
                self._battle_phase = "battle"; self._phase_timer = 0
                enemy._battle_phase = "battle"; enemy._phase_timer = 0
                self._engage_ticks = 0

        elif self._battle_phase == "battle":
            # Full decisive combat — heavy casualties every 2 days
            if self._phase_timer >= DAY_TICKS * 2:
                self._phase_timer = 0
                my_kills = max(3, int(self.power * random.randint(2, 4) * self.men / self.max_men))
                enemy_kills = max(3, int(enemy.power * random.randint(2, 4) * enemy.men / enemy.max_men))
                my_def = _get_biome_defense_mult(self.x, self.y)
                enemy_def = _get_biome_defense_mult(enemy.x, enemy.y)
                enemy._take_casualties(int(my_kills / enemy_def))
                self._take_casualties(int(enemy_kills / my_def))

        # Check for rout — unit retreats when below 25% strength
        if self._is_routed():
            self._battle_phase = "retreat"
            self.fighting = None; enemy.fighting = None; enemy._battle_phase = None
            _add_war_fatigue(self.owner["owner"], enemy.owner["owner"])
            # Dead men reduce city population (casualties don't come home)
            dead = self.max_men - self.men
            nearest = self._nearest_friendly_city()
            if nearest and "_population" in nearest:
                nearest["_population"] = max(50, nearest["_population"] - dead)
            _return_manpower(self.owner["owner"], self.men)  # survivors return
            self._start_retreat_to_city(); self._call_reinforcements(enemy)
        elif enemy._is_routed():
            enemy._battle_phase = "retreat"
            enemy.fighting = None; self.fighting = None; self._battle_phase = None
            _add_war_fatigue(enemy.owner["owner"], self.owner["owner"])
            dead = enemy.max_men - enemy.men
            nearest = enemy._nearest_friendly_city()
            if nearest and "_population" in nearest:
                nearest["_population"] = max(50, nearest["_population"] - dead)
            _return_manpower(enemy.owner["owner"], enemy.men)
            enemy._start_retreat_to_city(); self._call_reinforcements(enemy)
    def _call_reinforcements(self, enemy):
        bx, by = (self.x+enemy.x)/2, (self.y+enemy.y)/2
        for u in units[:]:
            if u is self or u is enemy or u.retreating or u.fighting: continue
            if (u.owner["owner"]==self.owner["owner"] or u.owner["owner"]==enemy.owner["owner"]) and math.hypot(u.x-bx, u.y-by)<120 and random.random()<0.3:
                u.target = enemy.owner if u.owner["owner"]==self.owner["owner"] else self.owner
    def _status_text(self):
        if self.fighting: return "Fighting"
        if self.retreating: return "Retreating"
        if self.ship_timer > 0: return "Boarding"
        if self.is_ship: return "Sailing"
        if self.idle: return "Idle"
        if self.patrolling: return "Patrolling"
        return "Sieging" if math.hypot(self.target["x"]-self.x, self.target["y"]-self.y) <= 5 else "Attacking"
    def draw(self):
        sx, sy = world_to_screen(self.x, self.y); col = self.owner["color"]
        if not _on_screen(sx, sy): return  # off-screen: skip drawing (position still tracked)
        # Draw as ship when in ship mode
        if self.is_ship and self.owner["owner"] in NATIVE_FACTIONS and _canoe_anims:
            # Draw as canoe for native units on water
            dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
            if abs(dx) > abs(dy):
                if dx > 0: frames = _canoe_anims["br"] if dy > 10 else (_canoe_anims["tr"] if dy < -10 else _canoe_anims["right"])
                else: frames = _canoe_anims["bl"] if dy > 10 else (_canoe_anims["tl"] if dy < -10 else _canoe_anims["left"])
            else:
                if dy > 0: frames = _canoe_anims["down"]
                else: frames = _canoe_anims["up"]
            idx = (_brit_anim_tick // 8) % len(frames); frame = frames[idx]
            sz = max(10, int(15*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.is_ship and _ship_anims:
            # Pick direction from movement
            dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
            if abs(dx) > abs(dy):
                if dx > 0: frames = _ship_anims["br"] if dy > 10 else (_ship_anims["tr"] if dy < -10 else _ship_anims["right"])
                else: frames = _ship_anims["bl"] if dy > 10 else (_ship_anims["tl"] if dy < -10 else _ship_anims["left"])
            else:
                if dy > 0: frames = _ship_anims["down"]
                else: frames = _ship_anims["up"]
            idx = (_brit_anim_tick // 8) % len(frames); frame = frames[idx]
            sz = max(12, int(18*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.ship_timer > 0:
            # Boarding animation — show idle
            r = max(2, int(3*zoom))
            pygame.draw.circle(screen, (139, 90, 43), (sx, sy), r)
        elif self.owner["owner"] == "Great Britain" and _brit_anims:
            # Use cavalry sprites for Square formation
            anims = _brit_cav if (self.formation == "Square" and _brit_cav) else _brit_anims
            # Pick animation based on movement direction and status
            status = self._status_text()
            if status in ("Fighting", "Sieging"):
                frames = anims["attack"]; spd = 6
            elif status in ("Idle", "Boarding"):
                frames = anims["idle"]; spd = 1
            else:
                # Determine direction from movement
                dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = anims["idle"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = anims["walk_br"] if dy > 20 else (anims["walk_tr"] if dy < -20 else anims["walk_right"])
                    else:
                        frames = anims["walk_bl"] if dy > 20 else (anims["walk_tl"] if dy < -20 else anims["walk_left"])
                else:
                    if dy > 0: frames = anims["walk_down"]
                    else: frames = anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if status in ("Fighting", "Sieging"):
                sz = max(6, int(9*zoom))  # full size for attack
            else:
                sz = max(3, int(5*zoom))  # half size for all other animations
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.owner["owner"] == "France" and _fren_anims:
            # Use cavalry sprites for Square formation
            anims = _fren_cav if (self.formation == "Square" and _fren_cav) else _fren_anims
            # Pick animation based on movement direction and status
            status = self._status_text()
            if status in ("Fighting", "Sieging"):
                frames = anims["attack"]; spd = 6
            elif status in ("Idle", "Boarding"):
                frames = anims["idle"]; spd = 1
            else:
                dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = anims["idle"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = anims["walk_br"] if dy > 20 else (anims["walk_tr"] if dy < -20 else anims["walk_right"])
                    else:
                        frames = anims["walk_bl"] if dy > 20 else (anims["walk_tl"] if dy < -20 else anims["walk_left"])
                else:
                    if dy > 0: frames = anims["walk_down"]
                    else: frames = anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if status in ("Fighting", "Sieging"):
                sz = max(6, int(9*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.owner["owner"] == "Russia" and _rus_anims:
            # Use cavalry sprites for Square formation
            anims = _rus_cav if (self.formation == "Square" and _rus_cav) else _rus_anims
            # Pick animation based on movement direction and status
            status = self._status_text()
            if status in ("Fighting", "Sieging"):
                frames = anims["attack"]; spd = 6
            elif status in ("Idle", "Boarding"):
                frames = anims["idle"]; spd = 1
            else:
                dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = anims["idle"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = anims["walk_br"] if dy > 20 else (anims["walk_tr"] if dy < -20 else anims["walk_right"])
                    else:
                        frames = anims["walk_bl"] if dy > 20 else (anims["walk_tl"] if dy < -20 else anims["walk_left"])
                else:
                    if dy > 0: frames = anims["walk_down"]
                    else: frames = anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if status in ("Fighting", "Sieging"):
                sz = max(6, int(9*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.owner["owner"] == "Spain" and _spa_anims:
            # Use cavalry sprites for Square formation
            anims = _spa_cav if (self.formation == "Square" and _spa_cav) else _spa_anims
            # Pick animation based on movement direction and status
            status = self._status_text()
            if status in ("Fighting", "Sieging"):
                frames = anims["attack"]; spd = 6
            elif status in ("Idle", "Boarding"):
                frames = anims["idle"]; spd = 1
            else:
                dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = anims["idle"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = anims["walk_br"] if dy > 20 else (anims["walk_tr"] if dy < -20 else anims["walk_right"])
                    else:
                        frames = anims["walk_bl"] if dy > 20 else (anims["walk_tl"] if dy < -20 else anims["walk_left"])
                else:
                    if dy > 0: frames = anims["walk_down"]
                    else: frames = anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if status in ("Fighting", "Sieging"):
                sz = max(6, int(9*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.owner["owner"] == "United States" and _usa_anims:
            # Use cavalry sprites for Square formation
            anims = _usa_cav if (self.formation == "Square" and _usa_cav) else _usa_anims
            # Pick animation based on movement direction and status
            status = self._status_text()
            if status in ("Fighting", "Sieging"):
                frames = anims["attack"]; spd = 6
            elif status in ("Idle", "Boarding"):
                frames = anims["idle"]; spd = 1
            else:
                dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = anims["idle"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = anims["walk_br"] if dy > 20 else (anims["walk_tr"] if dy < -20 else anims["walk_right"])
                    else:
                        frames = anims["walk_bl"] if dy > 20 else (anims["walk_tl"] if dy < -20 else anims["walk_left"])
                else:
                    if dy > 0: frames = anims["walk_down"]
                    else: frames = anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if status in ("Fighting", "Sieging"):
                sz = max(6, int(9*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.owner["owner"] == "Denmark" and _den_anims:
            # Use cavalry sprites for Square formation
            anims = _den_cav if (self.formation == "Square" and _den_cav) else _den_anims
            # Pick animation based on movement direction and status
            status = self._status_text()
            if status in ("Fighting", "Sieging"):
                frames = anims["attack"]; spd = 6
            elif status in ("Idle", "Boarding"):
                frames = anims["idle"]; spd = 1
            else:
                dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = anims["idle"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = anims["walk_br"] if dy > 20 else (anims["walk_tr"] if dy < -20 else anims["walk_right"])
                    else:
                        frames = anims["walk_bl"] if dy > 20 else (anims["walk_tl"] if dy < -20 else anims["walk_left"])
                else:
                    if dy > 0: frames = anims["walk_down"]
                    else: frames = anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if status in ("Fighting", "Sieging"):
                sz = max(6, int(9*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.owner["owner"] == "Pirates" and _pir_anims:
            # Pick animation based on movement direction and status
            status = self._status_text()
            if status in ("Fighting", "Sieging"):
                frames = _pir_anims["attack"]; spd = 6
            elif status in ("Idle", "Boarding"):
                frames = _pir_anims["idle"]; spd = 1
            else:
                dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = _pir_anims["idle"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = _pir_anims["walk_br"] if dy > 20 else (_pir_anims["walk_tr"] if dy < -20 else _pir_anims["walk_right"])
                    else:
                        frames = _pir_anims["walk_bl"] if dy > 20 else (_pir_anims["walk_tl"] if dy < -20 else _pir_anims["walk_left"])
                else:
                    if dy > 0: frames = _pir_anims["walk_down"]
                    else: frames = _pir_anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if status in ("Fighting", "Sieging"):
                sz = max(6, int(9*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.owner["owner"] in ("Iroquois", "Wabanaki", "Cree", "Comanche", "Dakota") and (_iro_anims or _wab_anims or _cree_anims):
            _nat_cav = sprites.cavalry_anims.get(self.owner["owner"])
            if self.owner["owner"] == "Cree": _nat_inf = _cree_anims
            elif self.owner["owner"] == "Iroquois": _nat_inf = _iro_anims
            elif self.owner["owner"] == "Wabanaki": _nat_inf = _wab_anims
            else: _nat_inf = sprites.infantry_anims.get(self.owner["owner"], _iro_anims)
            anims = _nat_cav if (self.formation == "Square" and _nat_cav) else _nat_inf
            status = self._status_text()
            if status in ("Fighting", "Sieging"):
                frames = anims["attack"]; spd = 6
            elif status in ("Idle", "Boarding"):
                frames = anims["idle"]; spd = 1
            else:
                dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = anims["idle"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = anims["walk_br"] if dy > 20 else (anims["walk_tr"] if dy < -20 else anims["walk_right"])
                    else:
                        frames = anims["walk_bl"] if dy > 20 else (anims["walk_tl"] if dy < -20 else anims["walk_left"])
                else:
                    if dy > 0: frames = anims["walk_down"]
                    else: frames = anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if status in ("Fighting", "Sieging"):
                sz = max(6, int(9*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        else:
            # Generic sprite handler for any faction with loaded sprites (Mexico, Haiti, etc.)
            _gen_anims = sprites.infantry_anims.get(self.owner["owner"])
            _gen_cav = sprites.cavalry_anims.get(self.owner["owner"])
            if _gen_anims:
                anims = _gen_cav if (self.formation == "Square" and _gen_cav) else _gen_anims
                status = self._status_text()
                if status in ("Fighting", "Sieging"):
                    frames = anims["attack"]; spd = 6
                elif status in ("Idle", "Boarding"):
                    frames = anims["idle"]; spd = 1
                else:
                    dx = self.target["x"] - self.x; dy = self.target["y"] - self.y
                    if abs(dx) < 5 and abs(dy) < 5: frames = anims["idle"]; spd = 1
                    elif abs(dx) > abs(dy):
                        if dx > 0:
                            frames = anims["walk_br"] if dy > 20 else (anims["walk_tr"] if dy < -20 else anims["walk_right"])
                        else:
                            frames = anims["walk_bl"] if dy > 20 else (anims["walk_tl"] if dy < -20 else anims["walk_left"])
                    else:
                        if dy > 0: frames = anims["walk_down"]
                        else: frames = anims["walk_up"]
                    spd = 8
                idx = (_brit_anim_tick // spd) % len(frames)
                frame = frames[idx]
                if status in ("Fighting", "Sieging"):
                    sz = max(6, int(9*zoom))
                else:
                    sz = max(3, int(5*zoom))
                ratio = frame.get_height() / max(1, frame.get_width())
                scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
                screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
            else:
                pygame.draw.circle(screen, col, (sx,sy), max(2, int(self.size*zoom)))
        status = self._status_text()
        if status == "Fighting" and self.fighting:
            ex = (sx+world_to_screen(self.fighting.x, self.fighting.y)[0])//2; ey = (sy+world_to_screen(self.fighting.x, self.fighting.y)[1])//2
            es = max(8, int(12*zoom)); screen.blit(get_explosion_frame(es), (ex-es//2, ey-es//2))
        elif status == "Sieging":
            es = max(5, int(6*zoom))
            for _ in range(random.randint(2,4)): screen.blit(get_explosion_frame(es), (sx-es//2+random.randint(-10,10), sy-es//2+random.randint(-10,10)))
        if zoom >= 1.5:
            pass  # Stats shown via hover tooltip now
MERCHANT_MAX_TRAVEL = config.MERCHANT_MAX_TRAVEL; UNIT_MAX_RANGE = config.UNIT_MAX_RANGE; NATIVE_MAX_RANGE = config.NATIVE_MAX_RANGE
class Merchant:
    def __init__(self, source, destination):
        self.source, self.destination = source, destination; self.owner_faction = source["owner"]
        self.x, self.y = float(source["x"]), float(source["y"]); self.material = source["material"]
        self.luxury = _city_luxury_resource(source)  # luxury resource from source city (or None)
        self.amount, self.speed, self.size = 5, 0.18, 3; self.status = "Travelling"; self.rest_timer = 0; self.trade_timer = 0
        self.is_ship = False; self.ship_timer = 0
        self._stuck_x = self.x; self._stuck_y = self.y; self._stuck_ticks = 0; self._trace_side = 1
        self._land_waypoints = []
    def update(self):
        if self.status == "Idle":
            # Look for a valid trade destination
            self.rest_timer += game_speed
            if self.rest_timer >= DAY_TICKS * 5:
                self.rest_timer = 0
                dest = self._find_trade_destination()
                if dest:
                    self.source = self.destination if self.destination else self.source
                    self.destination = dest; self.status = "Travelling"
        elif self.status == "Resting":
            self.rest_timer += game_speed
            if self.rest_timer >= DAY_TICKS * 5:
                self.rest_timer = 0
                dest = self._find_trade_destination()
                if dest:
                    self.source = self.destination; self.destination = dest; self.status = "Travelling"
                else:
                    self.status = "Idle"
        elif self.status == "Trading":
            self.trade_timer += game_speed
            if self.trade_timer >= DAY_TICKS * 2:
                self.trade_timer = 0
                dest_owner = self.destination["owner"]
                # Market bonus: +25% merchant income if source city has Market
                _market_mult = 1.25 if _city_has_building(self.source, "Market") else 1.0
                if dest_owner == self.owner_faction:
                    # Domestic trade: 5 gold
                    faction_materials[self.owner_faction]["Gold"] += int(5 * _market_mult)
                else:
                    # Foreign trade: 8 gold + resource sale
                    faction_materials[self.owner_faction]["Gold"] += int(8 * _market_mult)
                    mat = self.material
                    if mat in ("Lumber", "Hide"):
                        if faction_materials[self.owner_faction][mat] >= 5:
                            faction_materials[self.owner_faction][mat] -= 5
                            faction_materials[dest_owner]["Gold"] -= 5
                            faction_materials[dest_owner][mat] = faction_materials[dest_owner].get(mat, 0) + 5
                            faction_materials[self.owner_faction]["Gold"] += 5
                    elif mat in ("Coal", "Iron"):
                        if faction_materials[self.owner_faction][mat] >= 3:
                            faction_materials[self.owner_faction][mat] -= 3
                            faction_materials[dest_owner]["Gold"] -= 5
                            faction_materials[dest_owner][mat] = faction_materials[dest_owner].get(mat, 0) + 3
                            faction_materials[self.owner_faction]["Gold"] += 5
                    # Luxury resource trade: sell 1 unit for 8 gold
                    if self.luxury and faction_materials[self.owner_faction].get(self.luxury, 0) >= 1:
                        faction_materials[self.owner_faction][self.luxury] -= 1
                        faction_materials[dest_owner][self.luxury] = faction_materials[dest_owner].get(self.luxury, 0) + 1
                        faction_materials[dest_owner]["Gold"] -= 8
                        faction_materials[self.owner_faction]["Gold"] += int(8 * _market_mult)
                self.status = "Resting"
        elif self.status == "Travelling":
            # Enroute cost: 3 gold per day
            self.trade_timer += game_speed
            if self.trade_timer >= DAY_TICKS:
                self.trade_timer = 0
                if self.owner_faction not in NATIVE_FACTIONS:
                    faction_materials[self.owner_faction]["Gold"] -= 3
            # Auto-detect ship/canoe mode — disembark near cities
            near_dest = math.hypot(self.destination["x"]-self.x, self.destination["y"]-self.y) < 10
            if near_dest:
                self.is_ship = False
            else:
                self.is_ship = _is_water(self.x, self.y)
            dx, dy = self.destination["x"]-self.x, self.destination["y"]-self.y; dist = math.hypot(dx, dy)
            spd = self.speed * game_speed
            # Biome movement modifier for merchants
            if not self.is_ship:
                spd *= _get_biome_move_mult(self.x, self.y)
            if self.is_ship: spd *= 2  # faster on water
            if dist > 5:
                ndx, ndy = dx/dist, dy/dist
                self.x += ndx*spd; self.y += ndy*spd
            else: self.status = "Trading"; self.trade_timer = 0
    def _find_trade_destination(self):
        """Find a valid trade destination. Foreign nations must have >= 5 gold to buy."""
        mats = faction_materials[self.owner_faction]
        mat = self.material
        # Check if we have resources to sell
        has_resources = True
        if mat in ("Lumber", "Hide") and mats[mat] < 5: has_resources = False
        elif mat in ("Coal", "Iron") and mats[mat] < 3: has_resources = False
        candidates = []
        for c in cities:
            if c is self.source and c is self.destination: continue
            if at_war(self.owner_faction, c["owner"]): continue
            if math.hypot(c["x"]-self.x, c["y"]-self.y) > MERCHANT_MAX_TRAVEL: continue
            if c["owner"] == self.owner_faction:
                # Domestic trade always valid
                candidates.append(c)
            elif has_resources and faction_materials[c["owner"]]["Gold"] >= 5:
                # Foreign trade: they need gold to buy
                candidates.append(c)
        if candidates:
            return random.choice(candidates)
        return None
    def draw(self):
        sx, sy = world_to_screen(self.x, self.y); col = FACTION_COLORS.get(self.owner_faction, (0,200,0)); r = max(4, int(self.size*zoom))
        if not _on_screen(sx, sy): return  # off-screen: skip drawing (position still tracked)
        # Draw as canoe for native merchants on water
        if self.is_ship and self.owner_faction in NATIVE_FACTIONS and _canoe_anims:
            dx = self.destination["x"] - self.x; dy = self.destination["y"] - self.y
            if abs(dx) > abs(dy):
                if dx > 0: frames = _canoe_anims["br"] if dy > 10 else (_canoe_anims["tr"] if dy < -10 else _canoe_anims["right"])
                else: frames = _canoe_anims["bl"] if dy > 10 else (_canoe_anims["tl"] if dy < -10 else _canoe_anims["left"])
            else:
                if dy > 0: frames = _canoe_anims["down"]
                else: frames = _canoe_anims["up"]
            idx = (_brit_anim_tick // 8) % len(frames); frame = frames[idx]
            sz = max(6, int(9*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2)); return
        # Draw as ship when in ship mode
        if self.is_ship and _ship_anims:
            dx = self.destination["x"] - self.x; dy = self.destination["y"] - self.y
            if abs(dx) > abs(dy):
                if dx > 0: frames = _ship_anims["br"] if dy > 10 else (_ship_anims["tr"] if dy < -10 else _ship_anims["right"])
                else: frames = _ship_anims["bl"] if dy > 10 else (_ship_anims["tl"] if dy < -10 else _ship_anims["left"])
            else:
                if dy > 0: frames = _ship_anims["down"]
                else: frames = _ship_anims["up"]
            idx = (_brit_anim_tick // 8) % len(frames); frame = frames[idx]
            sz = max(10, int(15*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2)); return
        elif self.ship_timer > 0:
            r2 = max(2, int(3*zoom))
            pygame.draw.circle(screen, (139, 90, 43), (sx, sy), r2); return
        # Use sprite for Haiti merchants
        elif self.owner_faction == "Haiti" and _haiti_merch_anims:
            if self.status == "Trading":
                frames = _haiti_merch_anims["trading"]; spd = 6
            elif self.status == "Resting":
                frames = _haiti_merch_anims["resting"]; spd = 10
            else:
                dx = self.destination["x"] - self.x; dy = self.destination["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = _haiti_merch_anims["walk_right"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = _haiti_merch_anims["walk_br"] if dy > 20 else (_haiti_merch_anims["walk_tr"] if dy < -20 else _haiti_merch_anims["walk_right"])
                    else:
                        frames = _haiti_merch_anims["walk_bl"] if dy > 20 else (_haiti_merch_anims["walk_tl"] if dy < -20 else _haiti_merch_anims["walk_left"])
                else:
                    if dy > 0: frames = _haiti_merch_anims["walk_down"]
                    else: frames = _haiti_merch_anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if self.status in ("Trading", "Resting"):
                sz = max(6, int(8*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        # Use sprite for non-native merchants
        elif self.owner_faction not in NATIVE_FACTIONS and _merch_anims:
            if self.status == "Trading":
                frames = _merch_anims["trading"]; spd = 6
            elif self.status == "Resting":
                frames = _merch_anims["resting"]; spd = 10
            else:
                # Determine direction from movement
                dx = self.destination["x"] - self.x; dy = self.destination["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = _merch_anims["walk_right"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = _merch_anims["walk_br"] if dy > 20 else (_merch_anims["walk_tr"] if dy < -20 else _merch_anims["walk_right"])
                    else:
                        frames = _merch_anims["walk_bl"] if dy > 20 else (_merch_anims["walk_tl"] if dy < -20 else _merch_anims["walk_left"])
                else:
                    if dy > 0: frames = _merch_anims["walk_down"]
                    else: frames = _merch_anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if self.status in ("Trading", "Resting"):
                sz = max(6, int(8*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        elif self.owner_faction in NATIVE_FACTIONS and _natmerch_anims:
            # Use native merchant sprite
            if self.status == "Trading":
                frames = _natmerch_anims["trading"]; spd = 6
            elif self.status == "Resting":
                frames = _natmerch_anims["resting"]; spd = 10
            else:
                dx = self.destination["x"] - self.x; dy = self.destination["y"] - self.y
                if abs(dx) < 5 and abs(dy) < 5: frames = _natmerch_anims["walk_right"]; spd = 1
                elif abs(dx) > abs(dy):
                    if dx > 0:
                        frames = _natmerch_anims["walk_br"] if dy > 20 else (_natmerch_anims["walk_tr"] if dy < -20 else _natmerch_anims["walk_right"])
                    else:
                        frames = _natmerch_anims["walk_bl"] if dy > 20 else (_natmerch_anims["walk_tl"] if dy < -20 else _natmerch_anims["walk_left"])
                else:
                    if dy > 0: frames = _natmerch_anims["walk_down"]
                    else: frames = _natmerch_anims["walk_up"]
                spd = 8
            idx = (_brit_anim_tick // spd) % len(frames)
            frame = frames[idx]
            if self.status in ("Trading", "Resting"):
                sz = max(6, int(8*zoom))
            else:
                sz = max(3, int(5*zoom))
            ratio = frame.get_height() / max(1, frame.get_width())
            scaled = pygame.transform.scale(frame, (sz, int(sz*ratio)))
            screen.blit(scaled, (sx-sz//2, sy-scaled.get_height()//2))
        else:
            pygame.draw.polygon(screen, col, [(sx, sy-r), (sx-r, sy+r), (sx+r, sy+r)])
        if zoom >= 1.5:
            pass  # Stats shown via hover tooltip now
FORT_NAMES = {"France":["Fort Bourbon","Fort Royal","Fort Louis","Fort Conde","Fort Chartres"],"Spain":["Fuerte San Miguel","Fuerte Santa Ana","Fuerte del Rey","Presidio Real"],"Great Britain":["Fort William","Fort George","Fort Cumberland","Fort Frederick"],"Russia":["Fort Ross","Fort Mikhailovsky","Redoubt St. Dionysius","Fort Alexander"],"Denmark":["Fort Christianshaab","Fort Frederikshaab","Fort Julianeshaab","Fort Godthaab","Fort Egedesminde"],"Iroquois":["Kahnawake","Akwesasne","Tyendinaga","Wahta","Kanehsatake"],"Wabanaki":["Meductic","Panawahpskek","Sipayik","Peskedemakad","Wolinak"],"Comanche":["Quahadi Camp","Nokoni Camp","Penateka Camp","Kotsoteka Camp","Yamparika Camp"],"Cree":["Mistawasis","Pimicikamak","Wapaskwaw","Maskwa Sipi","K\u00e2w\u00e2siskw\u00e2w","W\u00e2pamew","Sak\u00e2wiyiniwak","Nistawâyaw","Paskwâw"],"United States":["Fort Ticonderoga","Fort Moultrie","Fort Washington","Fort Mercer","Fort Mifflin"],"Pirates":["Pirate Cove","Skull Bay","Blackbeard's Haven","Tortuga Hideout","Dead Man's Port"],"Mexico":["Fuerte de San Carlos","Fuerte de Guadalupe","Fuerte de Loreto","Presidio del Norte","Fuerte de San Juan"],"Haiti":["Fort Liberté","Fort Jacques","Fort Dimanche","Fort Picolet","Fort Drouet"],"Texas":["Fort Alamo","Fort Worth","Fort Bend","Fort Parker","Fort Houston"],"Confederate States":["Fort Sumter","Fort Monroe","Fort Henry","Fort Donelson","Fort Fisher"]}
class Settler:
    MIN_SETTLE_DIST = 60; MAX_SETTLE_DIST = 250; NATIVE_MAX_SETTLE = 120
    def __init__(self, owner_faction, x, y):
        self.owner_faction = owner_faction; self.x, self.y = float(x), float(y)
        self.size, self.speed = 3, 0.12; self.target_x, self.target_y = self._pick_settle_spot(); self.settled = False
        self._land_waypoints = []; self._trace_side = 1
        self._stuck_x = self.x; self._stuck_y = self.y; self._stuck_ticks = 0
    def _pick_settle_spot(self):
        all_pos = [(c["x"], c["y"]) for c in cities]
        max_dist = self.NATIVE_MAX_SETTLE if self.owner_faction in NATIVE_FACTIONS else self.MAX_SETTLE_DIST
        all_strategic = list(_MOUNTAIN_SPRITES)
        random.shuffle(all_strategic)
        for px, py in all_strategic:
            angle = random.uniform(0, 2*math.pi); offset = random.uniform(25, 40)
            ox, oy = px + math.cos(angle)*offset, py + math.sin(angle)*offset
            ox, oy = max(20, min(_MAP_NATIVE_W-20, ox)), max(20, min(_MAP_NATIVE_H-20, oy))
            if _is_water(ox, oy): continue  # don't settle on water
            if any(math.hypot(ox-cx, oy-cy) < self.MIN_SETTLE_DIST for cx, cy in all_pos): continue
            own_cities = [c for c in cities if c["owner"]==self.owner_faction]
            if own_cities and not any(math.hypot(ox-c["x"], oy-c["y"]) < max_dist for c in own_cities): continue
            if any(math.hypot(ox-c["x"], oy-c["y"]) < 60 for c in cities if c["owner"]!=self.owner_faction): continue
            return ox, oy
        # Fallback — find any valid land point nearby
        for _ in range(20):
            angle = random.uniform(0, 2*math.pi); dist = random.uniform(40, 100)
            nx, ny = self.x + math.cos(angle)*dist, self.y + math.sin(angle)*dist
            nx, ny = max(20, min(_MAP_NATIVE_W-20, nx)), max(20, min(_MAP_NATIVE_H-20, ny))
            if not _is_water(nx, ny):
                if not any(math.hypot(nx-c["x"], ny-c["y"]) < self.MIN_SETTLE_DIST for c in cities):
                    return nx, ny
        return self.x + random.uniform(-30, 30), self.y + random.uniform(-30, 30)
    def update(self):
        if self.settled: return
        dx, dy = self.target_x-self.x, self.target_y-self.y; dist = math.hypot(dx, dy)
        if dist > 5:
            spd = self.speed*game_speed
            ndx, ndy = dx/dist, dy/dist
            self.x += ndx*spd; self.y += ndy*spd
        else: self.establish_fort()
    def establish_fort(self):
        # Can't settle on water or too close to existing cities
        if _is_water(self.x, self.y):
            if self in settlers: settlers.remove(self); return
        if any(math.hypot(self.x-c["x"], self.y-c["y"]) < 50 for c in cities):
            if self in settlers: settlers.remove(self); return
        is_native = self.owner_faction in NATIVE_FACTIONS
        if is_native:
            # Natives settle camps — square, low health, destroyable
            name = random_comanche_settlement() if self.owner_faction == "Comanche" else (random_cree_settlement() if self.owner_faction == "Cree" else (random_dakota_settlement() if self.owner_faction == "Dakota" else random.choice(FORT_NAMES[self.owner_faction])))
            cities.append({"name":name,"x":self.x,"y":self.y,"color":FACTION_COLORS[self.owner_faction],"troops":30,"strategy":random.choice(strategies),"owner":self.owner_faction,"sovereign":self.owner_faction,"occupier":None,"material":random.choice(["Lumber","Hide"]),"is_capital":False,"tier":0,"is_village":False,"is_fort":True,"is_camp":True,"spawn_cd":0})
        else:
            cities.append({"name":random.choice(FORT_NAMES[self.owner_faction]),"x":self.x,"y":self.y,"color":FACTION_COLORS[self.owner_faction],"troops":75,"strategy":random.choice(strategies),"owner":self.owner_faction,"sovereign":self.owner_faction,"occupier":None,"material":random.choice(city_materials),"is_capital":False,"tier":0,"is_village":False,"is_fort":True,"is_camp":False,"spawn_cd":0})
        if self in settlers: settlers.remove(self)
    def draw(self):
        sx, sy = world_to_screen(self.x, self.y)
        if not _on_screen(sx, sy): return  # off-screen: skip drawing (position still tracked)
        pygame.draw.circle(screen, FACTION_COLORS.get(self.owner_faction, (150,0,200)), (sx,sy), max(2, int(self.size*zoom)))
        if zoom >= 1.3:
            draw_outlined_text(small_font, "Settler", (255,255,255), (sx, sy-max(10, int(12*zoom))), anchor="center")
            draw_outlined_text(small_font, self.owner_faction, FACTION_COLORS.get(self.owner_faction, (255,255,255)), (sx, sy+max(6, int(8*zoom))), anchor="center")



# ====================================================================
# === COMBAT & ENCOUNTERS ============================================
# ====================================================================
def check_battles():
    for a in units[:]:
        if a.fighting or a.retreating or a.hp < a.max_hp * 0.3: continue
        for b in units[:]:
            if b.fighting: continue
            if a != b and a.owner["owner"]!=b.owner["owner"] and math.hypot(a.x-b.x, a.y-b.y) < 15:
                if b.retreating: _add_war_fatigue(b.owner["owner"], a.owner["owner"]); (units.remove(b) if b in units else None)
                else: a.fight(b)
                break
        if a not in units or a.retreating or a.fighting: continue
        for m in merchants[:]:
            if m.owner_faction != a.owner["owner"] and at_war(a.owner["owner"], m.owner_faction) and math.hypot(a.x-m.x, a.y-m.y) < 15:
                # Merchants in ship mode can defend themselves
                if m.is_ship:
                    # Naval combat — merchant fights back
                    a.hp -= random.randint(1, 3)
                    m_hp = getattr(m, '_combat_hp', 30)
                    m_hp -= random.randint(2, max(2, a.power))
                    m._combat_hp = m_hp
                    if m_hp <= 0:
                        # Merchant sunk
                        if m in merchants: merchants.remove(m)
                    elif a.hp <= a.max_hp * 0.3:
                        a._start_retreat_to_city()
                else:
                    # Normal capture on land
                    captor, old_owner = a.owner["owner"], m.owner_faction
                    if sum(1 for mx in merchants if mx.owner_faction==captor) >= _get_merchant_cap(captor):
                        if m in merchants: merchants.remove(m)
                    else:
                        m.owner_faction = captor; m.status = "Travelling"
                        others = [c for c in cities if c["owner"]==captor and c is not m.destination]
                        if others: m.destination = random.choice(others)
                break
    # Pirates seize nearby merchants regardless of peace (short radius)
    if "Pirates" not in eliminated_factions:
        pirate_units = [u for u in units if u.owner["owner"] == "Pirates" and not u.retreating and not u.fighting]
        pirate_cities_pos = [(c["x"], c["y"]) for c in cities if c["owner"] == "Pirates"]
        for m in merchants[:]:
            if m.owner_faction == "Pirates": continue
            # Check if merchant is near a pirate unit (radius 40)
            seized = False
            for pu in pirate_units:
                if math.hypot(pu.x - m.x, pu.y - m.y) < 40:
                    # Ship-mode merchants fight back against pirates
                    if m.is_ship:
                        pu.hp -= random.randint(1, 3)
                        m_hp = getattr(m, '_combat_hp', 30)
                        m_hp -= random.randint(2, max(2, pu.power))
                        m._combat_hp = m_hp
                        if m_hp <= 0:
                            faction_materials["Pirates"]["Gold"] += 3
                            if m in merchants: merchants.remove(m)
                            news(f"Pirates sink a {m.owner_faction} merchant ship!")
                        seized = True; break
                    else:
                        # Normal seizure on land/near city
                        mat = m.material if hasattr(m, 'material') and m.material else "Gold"
                        faction_materials["Pirates"][mat] = faction_materials["Pirates"].get(mat, 0) + 3
                        faction_materials["Pirates"]["Gold"] += 2
                        if m in merchants: merchants.remove(m)
                        news(f"Pirates seize a {m.owner_faction} merchant!")
                        seized = True; break
            if seized: continue
            # Also check if merchant passes near a pirate city (radius 50)
            for px, py in pirate_cities_pos:
                if math.hypot(px - m.x, py - m.y) < 50:
                    mat = m.material if hasattr(m, 'material') and m.material else "Gold"
                    faction_materials["Pirates"][mat] = faction_materials["Pirates"].get(mat, 0) + 3
                    faction_materials["Pirates"]["Gold"] += 2
                    if m in merchants: merchants.remove(m)
                    news(f"Pirates seize a {m.owner_faction} merchant near their waters!")
                    break
def check_merchant_encounters():
    for a in merchants[:]:
        if a not in merchants: continue
        for b in merchants[:]:
            if b not in merchants or a is b or a.owner_faction==b.owner_faction: continue
            if math.hypot(a.x-b.x, a.y-b.y) < 12:
                if random.random() < 0.1:
                    winner, loser = (a, b) if random.random() < 0.5 else (b, a)
                    faction_materials[winner.owner_faction]["Gold"] += 5
                    if loser in merchants: merchants.remove(loser)
                break
iron_day_counter = 0  # legacy, unused

# ====================================================================
# === ECONOMY UPDATES ================================================
# ====================================================================
# Resource production timers (real-time frames at 60fps)
_food_timer = 0; _FOOD_INTERVAL = 24     # food every 0.4 seconds (frequent small ticks)
_lumber_timer = 0; _LUMBER_INTERVAL = 180  # lumber every 3 seconds
_hide_timer = 0; _HIDE_INTERVAL = 240     # hide every 4 seconds
_iron_timer = 0; _IRON_INTERVAL = 360     # iron every 6 seconds

def update_materials():
    global _food_timer, _lumber_timer, _hide_timer, _iron_timer
    _food_timer += 1; _lumber_timer += 1; _hide_timer += 1; _iron_timer += 1

    if _food_timer >= _FOOD_INTERVAL:
        _food_timer = 0
        for city in cities:
            if is_flooded(city): continue
            base = 8
            # Colonial factions get +25% food production
            if city["owner"] not in NATIVE_FACTIONS: base = 10
            biome = _get_biome(city["x"], city["y"])
            food_mult = _BIOME_MODIFIERS[biome][1]
            # Food production policy modifier
            _fp_name, _fp_mod, _fp_hap = _get_food_policy(city["owner"])
            food_mult *= (1.0 + _fp_mod)
            # Agrarian governor trait: +30% food
            if "Agrarian" in city.get("_governor_traits", []): food_mult *= 1.3
            # Farm building: doubles food production
            if _city_has_building(city, "Farm"): food_mult *= 2.0
            # Laws tiered food production bonus
            if city["owner"] not in NATIVE_FACTIONS:
                _laws = _get_faction_laws(city["owner"])
                _demos = _get_city_demographics(city)
                for _g, _pct in _demos.items():
                    _eth_st = _laws["ethnic"].get(_g, 0)
                    if _eth_st == 1:  # Segregated — small food bonus
                        food_mult += _pct * 0.0008
                    elif _eth_st == 3:  # Enslaved — plantation labor
                        food_mult += _pct * 0.002
            faction_materials[city["owner"]]["Food"] += int(base * food_mult)

    if _lumber_timer >= _LUMBER_INTERVAL:
        _lumber_timer = 0
        for city in cities:
            if is_flooded(city): continue
            if city["material"] == "Lumber":
                biome = _get_biome(city["x"], city["y"])
                lumber_mult = _BIOME_MODIFIERS[biome][2]
                if lumber_mult > 0:
                    base_lumber = 3
                    if _city_has_building(city, "Lumber Mill"): base_lumber *= 2
                    faction_materials[city["owner"]]["Lumber"] += int(base_lumber * lumber_mult)

    if _hide_timer >= _HIDE_INTERVAL:
        _hide_timer = 0
        for city in cities:
            if is_flooded(city): continue
            if city["material"] == "Hide":
                base = 1
                biome = _get_biome(city["x"], city["y"])
                hide_mult = _BIOME_MODIFIERS[biome][3]
                faction_materials[city["owner"]]["Hide"] += int(base * hide_mult)

    if _iron_timer >= _IRON_INTERVAL:
        _iron_timer = 0
        for city in cities:
            if is_flooded(city): continue
            if city["material"] == "Iron":
                iron_amt = 2 if _city_has_building(city, "Foundry") else 1
                coal_amt = 2 if _city_has_building(city, "Foundry") else 1
                faction_materials[city["owner"]]["Iron"] += iron_amt
                # Coal is produced alongside Iron
                faction_materials[city["owner"]]["Coal"] = faction_materials[city["owner"]].get("Coal", 0) + coal_amt

    # Luxury resource production (same interval as lumber — every 10 seconds)
    if _lumber_timer == 0:  # piggyback on lumber timer reset
        for city in cities:
            if is_flooded(city): continue
            lux = _city_luxury_resource(city)
            if lux:
                faction_materials[city["owner"]][lux] = faction_materials[city["owner"]].get(lux, 0) + 1

def update_gold_tax():
    """Gold tax runs once per in-game day, modified by biome and Town Hall (+25%). Base taxed at 75%."""
    for city in cities:
        if is_flooded(city): continue
        owner = city["owner"]
        biome = _get_biome(city["x"], city["y"])
        gold_mult = _BIOME_MODIFIERS[biome][4] * 0.75  # 25% less overall tax
        # Production policy modifier
        _gp_name, _gp_mod, _gp_hap = _get_gold_policy(owner)
        gold_mult *= (1.0 + _gp_mod)
        # Town Hall / Council Lodge bonus: +25% gold tax
        if _city_has_building(city, "Town Hall") or _city_has_building(city, "Council Lodge"):
            gold_mult *= 1.25
        # Governor trait gold modifiers
        traits = city.get("_governor_traits", [])
        if "Economist" in traits: gold_mult *= 1.2  # +20% gold
        if "Embezzler" in traits: gold_mult *= 0.8  # -20% gold
        if "Smuggler" in traits: gold_mult *= 1.15  # +15% gold (but occasional incident handled elsewhere)
        if "Corrupt" in traits and random.random() < 0.1: gold_mult *= 0.5  # 10% chance to siphon half
        # Laws production bonus: tiered labor system
        if owner not in NATIVE_FACTIONS:
            laws = _get_faction_laws(owner)
            demos = _get_city_demographics(city)
            for g, pct in demos.items():
                eth_status = laws["ethnic"].get(g, 0)
                if eth_status == 1:  # Segregated — cheap labor, small bonus
                    gold_mult += pct * 0.001  # up to +10% for 100% segregated
                elif eth_status == 3:  # Enslaved — forced labor, large bonus
                    gold_mult += pct * 0.003  # up to +30% for 100% enslaved
        if city.get("is_village"):
            if owner not in NATIVE_FACTIONS:
                faction_materials[owner]["Gold"] += int(2 * gold_mult)
        elif city.get("is_fort") or city.get("is_camp"):
            faction_materials[owner]["Gold"] += int(2 * gold_mult)
        else:
            faction_materials[owner]["Gold"] += int(5 * gold_mult)
def update_upkeep():
    for faction in active_factions():
        mats = faction_materials[faction]
        # Unit upkeep: colonial infantry 2g+3f, cavalry 4g+5f; native warrior 1f
        for u in [u for u in units if u.owner["owner"]==faction]:
            if faction in NATIVE_FACTIONS:
                g_cost, f_cost = 0, 1
            elif u.formation == "Square":
                g_cost, f_cost = 4, 5
            else:
                g_cost, f_cost = 2, 3
            if mats["Gold"] >= g_cost and mats["Food"] >= f_cost:
                mats["Gold"] -= g_cost; mats["Food"] -= f_cost
            else:
                u.hp -= 2
                if u.hp <= 0: u.hp = max(1, u.hp); u._start_retreat_to_city()
        # Merchant upkeep: colonial 1g+1f; native 1f
        for m in [m for m in merchants if m.owner_faction==faction]:
            if faction in NATIVE_FACTIONS:
                g_cost, f_cost = 0, 1
            else:
                g_cost, f_cost = 1, 1
            if mats["Gold"] >= g_cost and mats["Food"] >= f_cost:
                mats["Gold"] -= g_cost; mats["Food"] -= f_cost
            else:
                if m in merchants: merchants.remove(m)
def _get_food_income_per_day(faction):
    """Estimate food income per in-game day based on city count and production rate."""
    # Food produces every 60 frames (1 sec), DAY_TICKS=12 frames per day at 60fps = 0.2 sec per day
    # So food ticks ~5 times per in-game day (60fps / 12 ticks per day = 5 real seconds per day... actually 12/60 = 0.2s per day)
    # At 60fps, 1 in-game day = 12 frames. Food interval = 60 frames. So food fires every 5 in-game days.
    # Per food tick: 20 per city (40 native). Days between ticks: 60/12 = 5 days.
    # So per day: 20/5 = 4 food per city per day (8 for natives)
    city_count = sum(1 for c in cities if c["owner"]==faction)
    per_city = 4
    return city_count * per_city

def _can_sustain_new_unit(faction, formation):
    """Check if faction has enough food reserves to sustain another unit. Manpower is the main gate."""
    mats = faction_materials[faction]
    # Simple check: do we have at least 20 food? (won't immediately starve)
    if mats.get("Food", 0) < 20: return False
    # Don't recruit if gold is too low
    if faction not in NATIVE_FACTIONS and mats.get("Gold", 0) < 30: return False
    return True

def _can_sustain_new_merchant(faction):
    """Check if faction's food/gold income can cover total upkeep including the new merchant."""
    mats = faction_materials[faction]
    existing_units = [u for u in units if u.owner["owner"]==faction]
    if faction in NATIVE_FACTIONS:
        total_food_upkeep = len(existing_units) * 2
        merchant_count = sum(1 for m in merchants if m.owner_faction==faction)
        total_food_upkeep += (merchant_count + 1) * 1
        food_income = _get_food_income_per_day(faction)
        if total_food_upkeep > food_income: return False
        return mats["Food"] >= total_food_upkeep * 3
    else:
        total_gold_upkeep = sum(6 if u.formation == "Square" else 3 for u in existing_units)
        total_food_upkeep = sum(8 if u.formation == "Square" else 5 for u in existing_units)
        merchant_count = sum(1 for m in merchants if m.owner_faction==faction)
        total_gold_upkeep += (merchant_count + 1) * 2
        total_food_upkeep += (merchant_count + 1) * 1
        food_income = _get_food_income_per_day(faction)
        if total_food_upkeep > food_income: return False
        return mats["Gold"] >= total_gold_upkeep * 3 and mats["Food"] >= total_food_upkeep * 3

def can_afford(owner, cost): return all(faction_materials[owner][r] >= a for r, a in cost.items())

def _pick_formation(faction, city):
    """Pick a valid formation for spawning at a city. Cavalry requires Stables."""
    if faction == "Comanche": return "Square"  # Comanche always horsemen
    if faction == "Dakota": return random.choice(["Line", "Line", "Line", "Square"])  # mostly bowmen, some horsemen
    if faction == "Cree": return "Line"  # Cree always bowmen
    if faction == "Pirates": return random.choice(["Line", "Column"])
    if faction in NATIVE_FACTIONS: return "Line"
    # Colonial: cavalry only if city has Stables
    has_stables = _city_has_building(city, "Stables")
    available = ["Line", "Column"]
    if has_stables:
        available.append("Square")
    return random.choice(available)

def _unit_cost(faction, formation):
    """Unit cost: mainly manpower (handled separately). This is just the equipping fee."""
    if faction in NATIVE_FACTIONS:
        return {"Food": 5}  # natives just need food to mobilize
    return dict(FORMATION_COSTS[formation])
def update_city_regen():
    for city in cities:
        if city.get("is_camp"): base = 30
        elif city.get("is_village"): base = 50
        elif city["is_capital"]: base = 200
        else: base = 100
        max_troops = base + city["tier"]*25
        owner = city["owner"]
        if city["troops"] >= max_troops: continue
        if owner in NATIVE_FACTIONS:
            # Natives heal with food and hide
            if faction_materials[owner]["Food"] >= 10 and faction_materials[owner]["Hide"] >= 5:
                faction_materials[owner]["Food"] -= 10; faction_materials[owner]["Hide"] -= 5
                city["troops"] = max_troops
        else:
            # Colonial: 50 gold + 10 lumber to fully heal
            if faction_materials[owner]["Gold"] >= 50 and faction_materials[owner]["Lumber"] >= 10:
                faction_materials[owner]["Gold"] -= 50; faction_materials[owner]["Lumber"] -= 10
                city["troops"] = max_troops

# Passive city regen: 2 troops per second after 10 seconds of no attacks
_city_regen_timer = 0
_CITY_REGEN_INTERVAL = 60  # 1 second at 60fps

def update_passive_city_regen():
    """Tick passive regen for all cities. 2 troops/sec if not attacked for 10 seconds."""
    global _city_regen_timer
    _city_regen_timer += 1
    if _city_regen_timer < _CITY_REGEN_INTERVAL: return
    _city_regen_timer = 0
    for city in cities:
        # Tick down cooldown
        if city.get("_regen_cooldown", 0) > 0:
            city["_regen_cooldown"] -= _CITY_REGEN_INTERVAL
            continue
        # Passive regen: +2 troops per second
        if city.get("is_camp"): base = 30
        elif city.get("is_village"): base = 50
        elif city["is_capital"]: base = 200
        else: base = 100
        max_troops = base + city.get("tier", 0) * 25
        # Fortifier governor trait: +25 max garrison
        traits = city.get("_governor_traits", [])
        if "Fortifier" in traits: max_troops += 25
        # Regen amount: base 2, Recruiter doubles it
        regen_amount = 4 if "Recruiter" in traits else 2
        # Lazy governor slows regen
        if "Lazy" in traits: regen_amount = max(1, regen_amount - 1)
        if city["troops"] < max_troops:
            city["troops"] = min(max_troops, city["troops"] + regen_amount)

def update_unit_upgrades():
    for faction in active_factions():
        mats = faction_materials[faction]
        fu = [u for u in units if u.owner["owner"]==faction and u.tier<3 and not u.retreating and not u.fighting]
        if not fu: continue
        u = random.choice(fu); nt = u.tier+1; ic, cc = nt, max(0, nt-1)
        if mats["Iron"] >= ic and mats["Coal"] >= cc:
            mats["Iron"] -= ic; mats["Coal"] -= cc; u.tier = nt; u.power += 2; u.max_hp += 10; u.hp = min(u.hp+10, u.max_hp)
CITY_UPGRADE_COSTS = {1:{"Iron":2,"Lumber":10,"Gold":50}, 2:{"Iron":5,"Lumber":25,"Gold":120}, 3:{"Iron":8,"Lumber":40,"Gold":200}}
def update_city_upgrades():
    for faction in active_factions():
        mats = faction_materials[faction]
        # Cities need Town Hall (colonial) or Council Lodge (native) to upgrade
        if faction in NATIVE_FACTIONS:
            fc = [c for c in cities if c["owner"]==faction and c["tier"]<3 and _city_has_building(c, "Council Lodge")]
        else:
            fc = [c for c in cities if c["owner"]==faction and c["tier"]<3 and _city_has_building(c, "Town Hall")]
        if not fc: continue
        # Upgrade all affordable cities, not just one random one
        for city in fc:
            nt = city["tier"]+1; cost = CITY_UPGRADE_COSTS[nt]
            if all(mats[r] >= a for r, a in cost.items()):
                for r, a in cost.items(): mats[r] -= a
                city["tier"] = nt; base = 30 if city.get("is_camp") else (50 if city.get("is_village") else (200 if city["is_capital"] else 100))
                city["troops"] = min(city["troops"]+25, base+nt*25)
def pay(owner, cost):
    for r, a in cost.items(): faction_materials[owner][r] -= a

# ====================================================================
# === FACTIONS PANEL & UI ============================================
# ====================================================================
def _visible_factions():
    """Factions shown on the board — hide Pirates if they have no cities."""
    return [f for f in active_factions() if f != "Pirates" or any(c["owner"] == "Pirates" for c in cities)]

# === Ruler Portrait System ===
# {faction: [(start_year, end_year, image_path, ruler_name), ...]}
from rulers import _RULERS, _RULER_TRAITS  # extracted ruler data

# Ruler Traits — affect the entire nation while that ruler is in power
RULER_TRAITS = {
    # Military traits
    "Conqueror": {"desc": "+25% unit power", "color": (220, 60, 60)},
    "Militarist": {"desc": "+2 unit cap", "color": (200, 80, 80)},
    "Pacifist": {"desc": "-25% war declaration chance", "color": (100, 200, 100)},
    "Defensive": {"desc": "+30% garrison strength", "color": (140, 160, 200)},
    "Naval Supremacy": {"desc": "+50% ship speed", "color": (60, 140, 220)},
    # Economic traits
    "Mercantilist": {"desc": "+20% gold income", "color": (255, 220, 50)},
    "Spendthrift": {"desc": "-15% gold income", "color": (200, 150, 50)},
    "Industrialist": {"desc": "+30% material production", "color": (180, 140, 80)},
    "Patron of Trade": {"desc": "+2 merchant cap", "color": (220, 180, 80)},
    "Taxman": {"desc": "+10% gold but -2 happiness all cities", "color": (180, 180, 50)},
    # Diplomatic traits
    "Diplomat": {"desc": "+30% alliance chance", "color": (100, 200, 255)},
    "Expansionist": {"desc": "+50% settler spawn rate", "color": (200, 150, 100)},
    "Isolationist": {"desc": "No alliances, +10% defense", "color": (150, 150, 180)},
    "Imperialist": {"desc": "+30% war score from claims", "color": (180, 80, 180)},
    # Domestic traits
    "Enlightened": {"desc": "+3 happiness all cities", "color": (255, 255, 150)},
    "Tyrant": {"desc": "-5 happiness, +20% production", "color": (160, 40, 40)},
    "Reformer": {"desc": "Suppressed groups emigrate faster", "color": (150, 220, 200)},
    "Beloved": {"desc": "+5 happiness all cities", "color": (100, 255, 150)},
    "Unpopular": {"desc": "-3 happiness all cities", "color": (180, 100, 100)},
    "Pious": {"desc": "+2 happiness from majority religion", "color": (220, 200, 255)},
    # Negative/quirky traits
    "Mad": {"desc": "Random policy changes, unstable", "color": (200, 50, 200)},
    "Weak": {"desc": "-15% unit power, -10% gold", "color": (140, 140, 140)},
    "Lavish": {"desc": "-20% gold, +2 happiness capital", "color": (255, 180, 220)},
    "Young": {"desc": "No bonuses or penalties", "color": (180, 220, 180)},
    "Aged": {"desc": "-10% production, stable policies", "color": (160, 160, 140)},
}

# Traits assigned to each ruler based on historical character
_ruler_img_cache = {}
_ruler_frame_img = None

def _get_current_ruler(faction):
    """Get the current ruler image and name for a faction based on game year."""
    if faction not in _RULERS: return None, None
    for start, end, path, name in _RULERS[faction]:
        if start <= game_year < end:
            return path, name
    # If past all rulers, use the last one
    rulers = _RULERS[faction]
    if rulers and game_year >= rulers[-1][0]:
        return rulers[-1][2], rulers[-1][3]
    return None, None

def _draw_ruler_portrait(owner, panel_x, panel_y, panel_w, panel_h):
    """Draw ruler portrait in its own panel below the faction info panel."""
    global _ruler_frame_img
    ruler_path, ruler_name = _get_current_ruler(owner)
    if not ruler_path: return
    # Load/cache ruler image
    if ruler_path not in _ruler_img_cache:
        try:
            _ruler_img_cache[ruler_path] = pygame.image.load(ruler_path).convert_alpha()
        except:
            _ruler_img_cache[ruler_path] = None
    if _ruler_frame_img is None:
        try: _ruler_frame_img = pygame.image.load("images/frame.webp").convert_alpha()
        except: _ruler_frame_img = pygame.Surface((60, 60), pygame.SRCALPHA)
    ruler_img = _ruler_img_cache.get(ruler_path)
    if not ruler_img: return
    # Own panel below the faction panel
    portrait_size = 140
    rp_w = portrait_size + 40; rp_h = portrait_size + 120
    rp_x = panel_x
    rp_y = panel_y + panel_h + 6
    # Background
    rp_surf = pygame.Surface((rp_w, rp_h), pygame.SRCALPHA); rp_surf.fill((10, 10, 10, 200))
    screen.blit(rp_surf, (rp_x, rp_y))
    _draw_panel_frame(rp_x, rp_y, rp_w, rp_h)
    # Ruler image centered
    px = rp_x + (rp_w - portrait_size) // 2
    py = rp_y + 10
    scaled_ruler = pygame.transform.scale(ruler_img, (portrait_size, portrait_size))
    screen.blit(scaled_ruler, (px, py))
    # Frame around ruler
    frame = pygame.transform.scale(_ruler_frame_img, (portrait_size + 40, portrait_size + 40))
    screen.blit(frame, (px - 20, py - 20))
    # Ruler name below portrait
    _ruler_font = pygame.font.SysFont(None, 20, bold=True)
    name_surf = _ruler_font.render(ruler_name, True, (220, 200, 150))
    screen.blit(name_surf, (rp_x + (rp_w - name_surf.get_width()) // 2, py + portrait_size + 6))
    # Ruler traits below name
    traits = _RULER_TRAITS.get(ruler_name, [])
    if traits:
        _trait_font = pygame.font.SysFont(None, 16)
        _desc_font = pygame.font.SysFont(None, 13)
        ty = py + portrait_size + 22
        for trait in traits:
            tdata = RULER_TRAITS.get(trait, {"desc": "", "color": (200, 200, 200)})
            trait_surf = _trait_font.render(f"{trait}", True, tdata["color"])
            screen.blit(trait_surf, (rp_x + (rp_w - trait_surf.get_width()) // 2, ty))
            ty += 15
            desc_surf = _desc_font.render(tdata["desc"], True, (150, 150, 150))
            screen.blit(desc_surf, (rp_x + (rp_w - desc_surf.get_width()) // 2, ty))
            ty += 14

def draw_factions():
    """Show faction stats panel in top-left when a city is selected. Highlights all cities of that faction."""
    if not _selected_city: return pygame.Rect(0,0,1,1)
    owner = _selected_city["owner"]
    # Highlight all cities of this faction on the map with a glow ring
    for c in cities:
        if c["owner"] == owner:
            sx, sy = world_to_screen(c["x"], c["y"])
            col = FACTION_COLORS.get(owner, (200, 200, 200))
            glow = pygame.Surface((60, 60), pygame.SRCALPHA)
            for r in range(25, 15, -2):
                alpha = max(10, 100 - (r - 15) * 8)
                pygame.draw.circle(glow, (col[0], col[1], col[2], alpha), (30, 30), r, 2)
            screen.blit(glow, (sx - 30, sy - 30))
    # Draw faction info panel top-left
    panel_w, panel_h = 420, 180
    panel = pygame.Surface((panel_w, panel_h), pygame.SRCALPHA); panel.fill((0,0,0,180)); screen.blit(panel, (6, 6))
    _draw_panel_frame(6, 6, panel_w, panel_h)
    # Flag and name
    flag = faction_images.get(owner)
    if flag:
        big_flag = pygame.transform.scale(flag, (120, 120))
        screen.blit(big_flag, (12, 30))
    tx = 140  # text x offset (right of flag)
    draw_outlined_text(font, owner, FACTION_COLORS.get(owner, (255,255,255)), (tx, 14))
    # Units and merchants
    ucount = sum(1 for u in units if u.owner["owner"]==owner)
    ucap = _get_unit_cap(owner)
    mcount = sum(1 for m in merchants if m.owner_faction==owner)
    mcap = _get_merchant_cap(owner)
    city_count = sum(1 for c in cities if c["owner"]==owner)
    draw_outlined_text(small_font, f"Cities: {city_count}  Units: {ucount}/{ucap}  Merchants: {mcount}/{mcap}", (255,255,255), (tx, 52))
    # Resources
    mats = faction_materials.get(owner, {})
    y = 70
    res_line1 = f"Gold: {mats.get('Gold',0)}  Food: {mats.get('Food',0)}  Lumber: {mats.get('Lumber',0)}"
    res_line2 = f"Hide: {mats.get('Hide',0)}  Iron: {mats.get('Iron',0)}  Coal: {mats.get('Coal',0)}"
    draw_outlined_text(small_font, res_line1, (255, 220, 100), (tx, y))
    draw_outlined_text(small_font, res_line2, (255, 220, 100), (tx, y+16))
    # Manpower (below materials)
    manpower = _faction_manpower.get(owner, 0)
    draw_outlined_text(small_font, f"Manpower: {manpower}", (180, 220, 180), (tx, y+32))
    # Luxury resources
    luxes = [l for l in LUXURY_RESOURCES if mats.get(l, 0) > 0]
    if luxes:
        lux_str = "  ".join(f"{l}: {mats[l]}" for l in luxes)
        draw_outlined_text(small_font, lux_str, (255, 200, 255), (tx, y+48))
    # Wars/alliances
    war_list = [tuple(k) for k in wars if owner in k]
    ally_list = [tuple(k) for k in alliances if owner in k]
    wy = y + 50
    if war_list:
        enemies = [b if a == owner else a for a, b in war_list]
        draw_outlined_text(small_font, f"At war: {', '.join(enemies)}", (255, 80, 80), (tx, wy)); wy += 16
    if ally_list:
        allies = [b if a == owner else a for a, b in ally_list]
        draw_outlined_text(small_font, f"Allied: {', '.join(allies)}", (100, 255, 100), (tx, wy))
    # Ruler portrait (hide when laws panel is open to make space)
    if not _laws_panel_open:
        _draw_ruler_portrait(owner, 6, 6, panel_w, panel_h)
    # Laws button
    global _laws_btn_rect
    _laws_btn_rect = pygame.Rect(12, panel_h - 18, 60, 20)
    btn_col = (80, 140, 200) if _laws_panel_open else (50, 80, 120)
    pygame.draw.rect(screen, btn_col, (6 + _laws_btn_rect.x, 6 + _laws_btn_rect.y, _laws_btn_rect.w, _laws_btn_rect.h), border_radius=4)
    pygame.draw.rect(screen, (120, 180, 255), (6 + _laws_btn_rect.x, 6 + _laws_btn_rect.y, _laws_btn_rect.w, _laws_btn_rect.h), 1, border_radius=4)
    _laws_font = pygame.font.SysFont(None, 16, bold=True)
    lbl = _laws_font.render("Laws", True, (255, 255, 255))
    screen.blit(lbl, (6 + _laws_btn_rect.x + (_laws_btn_rect.w - lbl.get_width())//2, 6 + _laws_btn_rect.y + 3))
    # Draw laws panel if open
    if _laws_panel_open and owner not in NATIVE_FACTIONS:
        _draw_laws_panel(owner)
    return pygame.Rect(0,0,1,1)

_laws_btn_rect = pygame.Rect(0, 0, 1, 1)

# Ethnicity icon images cache
_ethnicity_icons = {}
_laws_grassland_bg = None
_laws_frame_img = None
_laws_view = "tabs"  # "tabs" | "ethnic_list" | "religion_list" | "ethnic_detail"
_laws_selected_group = None

def _init_laws_assets():
    """Load ethnicity icons and grassland background for laws panel."""
    global _ethnicity_icons, _laws_grassland_bg, _laws_frame_img
    if _ethnicity_icons: return
    _eth_files = {
        "British": "images/ethnicities/commonbritish.png",
        "French": "images/ethnicities/commonfrench.png",
        "Spanish": "images/ethnicities/commonspanish.png",
        "African": "images/ethnicities/commonafricans.png",
        "Indigenous": "images/ethnicities/commonindigenous.png",
        "Russian": "images/ethnicities/commonrussians.png",
        "Danish": "images/ethnicities/commondanes.png",
        "Latin American": "images/ethnicities/commonlatinamericans.png",
        "American": "images/ethnicities/commonamericans.png",
    }
    for name, path in _eth_files.items():
        try:
            _ethnicity_icons[name] = pygame.image.load(path).convert_alpha()
        except:
            s = pygame.Surface((32, 32), pygame.SRCALPHA)
            pygame.draw.circle(s, _DEMOG_GROUPS.get(name, (200, 200, 200)), (16, 16), 14)
            _ethnicity_icons[name] = s
    try: _laws_grassland_bg = pygame.image.load("images/biomes/grassland.jpg").convert()
    except: _laws_grassland_bg = pygame.Surface((200, 150)); _laws_grassland_bg.fill((60, 80, 40))
    try: _laws_frame_img = pygame.image.load("images/frame.webp").convert_alpha()
    except: _laws_frame_img = pygame.Surface((200, 150), pygame.SRCALPHA)

def _draw_laws_panel(faction):
    """Draw laws panel with tab navigation."""
    global _law_click_rects
    _init_laws_assets()
    _law_click_rects = []
    laws = _get_faction_laws(faction)

    if _laws_view == "tabs":
        _draw_laws_tabs(faction)
    elif _laws_view == "ethnic_list":
        _draw_laws_ethnic_list(faction, laws)
    elif _laws_view == "religion_list":
        _draw_laws_religion_list(faction, laws)
    elif _laws_view == "production_list":
        _draw_laws_production_list(faction)
    elif _laws_view == "military_list":
        _draw_laws_military_list(faction)
    elif _laws_view == "ethnic_detail" and _laws_selected_group:
        _draw_laws_ethnic_detail(faction, laws, _laws_selected_group)

def _draw_laws_tabs(faction):
    """Draw the tab buttons: Ethnic Laws / Religion Laws / Production / Military."""
    lp_w = 200; lp_h = 130
    lp_x = 6; lp_y = 192
    lp_surf = pygame.Surface((lp_w, lp_h), pygame.SRCALPHA); lp_surf.fill((10, 10, 10, 220))
    screen.blit(lp_surf, (lp_x, lp_y))
    _draw_panel_frame(lp_x, lp_y, lp_w, lp_h)
    _title_font = pygame.font.SysFont(None, 18, bold=True)
    # Ethnic Laws button
    eth_rect = pygame.Rect(lp_x + 10, lp_y + 10, lp_w - 20, 22)
    pygame.draw.rect(screen, (40, 40, 40), eth_rect, border_radius=4)
    pygame.draw.rect(screen, (180, 150, 60), eth_rect, 1, border_radius=4)
    eth_lbl = _title_font.render("Ethnic Laws", True, (255, 220, 100))
    screen.blit(eth_lbl, (eth_rect.x + (eth_rect.w - eth_lbl.get_width()) // 2, eth_rect.y + 3))
    _law_click_rects.append(("tab_ethnic", "", eth_rect, faction))
    # Religion Laws button
    rel_rect = pygame.Rect(lp_x + 10, lp_y + 38, lp_w - 20, 22)
    pygame.draw.rect(screen, (40, 40, 40), rel_rect, border_radius=4)
    pygame.draw.rect(screen, (180, 150, 60), rel_rect, 1, border_radius=4)
    rel_lbl = _title_font.render("Religion Laws", True, (255, 220, 100))
    screen.blit(rel_lbl, (rel_rect.x + (rel_rect.w - rel_lbl.get_width()) // 2, rel_rect.y + 3))
    _law_click_rects.append(("tab_religion", "", rel_rect, faction))
    # Production button
    prod_rect = pygame.Rect(lp_x + 10, lp_y + 66, lp_w - 20, 22)
    pygame.draw.rect(screen, (40, 40, 40), prod_rect, border_radius=4)
    pygame.draw.rect(screen, (180, 150, 60), prod_rect, 1, border_radius=4)
    prod_lbl = _title_font.render("Production", True, (255, 220, 100))
    screen.blit(prod_lbl, (prod_rect.x + (prod_rect.w - prod_lbl.get_width()) // 2, prod_rect.y + 3))
    _law_click_rects.append(("tab_production", "", prod_rect, faction))
    # Military button
    mil_rect = pygame.Rect(lp_x + 10, lp_y + 94, lp_w - 20, 22)
    pygame.draw.rect(screen, (40, 40, 40), mil_rect, border_radius=4)
    pygame.draw.rect(screen, (180, 150, 60), mil_rect, 1, border_radius=4)
    mil_lbl = _title_font.render("Military", True, (255, 220, 100))
    screen.blit(mil_lbl, (mil_rect.x + (mil_rect.w - mil_lbl.get_width()) // 2, mil_rect.y + 3))
    _law_click_rects.append(("tab_military", "", mil_rect, faction))

def _draw_laws_production_list(faction):
    """Draw production policies (Gold Tax, Food Labor)."""
    lp_w = 220; lp_h = 100
    lp_x = 6; lp_y = 192
    lp_surf = pygame.Surface((lp_w, lp_h), pygame.SRCALPHA); lp_surf.fill((10, 10, 10, 220))
    screen.blit(lp_surf, (lp_x, lp_y))
    _draw_panel_frame(lp_x, lp_y, lp_w, lp_h)
    _title_font = pygame.font.SysFont(None, 18, bold=True)
    _lp_font = pygame.font.SysFont(None, 15)
    dy = lp_y + 8
    screen.blit(_title_font.render("Production", True, (255, 220, 100)), (lp_x + 10, dy))
    # Back button
    back_rect = pygame.Rect(lp_x + lp_w - 40, dy - 2, 30, 16)
    pygame.draw.rect(screen, (60, 60, 60), back_rect, border_radius=3)
    screen.blit(_lp_font.render("Back", True, (200, 200, 200)), (back_rect.x + 3, back_rect.y + 1))
    _law_click_rects.append(("back", "", back_rect, faction))
    dy += 24
    # Gold policy
    gp_name, gp_mod, gp_hap = _get_gold_policy(faction)
    gp_col = (100, 255, 100) if gp_hap >= 0 else ((255, 220, 50) if gp_hap >= -2 else (255, 100, 100))
    screen.blit(_lp_font.render("Gold Tax", True, (255, 220, 100)), (lp_x + 12, dy))
    status_surf = _lp_font.render(gp_name, True, gp_col)
    screen.blit(status_surf, (lp_x + lp_w - status_surf.get_width() - 12, dy))
    dy += 20
    # Food policy
    fp_name, fp_mod, fp_hap = _get_food_policy(faction)
    fp_col = (100, 255, 100) if fp_hap >= 0 else ((255, 220, 50) if fp_hap >= -2 else (255, 100, 100))
    screen.blit(_lp_font.render("Food Labor", True, (255, 220, 100)), (lp_x + 12, dy))
    status_surf = _lp_font.render(fp_name, True, fp_col)
    screen.blit(status_surf, (lp_x + lp_w - status_surf.get_width() - 12, dy))

def _draw_laws_military_list(faction):
    """Draw military policies (Conscription)."""
    lp_w = 220; lp_h = 80
    lp_x = 6; lp_y = 192
    lp_surf = pygame.Surface((lp_w, lp_h), pygame.SRCALPHA); lp_surf.fill((10, 10, 10, 220))
    screen.blit(lp_surf, (lp_x, lp_y))
    _draw_panel_frame(lp_x, lp_y, lp_w, lp_h)
    _title_font = pygame.font.SysFont(None, 18, bold=True)
    _lp_font = pygame.font.SysFont(None, 15)
    dy = lp_y + 8
    screen.blit(_title_font.render("Military", True, (255, 220, 100)), (lp_x + 10, dy))
    # Back button
    back_rect = pygame.Rect(lp_x + lp_w - 40, dy - 2, 30, 16)
    pygame.draw.rect(screen, (60, 60, 60), back_rect, border_radius=3)
    screen.blit(_lp_font.render("Back", True, (200, 200, 200)), (back_rect.x + 3, back_rect.y + 1))
    _law_click_rects.append(("back", "", back_rect, faction))
    dy += 24
    # Conscription policy
    con_name, con_rate, con_hap = _get_conscription_policy(faction)
    con_col = (100, 255, 100) if con_hap >= 0 else ((255, 220, 50) if con_hap >= -2 else (255, 100, 100))
    screen.blit(_lp_font.render("Conscription", True, (255, 220, 100)), (lp_x + 12, dy))
    status_surf = _lp_font.render(con_name, True, con_col)
    screen.blit(status_surf, (lp_x + lp_w - status_surf.get_width() - 12, dy))

def _draw_laws_ethnic_list(faction, laws):
    """Draw list of ethnic groups with their status — click one to see detail."""
    lp_w = 220; lp_h = 220
    lp_x = 6; lp_y = 192
    lp_surf = pygame.Surface((lp_w, lp_h), pygame.SRCALPHA); lp_surf.fill((10, 10, 10, 220))
    screen.blit(lp_surf, (lp_x, lp_y))
    _draw_panel_frame(lp_x, lp_y, lp_w, lp_h)
    _title_font = pygame.font.SysFont(None, 18, bold=True)
    _lp_font = pygame.font.SysFont(None, 15)
    dy = lp_y + 8
    screen.blit(_title_font.render("Ethnic Laws", True, (255, 220, 100)), (lp_x + 10, dy))
    # Back button
    back_rect = pygame.Rect(lp_x + lp_w - 40, dy - 2, 30, 16)
    pygame.draw.rect(screen, (60, 60, 60), back_rect, border_radius=3)
    screen.blit(_lp_font.render("Back", True, (200, 200, 200)), (back_rect.x + 3, back_rect.y + 1))
    _law_click_rects.append(("back", "", back_rect, faction))
    dy += 22
    for group in _LAW_ETHNIC_GROUPS:
        status_idx = laws["ethnic"].get(group, 0)
        status = ETHNIC_LAW_STATUSES[status_idx]
        status_col = ETHNIC_LAW_COLORS[status]
        eth_col = _DEMOG_GROUPS.get(group, (200, 200, 200))
        row_rect = pygame.Rect(lp_x + 8, dy - 2, lp_w - 16, 18)
        # Hover highlight area
        pygame.draw.rect(screen, (30, 30, 30), row_rect, border_radius=3)
        screen.blit(_lp_font.render(group, True, eth_col), (lp_x + 12, dy))
        status_surf = _lp_font.render(status, True, status_col)
        screen.blit(status_surf, (lp_x + lp_w - status_surf.get_width() - 12, dy))
        _law_click_rects.append(("ethnic_detail", group, row_rect, faction))
        dy += 20

def _draw_laws_religion_list(faction, laws):
    """Draw list of religions with their status."""
    lp_w = 220; lp_h = 140
    lp_x = 6; lp_y = 192
    lp_surf = pygame.Surface((lp_w, lp_h), pygame.SRCALPHA); lp_surf.fill((10, 10, 10, 220))
    screen.blit(lp_surf, (lp_x, lp_y))
    _draw_panel_frame(lp_x, lp_y, lp_w, lp_h)
    _title_font = pygame.font.SysFont(None, 18, bold=True)
    _lp_font = pygame.font.SysFont(None, 15)
    dy = lp_y + 8
    screen.blit(_title_font.render("Religion Laws", True, (255, 220, 100)), (lp_x + 10, dy))
    # Back button
    back_rect = pygame.Rect(lp_x + lp_w - 40, dy - 2, 30, 16)
    pygame.draw.rect(screen, (60, 60, 60), back_rect, border_radius=3)
    screen.blit(_lp_font.render("Back", True, (200, 200, 200)), (back_rect.x + 3, back_rect.y + 1))
    _law_click_rects.append(("back", "", back_rect, faction))
    dy += 22
    for rel_name in _LAW_RELIGIONS:
        status_idx = laws["religion"].get(rel_name, 0)
        status = RELIGION_LAW_STATUSES[status_idx]
        col = RELIGION_LAW_COLORS[status]
        rel_col = _RELIGION_COLORS.get(rel_name, (200, 200, 200))
        screen.blit(_lp_font.render(rel_name, True, rel_col), (lp_x + 12, dy))
        status_surf = _lp_font.render(status, True, col)
        screen.blit(status_surf, (lp_x + lp_w - status_surf.get_width() - 12, dy))
        dy += 20

def _draw_laws_ethnic_detail(faction, laws, group):
    """Draw detail view: grassland background with frame, ethnic image centered inside."""
    lp_w = 240; lp_h = 220
    lp_x = 6; lp_y = 192
    # Grassland background
    bg_w = lp_w - 20; bg_h = lp_h - 60
    bg_x = lp_x + 10; bg_y = lp_y + 10
    bg = pygame.transform.scale(_laws_grassland_bg, (bg_w, bg_h))
    screen.blit(bg, (bg_x, bg_y))
    # Frame around the grassland — bigger to wrap around it
    frame = pygame.transform.scale(_laws_frame_img, (bg_w + 32, bg_h + 32))
    screen.blit(frame, (bg_x - 16, bg_y - 16))
    # Ethnic group image centered on the grassland
    icon = _ethnicity_icons.get(group)
    icon_size = 100
    icon_x = bg_x + (bg.get_width() - icon_size) // 2
    icon_y = bg_y + (bg.get_height() - icon_size) // 2 - 10
    if icon:
        scaled_icon = pygame.transform.scale(icon, (icon_size, icon_size))
        screen.blit(scaled_icon, (icon_x, icon_y))
    # Group name and status below the grassland area
    _title_font = pygame.font.SysFont(None, 20, bold=True)
    _status_font = pygame.font.SysFont(None, 17, bold=True)
    _info_font = pygame.font.SysFont(None, 14)
    eth_col = _DEMOG_GROUPS.get(group, (200, 200, 200))
    name_surf = _title_font.render(group, True, eth_col)
    screen.blit(name_surf, (lp_x + (lp_w - name_surf.get_width()) // 2, lp_y + lp_h - 42))
    status_idx = laws["ethnic"].get(group, 0)
    status = ETHNIC_LAW_STATUSES[status_idx]
    status_col = ETHNIC_LAW_COLORS[status]
    status_surf = _status_font.render(status, True, status_col)
    screen.blit(status_surf, (lp_x + (lp_w - status_surf.get_width()) // 2, lp_y + lp_h - 22))
    # Back button
    back_rect = pygame.Rect(lp_x + lp_w - 40, lp_y + lp_h - 20, 30, 16)
    pygame.draw.rect(screen, (60, 60, 60), back_rect, border_radius=3)
    screen.blit(_info_font.render("Back", True, (200, 200, 200)), (back_rect.x + 3, back_rect.y + 1))
    _law_click_rects.append(("back_to_ethnic", "", back_rect, faction))

_law_click_rects = []

def _handle_laws_click(mx, my):
    """Handle clicking in the laws panel — navigate tabs and detail views."""
    global _laws_panel_open, _laws_view, _laws_selected_group
    # Check Laws button in faction panel
    if _selected_city:
        btn = pygame.Rect(6 + _laws_btn_rect.x, 6 + _laws_btn_rect.y, _laws_btn_rect.w, _laws_btn_rect.h)
        if btn.collidepoint(mx, my):
            _laws_panel_open = not _laws_panel_open
            _laws_view = "tabs"
            _laws_selected_group = None
            return True
    # Check clicks within laws panel
    if _laws_panel_open:
        for entry in _law_click_rects:
            kind, name, rect, faction = entry
            if rect.collidepoint(mx, my):
                if kind == "tab_ethnic":
                    _laws_view = "ethnic_list"
                    return True
                elif kind == "tab_religion":
                    _laws_view = "religion_list"
                    return True
                elif kind == "tab_production":
                    _laws_view = "production_list"
                    return True
                elif kind == "tab_military":
                    _laws_view = "military_list"
                    return True
                elif kind == "ethnic_detail":
                    _laws_view = "ethnic_detail"
                    _laws_selected_group = name
                    return True
                elif kind == "back":
                    _laws_view = "tabs"
                    return True
                elif kind == "back_to_ethnic":
                    _laws_view = "ethnic_list"
                    _laws_selected_group = None
                    return True
    return False
MISSISSIPPI = [(x*_COORD_SCALE,y*_COORD_SCALE) for x,y in [(311,352),(306,345),(311,336),(306,327),(310,319),(304,310),(309,301),(306,296),(301,288),(298,279),(294,270),(297,262),(293,253),(290,244),(286,235),(284,225)]]
APPALACHIAN_POINTS = [(x*_COORD_SCALE,y*_COORD_SCALE) for x,y in [(347,318),(358,314),(353,304),(363,300),(359,289),(370,284),(365,275),(377,269)]]
SIERRA_MADRE_POINTS = [(x*_COORD_SCALE,y*_COORD_SCALE) for x,y in [(173,301),(186,295),(162,294),(177,286),(155,285),(168,279),(147,281),(160,273),(139,277),(152,269)]]
ALL_MOUNTAINS = APPALACHIAN_POINTS + SIERRA_MADRE_POINTS; MOUNTAIN_RADIUS = 8*_COORD_SCALE; RIVER_RADIUS = 6*_COORD_SCALE
RIO_GRANDE = [(x*_COORD_SCALE,y*_COORD_SCALE) for x,y in [(258,392),(255,388),(252,384),(249,380),(248,375),(246,370),(243,366),(238,361),(231,358),(224,357),(220,355),(221,351),(223,346),(224,341),(223,337),(220,332),(215,328),(211,324),(208,320),(204,316),(202,312),(203,307),(204,303),(204,298),(205,293),(207,288),(209,283)]]
# Bounding boxes for each mountain range — units deflect around the whole range

# ====================================================================
# === TERRAIN (Mountains, Rivers) ====================================
# ====================================================================
def _mountain_range_bounds():
    ranges = [APPALACHIAN_POINTS, SIERRA_MADRE_POINTS]
    bounds = []
    for pts in ranges:
        if not pts: continue
        xs = [p[0] for p in pts]; ys = [p[1] for p in pts]
        bounds.append((min(xs)-MOUNTAIN_RADIUS, min(ys)-MOUNTAIN_RADIUS, max(xs)+MOUNTAIN_RADIUS, max(ys)+MOUNTAIN_RADIUS))
    return bounds
MOUNTAIN_BOUNDS = _mountain_range_bounds()
def _inside_mountain_range(x, y):
    for (x1, y1, x2, y2) in MOUNTAIN_BOUNDS:
        if x1 <= x <= x2 and y1 <= y <= y2:
            if any(math.hypot(x-mx, y-my) < MOUNTAIN_RADIUS for mx, my in ALL_MOUNTAINS): return True
    return False
def _near_mountain(x, y):
    """A position is 'mountainous' if its climate tile is the mountain biome."""
    return _get_biome(x, y) == "mountains"
def _near_mountain_ahead(x, y, dx, dy, spd):
    """Check multiple steps ahead for mountain-biome tiles in path."""
    for step in range(1, 4):
        nx, ny = x+dx*spd*step, y+dy*spd*step
        if _get_biome(nx, ny) == "mountains": return True
    return False
def _near_river(x, y): return False  # rivers removed
def _deflect_around_mountain(x, y, dx, dy):
    """Route around the mountain range by finding the closest exit direction."""
    # Find which range we're near
    for i, (x1, y1, x2, y2) in enumerate(MOUNTAIN_BOUNDS):
        cx, cy = (x1+x2)/2, (y1+y2)/2
        if math.hypot(x-cx, y-cy) < max(x2-x1, y2-y1):
            # Find nearest edge of bounding box and head toward it
            dist_left = abs(x - x1); dist_right = abs(x - x2)
            dist_top = abs(y - y1); dist_bottom = abs(y - y2)
            min_dist = min(dist_left, dist_right, dist_top, dist_bottom)
            if min_dist == dist_left: ex, ey = x1 - 30, y
            elif min_dist == dist_right: ex, ey = x2 + 30, y
            elif min_dist == dist_top: ex, ey = x, y1 - 30
            else: ex, ey = x, y2 + 30
            # Direction toward the nearest edge
            edx, edy = ex-x, ey-y; ed = math.hypot(edx, edy) or 1
            return edx/ed, edy/ed
    # Fallback: push away from nearest mountain point
    mx, my = min(ALL_MOUNTAINS, key=lambda p: math.hypot(x-p[0], y-p[1]))
    ax, ay = x-mx, y-my; d = math.hypot(ax, ay) or 1
    return ax/d, ay/d
mountain_img = pygame.transform.scale(pygame.image.load("images/mountain.webp"), (16, 16))

# Mountain sprite positions are derived from the mountain biome (brown) in climates.png.
# We scan the climate map on a coarse grid and drop a sprite on each mountain-biome cell.
_MOUNTAIN_SPRITES = []  # list of (wx, wy) native world coords
def _build_mountain_sprites():
    global _MOUNTAIN_SPRITES
    pts = []
    step = 6 * _COORD_SCALE  # spacing between mountain sprites (native px)
    half = step // 2
    y = half
    while y < _MAP_NATIVE_H:
        x = half
        while x < _MAP_NATIVE_W:
            if _get_biome(x, y) == "mountains" and _is_land(x, y):
                # slight jitter so the range doesn't look like a rigid grid
                jx = x + random.randint(-half//2, half//2)
                jy = y + random.randint(-half//2, half//2)
                pts.append((jx, jy))
            x += step
        y += step
    _MOUNTAIN_SPRITES = pts
_build_mountain_sprites()

def _draw_mountains():
    if not _MOUNTAIN_SPRITES: return
    W, H = get_screen_size()
    w, h = max(9, int(14*zoom)), max(6, int(10*zoom))
    mimg = pygame.transform.scale(mountain_img, (w, h))
    for x, y in _MOUNTAIN_SPRITES:
        sx, sy = world_to_screen(x, y)
        if -w <= sx <= W+w and -h <= sy <= H+h:  # viewport cull
            screen.blit(mimg, (sx-w//2, sy-h//2))
def _draw_mississippi(): pass   # rivers removed
def _draw_rio_grande(): pass    # rivers removed

# ====================================================================
# === CITY RENDERING =================================================
# ====================================================================
# Plantation sprites around cities
_plantation_img = None
_city_plantation_positions = {}  # {city_id: [(wx, wy), ...]}

def _init_plantation_img():
    global _plantation_img
    if _plantation_img: return
    try:
        _plantation_img = pygame.image.load("images/buildings/plantation.png").convert_alpha()
    except:
        _plantation_img = None

def _get_plantation_positions(city):
    """Generate random land positions around a city for plantation sprites."""
    cid = id(city)
    if cid in _city_plantation_positions:
        return _city_plantation_positions[cid]
    positions = []
    cx, cy = city["x"], city["y"]
    city_region = city.get("_region", _region_at(cx, cy))
    # Place 1 plantation near the city (radius scaled to the enlarged native map).
    attempts = 0
    while len(positions) < 1 and attempts < 60:
        attempts += 1
        angle = random.uniform(0, math.pi * 2)
        dist = random.uniform(8, 18) * _COORD_SCALE
        px = cx + math.cos(angle) * dist
        py = cy + math.sin(angle) * dist
        # Must be on land and within (native) map bounds
        if px < 5 or px > _MAP_NATIVE_W - 5 or py < 5 or py > _MAP_NATIVE_H - 5: continue
        if _is_water(px, py): continue
        # Must stay inside the same territory/region as the city.
        if city_region >= 0 and _region_at(px, py) != city_region: continue
        positions.append((px, py))
    _city_plantation_positions[cid] = positions
    return positions

def _draw_city_plantations(city):
    """Draw plantation sprites around a city that has a Plantation building."""
    if not _city_has_building(city, "Plantation"): return
    _csx, _csy = world_to_screen(city["x"], city["y"])
    if not _on_screen(_csx, _csy, margin=120): return  # off-screen: skip
    _init_plantation_img()
    if not _plantation_img: return
    positions = _get_plantation_positions(city)
    ps = max(3, int(4 * zoom))  # plantation sprite size
    scaled = pygame.transform.scale(_plantation_img, (ps, ps))
    for wx, wy in positions:
        sx, sy = world_to_screen(wx, wy)
        screen.blit(scaled, (sx - ps // 2, sy - ps // 2))

# Farm sprites around cities
_farm_imgs = {}  # {faction_prefix: [img1, img2, img3]}
_city_farm_data = {}  # {city_id: (wx, wy, img_index)}

def _init_farm_imgs():
    if _farm_imgs: return
    _farm_factions = {
        "brit": ("Great Britain", "United States", "Texas", "Confederate States"),
        "fren": ("France", "Haiti"),
        "span": ("Spain", "Mexico"),
        "rus": ("Russia",),
        "dan": ("Denmark",),
    }
    for prefix, factions in _farm_factions.items():
        imgs = []
        for i in range(1, 4):
            try:
                img = pygame.image.load(f"images/buildings/{prefix}farm{i}.png").convert_alpha()
                imgs.append(img)
            except:
                pass
        if imgs:
            for f in factions:
                _farm_imgs[f] = imgs

def _get_farm_data(city):
    """Get or generate a farm position and random image variant for a city."""
    cid = id(city)
    if cid in _city_farm_data:
        return _city_farm_data[cid]
    cx, cy = city["x"], city["y"]
    city_region = city.get("_region", _region_at(cx, cy))
    # Find a valid land position near the city (radius scaled to the enlarged native map)
    for _ in range(60):
        angle = random.uniform(0, math.pi * 2)
        dist = random.uniform(8, 18) * _COORD_SCALE
        px = cx + math.cos(angle) * dist
        py = cy + math.sin(angle) * dist
        if px < 5 or px > _MAP_NATIVE_W - 5 or py < 5 or py > _MAP_NATIVE_H - 5: continue
        if _is_water(px, py): continue
        # Must stay inside the same territory/region as the city.
        if city_region >= 0 and _region_at(px, py) != city_region: continue
        # Don't overlap with plantation position
        plant_pos = _city_plantation_positions.get(cid, [])
        if any(math.hypot(px - ox, py - oy) < 8 * _COORD_SCALE for ox, oy in plant_pos): continue
        img_idx = random.randint(0, 2)
        _city_farm_data[cid] = (px, py, img_idx)
        return _city_farm_data[cid]
    _city_farm_data[cid] = None
    return None

def _draw_city_farm(city):
    """Draw a farm sprite around a city that has a Farm building."""
    if not _city_has_building(city, "Farm"): return
    if city.get("is_fort") or city.get("is_camp"): return
    _csx, _csy = world_to_screen(city["x"], city["y"])
    if not _on_screen(_csx, _csy, margin=120): return  # off-screen: skip
    _init_farm_imgs()
    owner = city["owner"]
    imgs = _farm_imgs.get(owner)
    if not imgs: return
    data = _get_farm_data(city)
    if not data: return
    wx, wy, img_idx = data
    img = imgs[img_idx % len(imgs)]
    fs = max(3, int(4 * zoom))
    sx, sy = world_to_screen(wx, wy)
    iw, ih = img.get_width(), img.get_height()
    ratio = ih / max(1, iw)
    sw = fs
    sh = max(2, int(fs * ratio))
    scaled = pygame.transform.scale(img, (sw, sh))
    screen.blit(scaled, (sx - sw // 2, sy - sh // 2))

# Smoke effect for damaged/recently sieged cities on the map
_smoke_frames = []
_smoke_initialized = False

def _init_smoke():
    global _smoke_frames, _smoke_initialized
    if _smoke_initialized: return
    _smoke_initialized = True
    try:
        from PIL import Image as PILImage
        pil_img = PILImage.open("images/smoke.gif")
        try:
            while True:
                frame = pil_img.convert("RGBA").resize((16, 20), PILImage.LANCZOS)
                _smoke_frames.append(pygame.image.fromstring(frame.tobytes(), (16, 20), "RGBA"))
                pil_img.seek(pil_img.tell() + 1)
        except EOFError:
            pass
    except ImportError:
        try:
            _smoke_frames.append(pygame.transform.scale(pygame.image.load("images/smoke.gif").convert_alpha(), (16, 20)))
        except:
            pass
    if not _smoke_frames:
        s = pygame.Surface((16, 20), pygame.SRCALPHA)
        pygame.draw.ellipse(s, (100, 100, 100, 80), (2, 2, 12, 16))
        _smoke_frames.append(s)

def _draw_city_smoke(city):
    """Draw semi-transparent animated smoke over damaged/sieged cities."""
    if city.get("is_camp"): max_hp = 30
    elif city.get("is_village"): max_hp = 50
    elif city.get("is_capital"): max_hp = 200
    else: max_hp = 100
    max_hp += city.get("tier", 0) * 25
    health_pct = city["troops"] / max(1, max_hp)
    # Show smoke if below 50% health or under siege cooldown
    if health_pct > 0.5 and city.get("_regen_cooldown", 0) <= 0: return
    _init_smoke()
    if not _smoke_frames: return
    sx, sy = world_to_screen(city["x"], city["y"])
    if not _on_screen(sx, sy, margin=120): return  # off-screen: skip
    frame_idx = (_brit_anim_tick // 10) % len(_smoke_frames)
    frame = _smoke_frames[frame_idx]
    smoke_size = max(4, int(7 * zoom))
    scaled = pygame.transform.scale(frame, (smoke_size, int(smoke_size * 1.3)))
    # Make semi-transparent so flags are still visible
    smoke_surf = scaled.copy()
    smoke_surf.set_alpha(255)
    if health_pct < 0.25:
        # Heavy damage — 2 plumes
        screen.blit(smoke_surf, (sx - smoke_size - 6, sy - int(smoke_size * 1.5)))
        screen.blit(smoke_surf, (sx - smoke_size // 2 - 3, sy - int(smoke_size * 1.2)))
    else:
        # Light smoke — 1 plume
        screen.blit(smoke_surf, (sx - smoke_size - 4, sy - int(smoke_size * 1.4)))

def draw_city(city):
    sx, sy = world_to_screen(city["x"], city["y"])
    if not _on_screen(sx, sy, margin=120): return  # off-screen city: skip drawing
    owner = city["owner"]
    orig_sovereign = city.get("_original_sovereign", city.get("sovereign", owner))
    # For native settlements taken by colonials, use the colonizer's building image
    if orig_sovereign in NATIVE_FACTIONS and owner not in NATIVE_FACTIONS:
        sovereign = owner  # colonized — looks like the new owner's architecture
    else:
        sovereign = orig_sovereign  # keep original look
    is_native = sovereign in NATIVE_FACTIONS

    # Determine which building image to use
    img_key = None
    if city.get("is_camp") or (is_native and city.get("is_village")):
        img_key = "_nativecamp_overlay"
    elif city.get("is_fort") and not city.get("is_camp"):
        img_key = "_woodfort_overlay"
    elif is_native:
        img_key = "_nativecamp_overlay"
    elif sovereign in ("Great Britain", "United States", "Texas", "Confederate States", "Canada", "Rupert's Land"):
        tier = city.get("tier", 1)
        if tier >= 3: img_key = "_britcity3_overlay"
        elif tier >= 2: img_key = "_britcity2_overlay"
        else: img_key = "_britcity_overlay"
    elif sovereign in ("France",):
        tier = city.get("tier", 1)
        if tier >= 3: img_key = "_frencity3_overlay"
        elif tier >= 2: img_key = "_frencity2_overlay"
        else: img_key = "_frencity_overlay"
    elif sovereign in ("Spain", "Mexico"):
        tier = city.get("tier", 1)
        if tier >= 3: img_key = "_spancity3_overlay"
        elif tier >= 2: img_key = "_spancity2_overlay"
        else: img_key = "_spancity_overlay"
    elif sovereign == "Russia":
        img_key = "_ruscity_overlay"
    elif sovereign == "Denmark":
        img_key = "_dancity_overlay"
    else:
        img_key = "_britcity_overlay"

    # Draw the building image scaled to map zoom
    bldg_img = _biome_img_cache.get(img_key)
    if bldg_img:
        # Scale based on zoom and city type
        if city.get("is_camp"):
            w, h = max(4, int(7 * zoom)), max(3, int(5 * zoom))
        elif city.get("is_fort"):
            w, h = max(5, int(8 * zoom)), max(4, int(6 * zoom))
        elif city.get("is_village"):
            w, h = max(5, int(8 * zoom)), max(4, int(6 * zoom))
        elif city["is_capital"]:
            w, h = max(7, int(11 * zoom)), max(5, int(8 * zoom))
        else:
            w, h = max(6, int(9 * zoom)), max(5, int(7 * zoom))
        scaled = pygame.transform.scale(bldg_img, (w, h))
        screen.blit(scaled, (sx - w // 2, sy - h // 2))
    else:
        # Fallback: colored circle
        pygame.draw.circle(screen, city["color"], (sx, sy), max(2, int(5 * zoom)))

    # Draw faction flag on top of the building
    flag = faction_images.get(owner)
    if flag:
        fs = max(3, int(5 * zoom))
        flag_scaled = pygame.transform.scale(flag, (fs, fs))
        # Flag sits on top of the building (above center)
        flag_x = sx - fs // 2
        flag_y = sy - (h // 2 if bldg_img else int(6 * zoom)) - fs + 2
        screen.blit(flag_scaled, (flag_x, flag_y))

    # City name and info text
    tier_str = f" [T{city['tier']}]" if city["tier"] > 0 else ""
    text_y_offset = max(10, int(14 * zoom))
    draw_outlined_text(small_font, city["name"] + tier_str, (255, 255, 255), (sx, sy - text_y_offset), anchor="center")
    draw_outlined_text(small_font, str(city["troops"]), (255, 255, 255), (sx, sy + max(8, int(10 * zoom))), anchor="center")
    if city["occupier"]: draw_outlined_text(small_font, "(occupied)", FACTION_COLORS[city["occupier"]], (sx, sy - text_y_offset + 12), anchor="center")
    if is_flooded(city): draw_outlined_text(small_font, "(flooded)", (80, 150, 255), (sx, sy + max(34, int(34 * zoom))), anchor="center")

def draw_city_tooltip():
    """Draw tooltip when mouse hovers over a city/fort/village/camp."""
    mx, my = pygame.mouse.get_pos()
    hovered = None
    for city in cities:
        sx, sy = world_to_screen(city["x"], city["y"])
        if math.hypot(mx-sx, my-sy) < max(5, int(7*zoom)):
            hovered = city; break
    if not hovered: return
    c = hovered
    owner = c["owner"]
    biome = _get_biome(c["x"], c["y"])
    # Determine settlement type
    if c.get("is_camp"): stype = "Camp"
    elif c.get("is_fort"): stype = "Fort"
    elif c.get("is_village"): stype = "Village"
    elif c["is_capital"]: stype = "Capital"
    else: stype = "City"
    # Material production info
    mat = c["material"]
    biome_mods = _BIOME_MODIFIERS[biome]
    if mat == "Lumber":
        mat_amount = int(2 * biome_mods[2])
        mat_rate = f"{mat_amount} per 10s"
        if biome_mods[2] == 0: mat_rate = "None (tundra)"
    elif mat == "Hide":
        base = 2 if owner in NATIVE_FACTIONS else 1
        mat_amount = int(base * biome_mods[3])
        mat_rate = f"{mat_amount} per 5s"
    elif mat == "Iron":
        mat_rate = "1 per 15s"
    elif mat == "Coal":
        mat_rate = "1 per 15s"
    else:
        mat_rate = "N/A"
    # Food production
    food_base = 40 if owner in NATIVE_FACTIONS else 20
    food_amount = int(food_base * biome_mods[1])
    food_rate = f"{food_amount} per 1s"
    # Gold tax
    gold_mult = biome_mods[4]
    if c.get("is_village"):
        gold_tax = int(4 * gold_mult) if owner not in NATIVE_FACTIONS else 0
    elif c.get("is_fort") or c.get("is_camp"):
        gold_tax = int(4 * gold_mult)
    else:
        gold_tax = int(10 * gold_mult)
    # Merchants from this city
    city_merchants = [m for m in merchants if m.source is c or m.destination is c]
    # Health
    if c.get("is_camp"): max_hp = 30
    elif c.get("is_village"): max_hp = 50
    elif c["is_capital"]: max_hp = 200
    else: max_hp = 100
    max_hp += c.get("tier", 0) * 25
    # Build tooltip lines
    lines = []
    lines.append(f"{c['name']} ({stype})")
    lines.append(f"Owner: {owner}")
    lines.append(f"Health: {c['troops']}/{max_hp}")
    lines.append(f"Tier: {c.get('tier', 0)}")
    lines.append(f"Biome: {biome.capitalize()}")
    lines.append(f"Material: {mat} ({mat_rate})")
    lines.append(f"Food: +{food_rate}")
    lines.append(f"Gold Tax: +{gold_tax}/day")
    if c["occupier"]:
        lines.append(f"Occupied by: {c['occupier']}")
    if city_merchants:
        lines.append(f"Merchants: {len(city_merchants)}")
    # Buildings
    bldgs = c.get("buildings", [])
    if bldgs:
        lines.append(f"Buildings: {', '.join(bldgs)}")
    else:
        lines.append("Buildings: None")
    # Construction in progress
    if c.get("construction"):
        days_left = max(1, c["construction"]["ticks_left"] // DAY_TICKS)
        lines.append(f"Building: {c['construction']['name']} ({days_left}d left)")
    # Storage cap
    lines.append(f"Storage Cap: {_get_storage_cap(owner)}")
    # Luxury resource
    lux = _city_luxury_resource(c)
    if lux:
        lines.append(f"Luxury: {lux}")
    # Render tooltip panel
    W, H = get_screen_size()
    line_h = 16; pad = 6
    panel_w = max(small_font.size(l)[0] for l in lines) + pad*2
    panel_h = len(lines) * line_h + pad*2
    tx = mx + 15; ty = my + 10
    # Keep on screen
    if tx + panel_w > W: tx = mx - panel_w - 10
    if ty + panel_h > H: ty = my - panel_h - 10
    panel = pygame.Surface((panel_w, panel_h), pygame.SRCALPHA)
    panel.fill((20, 20, 20, 200))
    screen.blit(panel, (tx, ty))
    for i, line in enumerate(lines):
        col = (255, 220, 100) if i == 0 else (255, 255, 255)
        screen.blit(small_font.render(line, True, col), (tx+pad, ty+pad+i*line_h))

def draw_city_panel():
    """Draw the city detail panel when a city is clicked (image + stats + buildings)."""
    global _selected_city
    if not _selected_city: return
    c = _selected_city
    # Verify city still exists
    if c not in cities: _selected_city = None; return
    W, H = get_screen_size()
    owner = c["owner"]
    biome = _get_biome(c["x"], c["y"])
    is_native = owner in NATIVE_FACTIONS
    is_fort_or_camp = c.get("is_fort") or c.get("is_camp")
    # Panel dimensions — wider to fit buildings on right
    panel_w, panel_h = 700, 320
    px, py = (W - panel_w) // 2, H - panel_h - 10
    # Background
    panel = pygame.Surface((panel_w, panel_h), pygame.SRCALPHA)
    panel.fill((15, 15, 15, 220))
    screen.blit(panel, (px, py))
    _draw_panel_frame(px, py, panel_w, panel_h)
    # Image in middle
    img = _get_city_image(c)
    img_x, img_y = px + 240, py + 5
    screen.blit(img, (img_x, img_y))
    # Bouncing mini merchants inside the image
    if _panel_city_id != id(c):
        _init_panel_merchants(c)
    _update_panel_merchants()
    _draw_panel_merchants(img_x, img_y)
    _draw_panel_fires(img_x, img_y, c)
    # Stats on left side
    line_h = 15; lx = px + 8; ly = py + 8
    # Type
    if c.get("is_camp"): stype = "Camp"
    elif c.get("is_fort"): stype = "Fort"
    elif c.get("is_village"): stype = "Village"
    elif c["is_capital"]: stype = "Capital"
    else: stype = "City"
    # Health
    if c.get("is_camp"): max_hp = 30
    elif c.get("is_village"): max_hp = 50
    elif c["is_capital"]: max_hp = 200
    else: max_hp = 100
    max_hp += c.get("tier", 0) * 25
    # Material rate
    mat = c["material"]
    biome_mods = _BIOME_MODIFIERS[biome]
    if mat == "Lumber": mat_rate = f"{int(4*biome_mods[2])}/10s"
    elif mat == "Hide":
        base = 2 if is_native else 1
        mat_rate = f"{int(base*biome_mods[3])}/5s"
    elif mat == "Iron": mat_rate = "1/15s"
    else: mat_rate = "N/A"
    # Gold tax
    gold_mult = biome_mods[4]
    if _city_has_building(c, "Town Hall") or _city_has_building(c, "Council Lodge"): gold_mult *= 1.25
    if c.get("is_village"):
        gold_tax = int(4 * gold_mult) if not is_native else 0
    elif is_fort_or_camp:
        gold_tax = int(4 * gold_mult)
    else:
        gold_tax = int(10 * gold_mult)
    # Draw text lines on left
    lines = [
        (f"{c['name']} ({stype})", (255, 220, 100)),
        (f"Owner: {owner}", (255, 255, 255)),
        (f"Region: #{c.get('_region', -1)}", (180, 200, 255)),
        (f"Population: {_format_population(_get_city_population(c))}", (200, 230, 255)),
        (f"Health: {c['troops']}/{max_hp}", (255, 255, 255)),
        (f"Tier: {c.get('tier', 0)}", (255, 255, 255)),
        (f"Biome: {biome.capitalize()}", (255, 255, 255)),
        (f"Material: {mat} ({mat_rate})", (255, 255, 255)),
        (f"Gold Tax: +{gold_tax}/day", (255, 220, 50)),
    ]
    lux = _city_luxury_resource(c)
    if lux: lines.append((f"Luxury: {lux}", (255, 200, 255)))
    lines.append((f"Storage: {_get_storage_cap(owner)}", (200, 200, 200)))
    # Happiness
    happiness = c.get("_happiness", 100)
    if happiness >= 75: hap_col = (50, 200, 50)
    elif happiness >= 25: hap_col = (220, 200, 50)
    else: hap_col = (220, 50, 50)
    lines.append((f"Happiness: {happiness}", hap_col))
    _happiness_line_idx = len(lines) - 1  # track which line has happiness
    # Construction progress
    if c.get("construction"):
        days_left = max(1, c["construction"]["ticks_left"] // DAY_TICKS)
        pct = 100 - int(100 * c["construction"]["ticks_left"] / max(1, BUILDINGS[c["construction"]["name"]]["build_days"] * DAY_TICKS))
        lines.append((f"Building: {c['construction']['name']} ({pct}% - {days_left}d left)", (255, 180, 80)))
    lines.append(("[Click elsewhere to close]", (150, 150, 150)))
    for text, col in lines:
        screen.blit(small_font.render(text, True, col), (lx, ly))
        ly += line_h
    # Draw happiness face image next to happiness text
    if not hasattr(draw_city_panel, '_hap_imgs'):
        draw_city_panel._hap_imgs = {
            "happy": pygame.image.load("images/happyface.png").convert_alpha(),
            "med": pygame.image.load("images/medface.png").convert_alpha(),
            "sad": pygame.image.load("images/sadface.png").convert_alpha(),
        }
    happiness = c.get("_happiness", 100)
    if happiness >= 75: h_img = draw_city_panel._hap_imgs["happy"]
    elif happiness >= 25: h_img = draw_city_panel._hap_imgs["med"]
    else: h_img = draw_city_panel._hap_imgs["sad"]
    hap_y = py + 8 + _happiness_line_idx * line_h
    hap_x = lx + small_font.size(f"Happiness: {happiness}")[0] + 5
    screen.blit(pygame.transform.scale(h_img, (14, 14)), (hap_x, hap_y))
    # === Buildings section on right side (below image) ===
    rx = px + 500; ry = py + 8
    screen.blit(small_font.render("-- Buildings --", True, (255, 220, 100)), (rx, ry)); ry += line_h
    # Built buildings
    bldgs = c.get("buildings", [])
    if bldgs:
        for b in bldgs:
            screen.blit(small_font.render(f"[Built] {b}", True, (100, 255, 100)), (rx, ry)); ry += line_h
    else:
        screen.blit(small_font.render("(none built)", True, (150, 150, 150)), (rx, ry)); ry += line_h
    ry += 4
    # Available buildings to construct
    if not is_fort_or_camp:
        screen.blit(small_font.render("-- Available --", True, (200, 200, 255)), (rx, ry)); ry += line_h
        has_hall = _city_has_building(c, "Council Lodge") if is_native else _city_has_building(c, "Town Hall")
        tier = c.get("tier", 0)
        for bname, bdata in BUILDINGS.items():
            if _city_has_building(c, bname): continue  # already built
            if bdata.get("colonial_only") and is_native: continue
            if bdata.get("native_only") and not is_native: continue
            if bname not in ("Town Hall", "Council Lodge", "Road") and not has_hall: continue
            if bname == "Plantation" and not _can_build_plantation(c): continue
            if bname == "Fur Trading Post" and not _can_build_fur_post(c): continue
            if bname == "Road" and c["name"] in _ISLAND_CITY_NAMES: continue
            if bname == "Lumber Mill" and c["material"] != "Lumber": continue
            # Determine display cost
            if is_native and bname == "Market": cost = _NATIVE_MARKET_COST
            elif is_native and bname == "Warehouse": cost = _NATIVE_WAREHOUSE_COST
            elif is_native and bname == "Lumber Mill": cost = _NATIVE_LUMBER_MILL_COST
            elif is_native and bname == "Fur Trading Post": cost = _NATIVE_FUR_POST_COST
            else: cost = bdata["cost"]
            cost_str = " ".join(f"{v}{k[0]}" for k, v in cost.items())
            days = bdata["build_days"]
            tier_req = bdata.get("tier_req", 0)
            # Check if tier locked
            if tier < tier_req:
                screen.blit(small_font.render(f"{bname} [Tier {tier_req}]: {cost_str} ({days}d)", True, (100, 100, 100)), (rx, ry)); ry += line_h
            else:
                # Show with description
                desc = _BUILDING_DESCS.get(bname, "")
                screen.blit(small_font.render(f"{bname}: {cost_str} ({days}d)", True, (180, 180, 255)), (rx, ry)); ry += line_h
                if desc:
                    screen.blit(small_font.render(f"  {desc}", True, (150, 200, 150)), (rx, ry)); ry += line_h
    # Draw border
    pygame.draw.rect(screen, (100, 100, 100), (px, py, panel_w, panel_h), 1)

# === Battle Viewer System ===
_selected_battle = None  # tuple (unit_a, unit_b) or None

def _get_selected_battle():
    """Get a valid selected battle, clearing if units are gone."""
    global _selected_battle
    if _selected_battle:
        a, b = _selected_battle
        if a not in units or b not in units or a.fighting is not b:
            _selected_battle = None
    return _selected_battle

def _draw_battle_viewer():
    """Draw the battle viewer panel when a fighting unit is clicked."""
    battle = _get_selected_battle()
    if not battle: return
    a, b = battle
    W, H = get_screen_size()
    # Panel dimensions — center bottom
    bv_w = 500; bv_h = 160
    bv_x = (W - bv_w) // 2
    bv_y = H - bv_h - 10
    # Biome background
    biome = _get_biome(a.x, a.y)
    biome_imgs = _BIOME_IMAGES.get(biome, [])
    if biome_imgs:
        bg_path = biome_imgs[0]
        if bg_path not in _biome_img_cache:
            try: _biome_img_cache[bg_path] = pygame.transform.scale(pygame.image.load(bg_path).convert(), (bv_w, bv_h))
            except: _biome_img_cache[bg_path] = None
        bg = _biome_img_cache.get(bg_path)
        if bg:
            screen.blit(bg, (bv_x, bv_y))
    # Dark overlay for readability
    overlay = pygame.Surface((bv_w, bv_h), pygame.SRCALPHA); overlay.fill((0, 0, 0, 120))
    screen.blit(overlay, (bv_x, bv_y))
    _draw_panel_frame(bv_x, bv_y, bv_w, bv_h)
    # Unit A (left side)
    a_col = FACTION_COLORS.get(a.owner["owner"], (200, 200, 200))
    b_col = FACTION_COLORS.get(b.owner["owner"], (200, 200, 200))
    _bv_font = pygame.font.SysFont(None, 16, bold=True)
    _bv_small = pygame.font.SysFont(None, 14)
    # Left unit info
    a_name = getattr(a, 'regiment_name', 'Unit')
    screen.blit(_bv_font.render(a.owner["owner"], True, a_col), (bv_x + 15, bv_y + 10))
    screen.blit(_bv_small.render(a_name, True, (200, 200, 200)), (bv_x + 15, bv_y + 26))
    screen.blit(_bv_small.render(f"Men: {a.men}/{a.max_men}  PWR: {a.power}  T{a.tier}", True, (180, 180, 180)), (bv_x + 15, bv_y + 42))
    # Right unit info
    b_name = getattr(b, 'regiment_name', 'Unit')
    b_name_surf = _bv_font.render(b.owner["owner"], True, b_col)
    screen.blit(b_name_surf, (bv_x + bv_w - b_name_surf.get_width() - 15, bv_y + 10))
    b_reg_surf = _bv_small.render(b_name, True, (200, 200, 200))
    screen.blit(b_reg_surf, (bv_x + bv_w - b_reg_surf.get_width() - 15, bv_y + 26))
    b_stats_surf = _bv_small.render(f"Men: {b.men}/{b.max_men}  PWR: {b.power}  T{b.tier}", True, (180, 180, 180))
    screen.blit(b_stats_surf, (bv_x + bv_w - b_stats_surf.get_width() - 15, bv_y + 42))
    # "VS" in the center
    vs_font = pygame.font.SysFont(None, 28, bold=True)
    vs_surf = vs_font.render("VS", True, (255, 220, 50))
    screen.blit(vs_surf, (bv_x + bv_w // 2 - vs_surf.get_width() // 2, bv_y + 15))
    # Battle phase display
    phase = getattr(a, '_battle_phase', 'battle') or 'battle'
    phase_colors = {"encampment": (180, 180, 255), "approach": (255, 200, 100), "engagement": (255, 150, 50), "battle": (255, 80, 80), "retreat": (150, 150, 150)}
    phase_col = phase_colors.get(phase, (200, 200, 200))
    phase_font = pygame.font.SysFont(None, 18, bold=True)
    phase_surf = phase_font.render(f"- {phase.upper()} -", True, phase_col)
    screen.blit(phase_surf, (bv_x + bv_w // 2 - phase_surf.get_width() // 2, bv_y + 40))
    # Unit sprites (left facing right, right facing left)
    # Left unit sprite
    _draw_battle_unit_sprite(a, bv_x + 80, bv_y + 60, facing_right=True)
    # Right unit sprite
    _draw_battle_unit_sprite(b, bv_x + bv_w - 130, bv_y + 60, facing_right=False)
    # Battle progress bar
    bar_x = bv_x + 40; bar_y = bv_y + bv_h - 35; bar_w = bv_w - 80; bar_h = 18
    # Calculate advantage: based on current men ratios
    a_pct = a.men / max(1, a.max_men)
    b_pct = b.men / max(1, b.max_men)
    total = a_pct + b_pct
    a_ratio = a_pct / total if total > 0 else 0.5
    # Draw bar background
    pygame.draw.rect(screen, (30, 30, 30), (bar_x, bar_y, bar_w, bar_h), border_radius=4)
    # Left side (unit A)
    a_bar_w = int(bar_w * a_ratio)
    if a_bar_w > 0:
        pygame.draw.rect(screen, a_col, (bar_x, bar_y, a_bar_w, bar_h), border_radius=4)
    # Right side (unit B)
    b_bar_w = bar_w - a_bar_w
    if b_bar_w > 0:
        pygame.draw.rect(screen, b_col, (bar_x + a_bar_w, bar_y, b_bar_w, bar_h), border_radius=4)
    # Percentage labels
    a_pct_text = f"{int(a_ratio * 100)}%"
    b_pct_text = f"{int((1 - a_ratio) * 100)}%"
    screen.blit(_bv_small.render(a_pct_text, True, (255, 255, 255)), (bar_x + 5, bar_y + 3))
    b_lbl = _bv_small.render(b_pct_text, True, (255, 255, 255))
    screen.blit(b_lbl, (bar_x + bar_w - b_lbl.get_width() - 5, bar_y + 3))
    # Biome defense info
    my_def = _get_biome_defense_mult(a.x, a.y)
    enemy_def = _get_biome_defense_mult(b.x, b.y)
    def_text = f"Terrain: {biome.capitalize()} (Def x{my_def:.1f})"
    screen.blit(_bv_small.render(def_text, True, (150, 200, 150)), (bv_x + bv_w // 2 - 50, bv_y + 55))

def _draw_battle_unit_sprite(unit, x, y, facing_right=True):
    """Draw a unit's sprite in the battle viewer."""
    owner_faction = unit.owner["owner"]
    is_cav = unit.formation == "Square"
    # Get the right animation set
    if is_cav:
        anims = sprites.cavalry_anims.get(owner_faction, {})
    else:
        anims = sprites.infantry_anims.get(owner_faction, {})
    # Get walk right frames
    frames = anims.get("walk_r", anims.get("idle", []))
    if not frames:
        # Fallback: draw colored rectangle
        pygame.draw.rect(screen, FACTION_COLORS.get(owner_faction, (200, 200, 200)), (x, y, 40, 50))
        return
    # Animate
    frame_idx = (_brit_anim_tick // 8) % len(frames)
    frame = frames[frame_idx]
    scaled = pygame.transform.scale(frame, (50, 60))
    if not facing_right:
        scaled = pygame.transform.flip(scaled, True, False)
    screen.blit(scaled, (x, y))

def _check_battle_click(mx, my):
    """Check if player clicked on a fighting/sieging unit to select it for the battle viewer."""
    global _selected_battle, _selected_siege
    # Check field battles first
    for u in units:
        if u.fighting and u.fighting in units:
            sx, sy = world_to_screen(u.x, u.y)
            if math.hypot(mx - sx, my - sy) < max(6, int(9 * zoom)):
                _selected_battle = (u, u.fighting); _selected_siege = None
                return True
    # Check sieges — unit is at a city and has a siege phase
    for u in units:
        if hasattr(u, '_siege_phase') and u._siege_phase and not u.fighting and not u.retreating:
            sx, sy = world_to_screen(u.x, u.y)
            if math.hypot(mx - sx, my - sy) < max(6, int(9 * zoom)):
                _selected_siege = u; _selected_battle = None
                return True
    # Click elsewhere clears selection
    _selected_battle = None; _selected_siege = None
    return False

_selected_siege = None

def _draw_siege_viewer():
    """Draw the siege viewer panel when a sieging unit is clicked."""
    if not _selected_siege: return
    u = _selected_siege
    if not hasattr(u, '_siege_phase') or not u._siege_phase: return
    if u not in units or u.retreating: return
    target = u.target
    if target not in cities: return
    W, H = get_screen_size()
    # Panel dimensions — center bottom
    sv_w = 500; sv_h = 160
    sv_x = (W - sv_w) // 2
    sv_y = H - sv_h - 10
    # Biome background
    biome = _get_biome(target["x"], target["y"])
    biome_imgs = _BIOME_IMAGES.get(biome, [])
    if biome_imgs:
        bg_path = biome_imgs[0]
        if bg_path not in _biome_img_cache:
            try: _biome_img_cache[bg_path] = pygame.transform.scale(pygame.image.load(bg_path).convert(), (sv_w, sv_h))
            except: _biome_img_cache[bg_path] = None
        bg = _biome_img_cache.get(bg_path)
        if bg: screen.blit(bg, (sv_x, sv_y))
    # Dark overlay
    overlay = pygame.Surface((sv_w, sv_h), pygame.SRCALPHA); overlay.fill((0, 0, 0, 130))
    screen.blit(overlay, (sv_x, sv_y))
    _draw_panel_frame(sv_x, sv_y, sv_w, sv_h)
    _sv_font = pygame.font.SysFont(None, 16, bold=True)
    _sv_small = pygame.font.SysFont(None, 14)
    # Attacker info (left)
    a_col = FACTION_COLORS.get(u.owner["owner"], (200, 200, 200))
    screen.blit(_sv_font.render(u.owner["owner"], True, a_col), (sv_x + 15, sv_y + 10))
    screen.blit(_sv_small.render(u.regiment_name, True, (200, 200, 200)), (sv_x + 15, sv_y + 26))
    screen.blit(_sv_small.render(f"Men: {u.men}/{u.max_men}  PWR: {u.power}", True, (180, 180, 180)), (sv_x + 15, sv_y + 42))
    # Attacker sprite (left)
    _draw_battle_unit_sprite(u, sv_x + 80, sv_y + 60, facing_right=True)
    # Defender info (right) — the city
    d_col = FACTION_COLORS.get(target["owner"], (200, 200, 200))
    city_name_surf = _sv_font.render(target["name"], True, d_col)
    screen.blit(city_name_surf, (sv_x + sv_w - city_name_surf.get_width() - 15, sv_y + 10))
    def_surf = _sv_small.render(f"Garrison: {target['troops']}", True, (180, 180, 180))
    screen.blit(def_surf, (sv_x + sv_w - def_surf.get_width() - 15, sv_y + 26))
    pop_surf = _sv_small.render(f"Pop: {_format_population(_get_city_population(target))}", True, (180, 180, 180))
    screen.blit(pop_surf, (sv_x + sv_w - pop_surf.get_width() - 15, sv_y + 42))
    # Settlement building image only (no biome background)
    sovereign = target.get("sovereign", target["owner"])
    _bldg_keys = {"Great Britain": "_britcity_overlay", "United States": "_britcity_overlay", "Texas": "_britcity_overlay",
                  "Confederate States": "_britcity_overlay", "France": "_frencity_overlay", "Spain": "_spancity_overlay",
                  "Mexico": "_spancity_overlay", "Russia": "_ruscity_overlay", "Denmark": "_dancity_overlay"}
    bldg_key = _bldg_keys.get(sovereign)
    if target.get("is_camp") or (target.get("sovereign", target["owner"]) in NATIVE_FACTIONS):
        bldg_key = "_nativecamp_overlay"
    elif target.get("is_fort"):
        bldg_key = "_woodfort_overlay"
    if bldg_key and bldg_key in _biome_img_cache:
        bldg_img = _biome_img_cache[bldg_key]
        small_bldg = pygame.transform.scale(bldg_img, (80, 60))
        screen.blit(small_bldg, (sv_x + sv_w - 120, sv_y + 60))
    # "SIEGE" header
    vs_font = pygame.font.SysFont(None, 24, bold=True)
    screen.blit(vs_font.render("SIEGE", True, (255, 150, 50)), (sv_x + sv_w // 2 - 25, sv_y + 12))
    # Phase display
    phase = u._siege_phase
    phase_colors = {"encirclement": (180, 180, 255), "bombardment": (255, 200, 100), "assault": (255, 100, 50), "breach": (255, 50, 50)}
    phase_col = phase_colors.get(phase, (200, 200, 200))
    phase_font = pygame.font.SysFont(None, 18, bold=True)
    phase_surf = phase_font.render(f"- {phase.upper()} -", True, phase_col)
    screen.blit(phase_surf, (sv_x + sv_w // 2 - phase_surf.get_width() // 2, sv_y + 38))
    # Progress bar: attacker men vs garrison
    bar_x = sv_x + 40; bar_y = sv_y + sv_h - 35; bar_w = sv_w - 80; bar_h = 18
    a_str = u.men; d_str = max(1, target["troops"])
    total = a_str + d_str
    a_ratio = a_str / total
    pygame.draw.rect(screen, (30, 30, 30), (bar_x, bar_y, bar_w, bar_h), border_radius=4)
    a_bar_w = int(bar_w * a_ratio)
    if a_bar_w > 0: pygame.draw.rect(screen, a_col, (bar_x, bar_y, a_bar_w, bar_h), border_radius=4)
    b_bar_w = bar_w - a_bar_w
    if b_bar_w > 0: pygame.draw.rect(screen, d_col, (bar_x + a_bar_w, bar_y, b_bar_w, bar_h), border_radius=4)
    screen.blit(_sv_small.render(f"{int(a_ratio*100)}%", True, (255,255,255)), (bar_x + 5, bar_y + 3))
    b_lbl = _sv_small.render(f"{int((1-a_ratio)*100)}%", True, (255,255,255))
    screen.blit(b_lbl, (bar_x + bar_w - b_lbl.get_width() - 5, bar_y + 3))

def draw_unit_tooltip():
    """Draw tooltip when mouse hovers over a unit or merchant."""
    mx, my = pygame.mouse.get_pos()
    # Check units first
    for u in units:
        sx, sy = world_to_screen(u.x, u.y)
        if math.hypot(mx-sx, my-sy) < max(6, int(10*zoom)):
            _draw_unit_tooltip_panel(mx, my, u)
            return
    # Check merchants
    for m in merchants:
        sx, sy = world_to_screen(m.x, m.y)
        if math.hypot(mx-sx, my-sy) < max(6, int(10*zoom)):
            _draw_merchant_tooltip_panel(mx, my, m)
            return

def _draw_unit_tooltip_panel(mx, my, u):
    """Render unit tooltip panel."""
    status = u._status_text()
    biome = _get_biome(u.x, u.y)
    formation_type = "Cavalry" if u.formation == "Square" else "Infantry"
    lines = []
    lines.append(f"{u.regiment_name}")
    lines.append(f"Owner: {u.owner['owner']}")
    lines.append(f"Type: {formation_type} ({u.formation})")
    lines.append(f"HP: {u.hp}/{u.max_hp}")
    lines.append(f"Power: {u.power}")
    lines.append(f"Speed: {u.speed:.2f}")
    lines.append(f"Tier: {u.tier}")
    lines.append(f"Status: {status}")
    lines.append(f"Biome: {biome.capitalize()}")
    if u.is_ship:
        mode = "Canoe" if u.owner["owner"] in NATIVE_FACTIONS else "Ship"
        lines.append(f"Mode: {mode}")
    if u.target:
        lines.append(f"Target: {u.target.get('name', '?')}")
    # Upkeep info
    if u.owner["owner"] in NATIVE_FACTIONS:
        lines.append(f"Upkeep: 2 Food/day")
    else:
        g, f = (6, 8) if u.formation == "Square" else (3, 5)
        lines.append(f"Upkeep: {g}G + {f}F/day")
    # Render
    W, H = get_screen_size()
    line_h = 16; pad = 6
    panel_w = max(small_font.size(l)[0] for l in lines) + pad*2
    panel_h = len(lines) * line_h + pad*2
    tx = mx + 15; ty = my + 10
    if tx + panel_w > W: tx = mx - panel_w - 10
    if ty + panel_h > H: ty = my - panel_h - 10
    panel = pygame.Surface((panel_w, panel_h), pygame.SRCALPHA)
    panel.fill((20, 20, 20, 200))
    screen.blit(panel, (tx, ty))
    for i, line in enumerate(lines):
        col = (255, 220, 100) if i == 0 else (255, 255, 255)
        screen.blit(small_font.render(line, True, col), (tx+pad, ty+pad+i*line_h))

def _draw_merchant_tooltip_panel(mx, my, m):
    """Render merchant tooltip panel."""
    biome = _get_biome(m.x, m.y)
    lines = []
    lines.append(f"Merchant ({m.material})")
    lines.append(f"Owner: {m.owner_faction}")
    lines.append(f"Status: {m.status}")
    if getattr(m, 'luxury', None):
        lines.append(f"Luxury: {m.luxury}")
    lines.append(f"Biome: {biome.capitalize()}")
    if m.is_ship:
        mode = "Canoe" if m.owner_faction in NATIVE_FACTIONS else "Ship"
        lines.append(f"Mode: {mode}")
    if m.source:
        lines.append(f"From: {m.source.get('name', '?')}")
    if m.destination:
        lines.append(f"To: {m.destination.get('name', '?')}")
    # Upkeep
    if m.owner_faction in NATIVE_FACTIONS:
        lines.append(f"Upkeep: 1 Food/day")
    else:
        lines.append(f"Upkeep: 2G + 1F/day")
        lines.append(f"Travel cost: 3G/day")
    # Render
    W, H = get_screen_size()
    line_h = 16; pad = 6
    panel_w = max(small_font.size(l)[0] for l in lines) + pad*2
    panel_h = len(lines) * line_h + pad*2
    tx = mx + 15; ty = my + 10
    if tx + panel_w > W: tx = mx - panel_w - 10
    if ty + panel_h > H: ty = my - panel_h - 10
    panel = pygame.Surface((panel_w, panel_h), pygame.SRCALPHA)
    panel.fill((20, 20, 20, 200))
    screen.blit(panel, (tx, ty))
    for i, line in enumerate(lines):
        col = (255, 220, 100) if i == 0 else (255, 255, 255)
        screen.blit(small_font.render(line, True, col), (tx+pad, ty+pad+i*line_h))

MONTH_NAMES = ["January","February","March","April","May","June","July","August","September","October","November","December"]
MONTH_DAYS = [31,28,31,30,31,30,31,31,30,31,30,31]
game_day, game_month, game_year = 1, 1, 1754; day_timer = 0

# ====================================================================
# === DATE/TIME & EVENTS =============================================
# ====================================================================
def advance_date():
    global game_day, game_month, game_year, _fren_anims, _fren_cav; game_day += 1
    if game_day > MONTH_DAYS[game_month-1]:
        game_day = 1; game_month += 1
        if game_month > 12: game_month = 1; game_year += 1
        # Monthly population growth and happiness update for all cities
        for _pc in cities:
            _grow_city_population(_pc)
            _update_city_happiness(_pc)
        # AI adjusts production policies monthly
        _ai_update_policies()
        # Manpower replenishment from populations
        _update_manpower_monthly()
    # 1800 sprite swap — colonial troops get new uniforms
    if game_year >= 1800 and not _sprites_1800_swapped:
        _swap_to_1800()
    # 1850 — American troops switch to Union sprites
    if game_year >= 1850 and not _sprites_1850_swapped:
        _swap_us_to_union()
    # American Revolution — April 19, 1775
    if game_day == 19 and game_month == 4 and game_year == 1775 and "United States" not in eliminated_factions:
        _trigger_american_revolution()
        show_newspaper("American Revolution!", "The thirteen colonies declare independence from Great Britain. The United States of America is born.")
    # Found cities at their historical dates
    for fc in _FOUNDED_CITIES[:]:
        if game_year >= fc["_founded_year"] and not any(c["name"] == fc["name"] for c in cities):
            if "_happiness" not in fc: fc["_happiness"] = 100
            fc["_region"] = _region_at(fc["x"], fc["y"])  # tag its territory when founded
            cities.append(fc)
            _FOUNDED_CITIES.remove(fc)
            news(f"{fc['name']} is founded!")
    # Washington becomes US capital — July 16, 1790
    if game_day == 16 and game_month == 7 and game_year == 1790:
        if "United States" not in eliminated_factions:
            for c in cities:
                if c["name"] == "Washington" and c["owner"] == "United States":
                    c["is_capital"] = True
                elif c["name"] == "Boston" and c["owner"] == "United States" and c["is_capital"]:
                    c["is_capital"] = False
            news("Washington is established as the new capital of the United States!")
    # York is renamed Toronto — March 6, 1834
    if game_day == 6 and game_month == 3 and game_year == 1834:
        for c in cities:
            if c["name"] == "York":
                c["name"] = "Toronto"
                news("York is renamed Toronto.")
                break
    # Spain flag change — 1785
    if game_day == 1 and game_month == 1 and game_year == 1785 and "Spain" not in eliminated_factions:
        try: faction_images["Spain"] = pygame.transform.scale(pygame.image.load("images/flags/kingdomofspain.webp").convert_alpha(), (30, 30))
        except: pass
        news("Spain adopts a new royal banner!")
    # French Republic — September 21, 1792
    if game_day == 21 and game_month == 9 and game_year == 1792 and "France" not in eliminated_factions:
        try: faction_images["France"] = pygame.transform.scale(pygame.image.load("images/flags/francerp.webp"), (30, 30))
        except: pass
        # Swap French unit sprites to revolutionary era
        try:
            _fren_anims = sprites._load_infantry_sheet("images/sprites/frenchinfrv.png")
            sprites.infantry_anims["France"] = _fren_anims
            _fren_cav = sprites._load_cavalry_sheet("images/sprites/frenchcavrv.png")
            sprites.cavalry_anims["France"] = _fren_cav
        except: pass
        news("France abolishes the monarchy — the French Republic is proclaimed!")
        show_newspaper("French Revolution!", "The French monarchy is abolished. The First French Republic is proclaimed on this day.")
    # Louisiana Purchase — April 30, 1803
    if game_day == 30 and game_month == 4 and game_year == 1803:
        if "France" not in eliminated_factions and "United States" not in eliminated_factions:
            for c in cities:
                if c["name"] in ("New Orleans", "Saint-Louis") and c["owner"] == "France":
                    c["owner"] = "United States"; c["sovereign"] = "United States"
                    c["color"] = FACTION_COLORS.get("United States", (100, 180, 255))
                    c["occupier"] = None; c["is_capital"] = False
            news("Louisiana Purchase! France sells New Orleans & Saint-Louis to the United States.")
            show_newspaper("Louisiana Purchase!", "France sells the Louisiana Territory to the United States for 15 million dollars. New Orleans and Saint-Louis are now American.")
    # Haitian Independence — January 1, 1804
    if game_day == 1 and game_month == 1 and game_year == 1804:
        if "Haiti" not in FACTIONS:
            _trigger_haitian_independence()
    # Florida becomes US territory — March 3, 1845
    if game_day == 3 and game_month == 3 and game_year == 1845:
        if "United States" not in eliminated_factions:
            for c in cities:
                if c["name"] == "San Agustin" and c["owner"] != "United States":
                    c["owner"] = "United States"; c["sovereign"] = "United States"
                    c["color"] = FACTION_COLORS.get("United States", (100, 180, 255))
                    c["occupier"] = None; c["is_capital"] = False
                    news("Florida becomes a United States territory!")
                    show_newspaper("Florida Statehood!", "On March 3, 1845, Florida is admitted as the 27th state of the United States.")
                    break
    # Mexican Independence — September 16, 1810
    if game_day == 16 and game_month == 9 and game_year == 1810:
        if "Mexico" not in FACTIONS:
            _trigger_mexican_independence()
    # Texas Revolution — March 2, 1836
    if game_day == 2 and game_month == 3 and game_year == 1836:
        if "Texas" not in FACTIONS:
            _trigger_texas_independence()
    # Confederate States — February 8, 1861
    if game_day == 8 and game_month == 2 and game_year == 1861:
        if "Confederate States" not in FACTIONS:
            _trigger_confederate_states()
    # Alaska Purchase — March 30, 1867
    if game_day == 30 and game_month == 3 and game_year == 1867:
        if "United States" not in eliminated_factions and "Russia" not in eliminated_factions:
            for c in cities:
                if c["name"] in ("Sitka", "Kodiak") and c["owner"] == "Russia":
                    c["owner"] = "United States"; c["sovereign"] = "United States"
                    c["color"] = FACTION_COLORS.get("United States", (100, 180, 255))
                    c["occupier"] = None
                    # Replace governor
                    for _gk in ("_governor", "_governor_type", "_governor_traits", "_governor_ethnicity", "_governor_religion", "_is_chief"):
                        c.pop(_gk, None)
            news("Alaska Purchase! The United States buys Alaska from Russia for $7.2 million.")
            show_newspaper("Alaska Purchase!", "On March 30, 1867, the United States purchases Alaska from the Russian Empire for $7.2 million. Sitka and Kodiak are now American.")
    # Canadian Confederation — July 1, 1867
    if game_day == 1 and game_month == 7 and game_year == 1867:
        if "Canada" not in FACTIONS and "Rupert's Land" in FACTIONS and "Rupert's Land" not in eliminated_factions:
            _trigger_canadian_confederation()

def _rename_faction(old, new):
    """Rename a faction across ALL live game state (cities, materials, wars, alliances,
    puppets, focus, manpower). Used to turn Rupert's Land into Canada in 1867."""
    # FACTIONS list
    if old in FACTIONS:
        FACTIONS[FACTIONS.index(old)] = new
    else:
        FACTIONS.append(new)
    # Simple name->value dicts
    for d in (FACTION_COLORS, faction_materials, _faction_manpower, _faction_conscription,
              _faction_focus, _faction_focus_timer):
        if old in d: d[new] = d.pop(old)
    # eliminated / surrendered sets
    for s in (eliminated_factions, _surrendered):
        if old in s: s.discard(old); s.add(new)
    # Puppets: rename as both key (puppet) and value (overlord)
    for p, o in list(_puppets.items()):
        if p == old: _puppets[new] = _puppets.pop(p); p = new
        if _puppets.get(p) == old: _puppets[p] = new
    # Frozenset-keyed relations: wars(set), treaties/alliances/war_pressure/refused_demands(dicts)
    def _swap_key(container, is_dict):
        for key in list(container):
            if old in key:
                newkey = frozenset((new if f == old else f) for f in key)
                if is_dict:
                    container[newkey] = container.pop(key)
                else:
                    container.discard(key); container.add(newkey)
    _swap_key(wars, False)
    for dct in (treaties, alliances, war_pressure, refused_demands):
        _swap_key(dct, True)
    # Cities
    for c in cities:
        if c.get("owner") == old: c["owner"] = new
        if c.get("sovereign") == old: c["sovereign"] = new
        if c.get("_original_sovereign") == old: c["_original_sovereign"] = new
        if c.get("occupier") == old: c["occupier"] = new
    # Units store owner as a city dict (renamed above). Merchants/settlers key by faction name.
    for m in merchants:
        if getattr(m, "owner_faction", None) == old: m.owner_faction = new
    for s in settlers:
        if getattr(s, "owner_faction", None) == old: s.owner_faction = new

_canada_fired = False
def _trigger_canadian_confederation():
    global _canada_fired
    if _canada_fired: return
    _canada_fired = True
    # Rupert's Land becomes the Dominion of Canada (keep its identity, cities, and puppet status).
    _rename_faction("Rupert's Land", "Canada")
    FACTION_COLORS["Canada"] = (200, 50, 50)
    # Give it a small boost of resources for its new statehood
    if "Canada" in faction_materials:
        faction_materials["Canada"]["Gold"] = faction_materials["Canada"].get("Gold", 0) + 60
        faction_materials["Canada"]["Lumber"] = faction_materials["Canada"].get("Lumber", 0) + 40
    # New flag + Canadian sprites
    try: faction_images["Canada"] = pygame.transform.scale(pygame.image.load("images/flags/canada.png").convert_alpha(), (30, 30))
    except: pass
    try:
        sprites.infantry_anims["Canada"] = sprites._load_infantry_sheet("images/sprites/caninf.png")
        sprites.cavalry_anims["Canada"] = sprites._load_cavalry_sheet("images/sprites/cancav.png")
    except: pass
    # Gain the rest of British North America
    _canada_cities = {"Quebec", "Montreal", "Halifax", "Ottawa", "Tadoussac", "Plaisance", "Victoria", "Toronto", "York"}
    for c in cities:
        if c["name"] in _canada_cities and c["owner"] == "Great Britain":
            c["owner"] = "Canada"; c["sovereign"] = "Canada"
            c["color"] = FACTION_COLORS["Canada"]; c["occupier"] = None
            for _gk in ("_governor", "_governor_type", "_governor_traits", "_governor_ethnicity", "_governor_religion", "_is_chief"):
                c.pop(_gk, None)
    # Recolor all Canadian cities
    for c in cities:
        if c["owner"] == "Canada": c["color"] = FACTION_COLORS["Canada"]
    # Ottawa becomes the capital (York Factory yields the capital status)
    _ott = next((c for c in cities if c["name"] == "Ottawa" and c["owner"] == "Canada"), None)
    if _ott:
        for c in cities:
            if c["owner"] == "Canada" and c.get("is_capital"): c["is_capital"] = False
        _ott["is_capital"] = True
    # Remains a puppet of Great Britain
    set_puppet("Canada", "Great Britain")
    _faction_focus["Canada"] = "Economic"
    _faction_focus_timer["Canada"] = random.randint(DAY_TICKS * 60, DAY_TICKS * 365)
    news("Canadian Confederation! Rupert's Land unites into the Dominion of Canada!")
    show_newspaper("Canadian Confederation!", "On July 1, 1867, Rupert's Land and the British North American colonies unite to form the Dominion of Canada, a self-governing dominion of the British Empire. Ottawa is the new capital.")

_mexico_fired = False
def _trigger_mexican_independence():
    global _mexico_fired
    if _mexico_fired: return
    _mexico_fired = True
    # Add Mexico to factions
    FACTION_COLORS["Mexico"] = (0, 100, 50)
    if "Mexico" not in FACTIONS: FACTIONS.append("Mexico")
    if "Mexico" not in faction_materials: faction_materials["Mexico"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["Mexico"][_mat] = _val
    faction_materials["Mexico"]["Gold"] = 40; faction_materials["Mexico"]["Food"] = 40
    # Load flag and sprites
    try: faction_images["Mexico"] = pygame.transform.scale(pygame.image.load("images/flags/mexico.png").convert_alpha(), (30, 30))
    except: pass
    # Load infantry and cavalry sprites
    try:
        sprites.infantry_anims["Mexico"] = sprites._load_infantry_sheet("images/sprites/mexinf.png")
        sprites.cavalry_anims["Mexico"] = sprites._load_cavalry_sheet("images/sprites/mexcav.png")
    except: pass
    # Transfer cities to Mexico from whoever holds them
    mexico_cities = ["Mexico City", "Chihuahua", "Oaxaca"]
    war_targets = set()
    for c in cities:
        if c["name"] in mexico_cities:
            old_owner = c["owner"]
            if old_owner != "Mexico":
                war_targets.add(old_owner)
            c["owner"] = "Mexico"; c["sovereign"] = "Mexico"
            c["color"] = (0, 100, 50); c["occupier"] = None
            if c["name"] == "Mexico City": c["is_capital"] = True
    # Spain moves capital to Havana if they lost Mexico City
    spain_has_capital = any(c["is_capital"] and c["owner"] == "Spain" for c in cities)
    if not spain_has_capital:
        for c in cities:
            if c["owner"] == "Spain": c["is_capital"] = False
        havana = next((c for c in cities if c["name"] == "Havana" and c["owner"] == "Spain"), None)
        if havana: havana["is_capital"] = True
    # Declare war on whoever held the cities
    for target in war_targets:
        if target not in eliminated_factions:
            declare_war("Mexico", target, f"Mexico declares independence from {target}!")
    # Init faction focus
    _faction_focus["Mexico"] = "Aggressive"
    _faction_focus_timer["Mexico"] = random.randint(DAY_TICKS * 60, DAY_TICKS * 365)
    news("Mexican War of Independence! Mexico rises against Spanish colonial rule!")
    show_newspaper("Mexican Independence!", "On September 16, 1810, Mexico declares independence from Spain. The Mexican War of Independence has begun!")

_texas_fired = False
def _trigger_texas_independence():
    global _texas_fired
    if _texas_fired: return
    _texas_fired = True
    # Add Texas to factions
    FACTION_COLORS["Texas"] = (0, 50, 150)
    if "Texas" not in FACTIONS: FACTIONS.append("Texas")
    if "Texas" not in faction_materials: faction_materials["Texas"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["Texas"][_mat] = _val
    faction_materials["Texas"]["Gold"] = 30; faction_materials["Texas"]["Food"] = 30
    # Load flag and sprites
    try: faction_images["Texas"] = pygame.transform.scale(pygame.image.load("images/flags/texas.webp").convert_alpha(), (30, 30))
    except: pass
    try:
        sprites.infantry_anims["Texas"] = sprites._load_infantry_sheet("images/sprites/texinf.png")
        sprites.cavalry_anims["Texas"] = sprites._load_cavalry_sheet("images/sprites/texcav.png")
    except: pass
    # Transfer San Antonio from whoever holds it
    for c in cities:
        if c["name"] == "San Antonio":
            old_owner = c["owner"]
            c["owner"] = "Texas"
            c["color"] = (0, 50, 150); c["occupier"] = None; c["is_capital"] = False
            # Declare war on whoever held it
            if old_owner != "Texas" and old_owner not in eliminated_factions:
                declare_war("Texas", old_owner, f"Texas declares independence from {old_owner}!")
            break
    # Austin is founded via _FOUNDED_CITIES system (year 1836) — claim it for Texas
    for c in cities:
        if c["name"] == "Austin":
            c["owner"] = "Texas"; c["sovereign"] = "Texas"; c["color"] = (0, 50, 150); c["is_capital"] = True
    # Init faction focus
    _faction_focus["Texas"] = "Aggressive"
    _faction_focus_timer["Texas"] = random.randint(DAY_TICKS * 60, DAY_TICKS * 365)
    news("Texas Revolution! The Republic of Texas declares independence!")
    show_newspaper("Texas Revolution!", "On March 2, 1836, Texas declares independence. The Lone Star Republic is born!")

_confederate_fired = False
def _trigger_confederate_states():
    global _confederate_fired
    if _confederate_fired: return
    _confederate_fired = True
    # Add Confederate States to factions
    FACTION_COLORS["Confederate States"] = (150, 0, 0)
    if "Confederate States" not in FACTIONS: FACTIONS.append("Confederate States")
    if "Confederate States" not in faction_materials: faction_materials["Confederate States"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["Confederate States"][_mat] = _val
    faction_materials["Confederate States"]["Gold"] = 40; faction_materials["Confederate States"]["Food"] = 40
    # Load flag and sprites
    try: faction_images["Confederate States"] = pygame.transform.scale(pygame.image.load("images/flags/confederatestates.png").convert_alpha(), (30, 30))
    except: pass
    try:
        sprites.infantry_anims["Confederate States"] = sprites._load_infantry_sheet("images/sprites/confedinf.png")
        sprites.cavalry_anims["Confederate States"] = sprites._load_cavalry_sheet("images/sprites/confedcav.png")
    except: pass
    # Transfer cities from whoever holds them
    confed_cities = ["Richmond", "Charleston", "San Antonio", "Austin", "San Agustin", "New Orleans", "Williamsburg"]
    war_targets = set()
    for c in cities:
        if c["name"] in confed_cities:
            old_owner = c["owner"]
            if old_owner != "Confederate States":
                war_targets.add(old_owner)
            c["owner"] = "Confederate States"
            c["color"] = (150, 0, 0); c["occupier"] = None
            if c["name"] == "Richmond": c["is_capital"] = True
            else: c["is_capital"] = False
    # Declare war on whoever held the cities
    for target in war_targets:
        if target not in eliminated_factions:
            declare_war("Confederate States", target, f"Confederate States secedes from {target}!")
    # Init faction focus
    _faction_focus["Confederate States"] = "Aggressive"
    _faction_focus_timer["Confederate States"] = random.randint(DAY_TICKS * 60, DAY_TICKS * 365)
    news("The Confederate States of America is formed! Civil War begins!")
    show_newspaper("Confederate Secession!", "On February 8, 1861, the Confederate States of America is formed. The American Civil War has begun!")

_haiti_fired = False
def _trigger_haitian_independence():
    global _haiti_fired
    if _haiti_fired: return
    _haiti_fired = True
    # Add Haiti to factions
    FACTION_COLORS["Haiti"] = (0, 0, 150)
    if "Haiti" not in FACTIONS: FACTIONS.append("Haiti")
    if "Haiti" not in faction_materials: faction_materials["Haiti"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["Haiti"][_mat] = _val
    faction_materials["Haiti"]["Gold"] = 30; faction_materials["Haiti"]["Food"] = 30
    # Load flag and sprites
    try: faction_images["Haiti"] = pygame.transform.scale(pygame.image.load("images/flags/haiti.png").convert_alpha(), (30, 30))
    except: pass
    try:
        sprites.infantry_anims["Haiti"] = sprites._load_infantry_sheet("images/sprites/haitinf.png")
        sprites.cavalry_anims["Haiti"] = sprites._load_cavalry_sheet("images/sprites/haiticav.png")
    except: pass
    # Transfer Port-au-Prince from whoever holds it to Haiti
    for c in cities:
        if c["name"] == "Port-au-Prince":
            old_owner = c["owner"]
            c["owner"] = "Haiti"; c["sovereign"] = "Haiti"
            c["color"] = (0, 0, 150); c["occupier"] = None; c["is_capital"] = True
            # Declare war on whoever held it
            if old_owner != "Haiti" and old_owner not in eliminated_factions:
                declare_war("Haiti", old_owner, "Haiti declares independence from " + old_owner + "!")
    # Init faction focus
    _faction_focus["Haiti"] = "Aggressive"
    _faction_focus_timer["Haiti"] = random.randint(DAY_TICKS * 60, DAY_TICKS * 365)
    news("Haitian Revolution! Haiti declares independence from France!")
    show_newspaper("Haitian Independence!", "On January 1, 1804, Haiti becomes the first free Black republic, declaring independence from France after a successful slave revolution.")

_revolution_fired = False
_sprites_1800_swapped = False
_sprites_1850_swapped = False

def _swap_us_to_union():
    """Swap US sprites to Union era (1850+)."""
    global _sprites_1850_swapped, _usa_anims, _usa_cav
    _sprites_1850_swapped = True
    try:
        sprites.infantry_anims["United States"] = sprites._load_infantry_sheet("images/sprites/unioninf.png")
        sprites.cavalry_anims["United States"] = sprites._load_cavalry_sheet("images/sprites/unioncav.png")
        _usa_anims = sprites.infantry_anims["United States"]
        _usa_cav = sprites.cavalry_anims["United States"]
    except: pass

def _swap_to_1800():
    """Swap colonial sprites to 1800 era and update local references."""
    global _sprites_1800_swapped, _brit_anims, _fren_anims, _rus_anims, _spa_anims, _usa_anims, _den_anims
    global _brit_cav, _fren_cav, _rus_cav, _spa_cav, _usa_cav, _den_cav
    _sprites_1800_swapped = True
    sprites.swap_to_1800_sprites()
    _brit_anims = sprites.infantry_anims.get("Great Britain", _brit_anims)
    _fren_anims = sprites.infantry_anims.get("France", _fren_anims)
    _rus_anims = sprites.infantry_anims.get("Russia", _rus_anims)
    _spa_anims = sprites.infantry_anims.get("Spain", _spa_anims)
    _usa_anims = sprites.infantry_anims.get("United States", _usa_anims)
    _den_anims = sprites.infantry_anims.get("Denmark", _den_anims)
    _brit_cav = sprites.cavalry_anims.get("Great Britain", _brit_cav)
    _fren_cav = sprites.cavalry_anims.get("France", _fren_cav)
    _rus_cav = sprites.cavalry_anims.get("Russia", _rus_cav)
    _spa_cav = sprites.cavalry_anims.get("Spain", _spa_cav)
    _usa_cav = sprites.cavalry_anims.get("United States", _usa_cav)
    _den_cav = sprites.cavalry_anims.get("Denmark", _den_cav)
def _trigger_american_revolution():
    global _revolution_fired
    if _revolution_fired: return
    _revolution_fired = True
    # Add United States to factions
    FACTION_COLORS["United States"] = (100, 180, 255)
    if "United States" not in FACTIONS: FACTIONS.append("United States")
    faction_materials["United States"] = {m: 0 for m in materials}
    faction_materials["United States"]["Gold"] = 30; faction_materials["United States"]["Food"] = 30
    faction_materials["United States"]["Hide"] = 15; faction_materials["United States"]["Lumber"] = 15
    # Load continental flag (will switch to usa.webp after treaty)
    faction_images["United States"] = pygame.transform.scale(pygame.image.load("images/continent.webp"), (30, 30))
    # Determine who owns Boston and New York — France is always friendly
    enemies_to_fight = set()
    for city in cities:
        if city["name"] in ("Boston", "New York"):
            owner = city["owner"]
            if owner == "United States": continue
            if owner == "France":
                # France peacefully transfers to US — no war
                city["owner"] = "United States"; city["sovereign"] = "United States"
                city["occupier"] = None; city["color"] = (100, 180, 255)
                news(f"France transfers {city['name']} to the United States.")
            else:
                # Non-France owner — US takes it and declares war
                enemies_to_fight.add(owner)
                city["owner"] = "United States"; city["sovereign"] = "United States"
                city["occupier"] = None; city["color"] = (100, 180, 255)
    # Declare war on all non-France occupiers
    for enemy in enemies_to_fight:
        declare_war("United States", enemy, f"United States declares independence from {enemy}!")
    # France always allies with the United States
    if "France" not in eliminated_factions:
        key = pair("United States", "France")
        alliances[key] = random.randint(DAY_TICKS*200, DAY_TICKS*400)
        news("France allies with the United States!")
    news("The American Revolution has begun! United States declares independence!")
    # Start with a Tier III Line unit and a Tier III Cavalry unit at Boston
    boston = next((c for c in cities if c["name"]=="Boston" and c["owner"]=="United States"), None)
    if boston:
        u1 = Unit(boston, boston, "Line"); u1.tier = 3; u1.power += 6; u1.max_hp += 30; u1.hp = u1.max_hp; u1.idle = True; units.append(u1)
        u2 = Unit(boston, boston, "Square"); u2.tier = 3; u2.power += 6; u2.max_hp += 30; u2.hp = u2.max_hp; u2.idle = True; units.append(u2)
def get_season():
    if game_month in (12,1,2): return "Winter"
    if game_month in (3,4,5): return "Spring"
    if game_month in (6,7,8): return "Summer"
    return "Autumn"
def is_winter(): return get_season() == "Winter"
def draw_date():
    W, _ = get_screen_size(); date_str = f"{game_day} {MONTH_NAMES[game_month-1]} {game_year}"
    season = get_season()
    sc = (180,220,255) if season=="Winter" else (150,255,150) if season=="Spring" else (255,220,80) if season=="Summer" else (255,160,50)
    # Frame dimensions for top-center
    date_w = font.render(date_str, True, (255,255,255)).get_width()
    season_w = small_font.render(season, True, sc).get_width()
    frame_w = max(date_w, season_w) + 220  # extra space for speed buttons
    frame_h = 42
    frame_x = (W - frame_w) // 2
    frame_y = 4
    # Draw frame background
    frame_surf = pygame.Surface((frame_w, frame_h), pygame.SRCALPHA); frame_surf.fill((10, 10, 10, 180))
    screen.blit(frame_surf, (frame_x, frame_y))
    pygame.draw.rect(screen, (120, 100, 50), (frame_x, frame_y, frame_w, frame_h), 2, border_radius=4)
    pygame.draw.rect(screen, (180, 150, 60), (frame_x+1, frame_y+1, frame_w-2, frame_h-2), 1, border_radius=4)
    # Date text centered
    date_x = frame_x + (frame_w - date_w) // 2 - 80
    draw_outlined_text(font, date_str, (255,255,255), (date_x, frame_y + 6))
    # Season text below date
    draw_outlined_text(small_font, season, sc, (date_x + (date_w - season_w) // 2, frame_y + 24))
    # Speed buttons on the right side of frame
    global _speed_btn_rects; _speed_btn_rects = []
    btn_y = frame_y + 11
    btn_area_x = frame_x + frame_w - 180
    # Pause button
    pause_rect = pygame.Rect(btn_area_x, btn_y, 28, 20)
    pause_col = (255,100,100) if game_speed == 0 else (180,180,180)
    pygame.draw.rect(screen, (40,40,40), pause_rect, border_radius=3)
    pygame.draw.rect(screen, pause_col, (btn_area_x+8, btn_y+4, 4, 12))
    pygame.draw.rect(screen, pause_col, (btn_area_x+16, btn_y+4, 4, 12))
    _speed_btn_rects.append((pause_rect, 0))
    for i in range(1, 5):
        bx = btn_area_x + 32 + (i-1)*36; rect = pygame.Rect(bx, btn_y, 34, 20); spd_val = i if i <= 3 else 6
        col = (255,220,50) if spd_val==game_speed else ((200,200,0) if spd_val==6 else (180,180,180))
        pygame.draw.rect(screen, (40,40,40), rect, border_radius=3)
        for t in range(i if i <= 3 else 4): pygame.draw.polygon(screen, col, [(bx+4+t*7, btn_y+4), (bx+4+t*7, btn_y+16), (bx+10+t*7, btn_y+10)])
        _speed_btn_rects.append((rect, spd_val))
    # --- Coordinate readout box, directly below the date/speed frame ---
    mx, my = pygame.mouse.get_pos()
    # Cursor -> native world coords, then -> raw coords (native / _COORD_SCALE) used by city defs
    world_x = (mx - cam_x) / zoom
    world_y = (my - cam_y) / zoom
    raw_x = world_x / _COORD_SCALE
    raw_y = world_y / _COORD_SCALE
    on_map = 0 <= world_x < _MAP_NATIVE_W and 0 <= world_y < _MAP_NATIVE_H
    coord_str = f"X: {raw_x:.0f}   Y: {raw_y:.0f}" if on_map else "X: --   Y: --"
    cf_w = max(140, small_font.render(coord_str, True, (255,255,255)).get_width() + 24)
    cf_h = 22
    cf_x = (W - cf_w) // 2
    cf_y = frame_y + frame_h + 4  # just below the date frame
    cf_surf = pygame.Surface((cf_w, cf_h), pygame.SRCALPHA); cf_surf.fill((10, 10, 10, 180))
    screen.blit(cf_surf, (cf_x, cf_y))
    pygame.draw.rect(screen, (120, 100, 50), (cf_x, cf_y, cf_w, cf_h), 2, border_radius=4)
    cs_w = small_font.render(coord_str, True, (200, 230, 255)).get_width()
    draw_outlined_text(small_font, coord_str, (200, 230, 255), (cf_x + (cf_w - cs_w) // 2, cf_y + 3))
spawn_timer = material_timer = merchant_timer = settler_timer = 0
upkeep_timer = treaty_timer = war_timer = disaster_timer = 0
game_speed = 0; _speed_btn_rects = []
game_state.game_speed = game_speed
panning = False; pan_start = (0,0); cam_start = (0,0)
_news_btn = pygame.Rect(0,0,1,1); _factions_btn = pygame.Rect(0,0,1,1)

# === MAIN MENU ===
_btn_frame_img = pygame.image.load("images/frame.webp").convert_alpha()

def _draw_btn_frame(rect):
    """Draw frame.webp stretched around a button rect."""
    frame_scaled = pygame.transform.scale(_btn_frame_img, (rect.width, rect.height))
    screen.blit(frame_scaled, rect.topleft)

def show_menu():
    """Show main menu with Play button and map selection."""
    menu_bg = pygame.transform.scale(pygame.image.load("images/nasasatelliteview.jpg"), screen.get_size())
    title_font = pygame.font.SysFont(None, 72)
    btn_font = pygame.font.SysFont(None, 42)
    state = "main"  # "main" or "maps"
    _menu_scroll = 0  # horizontal scroll offset for scenario chooser
    # Marching units across bottom of screen — groups of country units (infantry front, cavalry back)
    # Each country shows old sprites then 1800 sprites, then next country
    _march_inf_sheets = [
        ("images/sprites/britinf.png", "images/sprites/britinf1800.png", "images/sprites/britishcav.png", "images/sprites/britcav1800.png", "images/flags/britian.png", "images/flags/britian.png"),
        ("images/sprites/frenchinf.png", "images/sprites/frenchinf1800.png", "images/sprites/frenchcav.png", "images/sprites/frenchcav1800.png", "images/flags/france.png", "images/flags/francerp.webp"),
        ("images/sprites/spaininf.png", "images/sprites/spaininf1800.png", "images/sprites/spanishcav.png", "images/sprites/spaincav1800.png", "images/flags/spain.png", "images/flags/spain.png"),
        ("images/sprites/rusinf.png", "images/sprites/rusinf1800.png", "images/sprites/russiancav.png", "images/sprites/ruscav1800.png", "images/flags/russia.webp", "images/flags/russia.webp"),
        ("images/sprites/daneinf.png", "images/sprites/daneinf1800.png", "images/sprites/danecav.png", "images/sprites/danecav1800.png", "images/flags/denmark.webp", "images/flags/denmark.webp"),
        ("images/sprites/americaninf.png", "images/sprites/americaninf1800.png", "images/sprites/americancav.png", "images/sprites/americancav1800.png", "images/flags/usa.webp", "images/flags/usa.webp"),
        ("images/sprites/mexinf.png", "images/sprites/mexinf.png", "images/sprites/mexcav.png", "images/sprites/mexcav.png", "images/flags/mexico.png", "images/flags/mexico.png"),
        ("images/sprites/haitinf.png", "images/sprites/haitinf.png", "images/sprites/haiticav.png", "images/sprites/haiticav.png", "images/flags/haiti.png", "images/flags/haiti.png"),
        ("images/sprites/texinf.png", "images/sprites/texinf.png", "images/sprites/texcav.png", "images/sprites/texcav.png", "images/flags/texas.webp", "images/flags/texas.webp"),
        ("images/sprites/confedinf.png", "images/sprites/confedinf.png", "images/sprites/confedcav.png", "images/sprites/confedcav.png", "images/flags/confederatestates.png", "images/flags/confederatestates.png"),
    ]
    _march_units = []
    x_offset = -60
    random.shuffle(_march_inf_sheets)
    for inf_path, inf1800_path, cav_path, cav1800_path, flag_path, flag1800_path in _march_inf_sheets:
        # Load flags for this country
        flag_img = pygame.transform.flip(pygame.transform.scale(pygame.image.load(flag_path).convert_alpha(), (70, 70)), True, False)
        flag1800_img = pygame.transform.flip(pygame.transform.scale(pygame.image.load(flag1800_path).convert_alpha(), (70, 70)), True, False)
        # Load old era sprites
        inf_sheet = pygame.image.load(inf_path).convert_alpha()
        inf_frames = [inf_sheet.subsurface(pygame.Rect(j*20, 40, 20, 40)) for j in range(2) if j*20+20 <= inf_sheet.get_width() and 80 <= inf_sheet.get_height()]
        if not inf_frames: inf_frames = [inf_sheet.subsurface(pygame.Rect(0, 0, 20, 40))]
        cav_sheet = pygame.image.load(cav_path).convert_alpha()
        cav_frames = [cav_sheet.subsurface(pygame.Rect(j*40, 40, 40, 40)) for j in range(2) if j*40+40 <= cav_sheet.get_width() and 80 <= cav_sheet.get_height()]
        if not cav_frames: cav_frames = [cav_sheet.subsurface(pygame.Rect(0, 0, min(40, cav_sheet.get_width()), min(40, cav_sheet.get_height())))]
        # Load 1800 era sprites
        inf1800_sheet = pygame.image.load(inf1800_path).convert_alpha()
        inf1800_frames = [inf1800_sheet.subsurface(pygame.Rect(j*20, 40, 20, 40)) for j in range(2) if j*20+20 <= inf1800_sheet.get_width() and 80 <= inf1800_sheet.get_height()]
        if not inf1800_frames: inf1800_frames = [inf1800_sheet.subsurface(pygame.Rect(0, 0, 20, 40))]
        cav1800_sheet = pygame.image.load(cav1800_path).convert_alpha()
        cav1800_frames = [cav1800_sheet.subsurface(pygame.Rect(j*40, 40, 40, 40)) for j in range(2) if j*40+40 <= cav1800_sheet.get_width() and 80 <= cav1800_sheet.get_height()]
        if not cav1800_frames: cav1800_frames = [cav1800_sheet.subsurface(pygame.Rect(0, 0, min(40, cav1800_sheet.get_width()), min(40, cav1800_sheet.get_height())))]
        # Old era group: 3 infantry + 2 cavalry
        for i in range(3):
            _march_units.append({"x": x_offset, "frames": inf_frames, "tick": random.randint(0, 100), "is_cav": False, "flag": flag_img if i == 0 else None})
            x_offset -= 150
        x_offset -= 200
        for i in range(2):
            _march_units.append({"x": x_offset, "frames": cav_frames, "tick": random.randint(0, 100), "is_cav": True, "flag": None})
            x_offset -= 290
        # Small gap between old and 1800 of same country
        x_offset -= 200
        # 1800 era group: 3 infantry + 2 cavalry
        for i in range(3):
            _march_units.append({"x": x_offset, "frames": inf1800_frames, "tick": random.randint(0, 100), "is_cav": False, "flag": flag1800_img if i == 0 else None})
            x_offset -= 150
        x_offset -= 200
        for i in range(2):
            _march_units.append({"x": x_offset, "frames": cav1800_frames, "tick": random.randint(0, 100), "is_cav": True, "flag": None})
            x_offset -= 290
        # Large gap between countries
        x_offset -= 400
    _march_tick = 0
    _menu_explosions = []  # list of {"x", "y", "tick"}
    # Load explosion frames for menu
    _menu_exp_frames = []
    try:
        from PIL import Image as PILImage
        pil_img = PILImage.open("images/explosion.gif")
        try:
            while True:
                frame = pil_img.convert("RGBA").resize((240, 240), PILImage.LANCZOS)
                _menu_exp_frames.append(pygame.image.fromstring(frame.tobytes(), (240, 240), "RGBA"))
                pil_img.seek(pil_img.tell() + 1)
        except EOFError: pass
    except ImportError:
        _menu_exp_frames = [pygame.transform.scale(pygame.image.load("images/explosion.gif"), (240, 240))]
    if not _menu_exp_frames: _menu_exp_frames = [pygame.Surface((240, 240), pygame.SRCALPHA)]
    while True:
        for event in pygame.event.get():
            if event.type == pygame.QUIT: pygame.quit(); sys.exit()
            if event.type == pygame.KEYDOWN:
                if event.key == pygame.K_F11: toggle_fullscreen()
                if event.key == pygame.K_ESCAPE:
                    if state == "maps" or state == "codex": state = "main"
            if event.type == pygame.MOUSEWHEEL and state == "maps":
                _menu_scroll -= event.y * 80
            if event.type == pygame.MOUSEBUTTONDOWN and event.button == 1:
                mx, my = event.pos
                W, H = screen.get_size()
                # Check if clicked on a marching unit
                unit_clicked = False
                for mu in _march_units[:]:
                    if mu.get("dead"): continue
                    if mu["is_cav"]:
                        ux, uy, uw, uh = int(mu["x"]), H - 270, 260, 260
                    else:
                        ux, uy, uw, uh = int(mu["x"]), H - 250, 120, 240
                    if ux <= mx <= ux+uw and uy <= my <= uy+uh:
                        mu["dead"] = True
                        _menu_explosions.append({"x": ux + uw//2 - 120, "y": uy + uh//2 - 120, "tick": 0})
                        unit_clicked = True; break
                if unit_clicked: continue
                if state == "main":
                    # Play button
                    play_rect = pygame.Rect(W//2-100, H//2-30, 200, 60)
                    if play_rect.collidepoint(mx, my): state = "maps"
                    # Codex button
                    codex_rect = pygame.Rect(W//2-100, H//2+50, 200, 60)
                    if codex_rect.collidepoint(mx, my): state = "codex"; show_menu._codex_sel = None
                    # Exit button
                    exit_rect = pygame.Rect(W//2-100, H//2+130, 200, 60)
                    if exit_rect.collidepoint(mx, my): pygame.quit(); sys.exit()
                elif state == "maps":
                    W, H = screen.get_size()
                    card_w, card_h = 320, 240; gap = 40
                    start_x = 50 - _menu_scroll
                    cy = H//2 - card_h//2
                    scenarios_keys = ["1754", "1790", "1809", "1835", "1858"]
                    for i, key in enumerate(scenarios_keys):
                        cx = start_x + i * (card_w + gap)
                        if cx <= mx <= cx+card_w and cy <= my <= cy+card_h: return key
                    # Back button
                    back_rect = pygame.Rect(W//2-60, cy + card_h + 20, 120, 40)
                    if back_rect.collidepoint(mx, my): state = "main"
                elif state == "codex":
                    _codex_nations = ["Great Britain", "France", "Spain", "Russia", "Denmark", "United States", "Confederate States", "Mexico", "Haiti", "Texas", "Canada", "Iroquois", "Wabanaki", "Comanche", "Cree", "Dakota"]
                    if not hasattr(show_menu, '_codex_sel') or show_menu._codex_sel is None:
                        # Grid view — check if clicked on a nation
                        cols = 5; flag_sz = 80; cell_w = 180; cell_h = 130
                        total_rows = (len(_codex_nations) + cols - 1) // cols
                        grid_w = cols * cell_w; grid_h = total_rows * cell_h
                        gx = W//2 - grid_w//2; gy = H//2 - grid_h//2
                        for i, nation in enumerate(_codex_nations):
                            row, col_i = i // cols, i % cols
                            cx_n = gx + col_i * cell_w; cy_n = gy + row * cell_h
                            if cx_n <= mx <= cx_n+cell_w and cy_n <= my <= cy_n+cell_h:
                                show_menu._codex_sel = nation; break
                        # Back button
                        back_rect = pygame.Rect(W//2-60, gy + grid_h + 20, 120, 40)
                        if back_rect.collidepoint(mx, my): state = "main"
                    else:
                        # Detail view — back button and arrows
                        back_rect = pygame.Rect(W//2-60, H - 80, 120, 40)
                        if back_rect.collidepoint(mx, my): show_menu._codex_sel = None; show_menu._codex_variant = 0
                        # Left arrow
                        unit_y = 350
                        left_arrow = pygame.Rect(W//2 - 220, unit_y + 60, 30, 30)
                        if left_arrow.collidepoint(mx, my): show_menu._codex_variant -= 1
                        # Right arrow
                        right_arrow = pygame.Rect(W//2 + 190, unit_y + 60, 30, 30)
                        if right_arrow.collidepoint(mx, my): show_menu._codex_variant += 1
        W, H = screen.get_size()
        screen.blit(pygame.transform.scale(menu_bg, (W, H)), (0, 0))
        # Darken overlay
        overlay = pygame.Surface((W, H), pygame.SRCALPHA); overlay.fill((0, 0, 0, 120))
        screen.blit(overlay, (0, 0))
        # Title (only show on main and maps screens)
        if state in ("main", "maps"):
            title_surf = title_font.render("Colonial Simulator", True, (255, 220, 100))
            screen.blit(title_surf, (W//2 - title_surf.get_width()//2, H//4))
            subtitle_surf = small_font.render("Grand Strategy Colonial Simulation", True, (200, 200, 200))
            screen.blit(subtitle_surf, (W//2 - subtitle_surf.get_width()//2, H//4 + 60))
        if state == "main":
            # Play button
            play_rect = pygame.Rect(W//2-100, H//2-30, 200, 60)
            _draw_btn_frame(play_rect)
            play_text = btn_font.render("Play", True, (255, 255, 255))
            screen.blit(play_text, (W//2 - play_text.get_width()//2, H//2 - play_text.get_height()//2))
            # Codex button
            codex_rect = pygame.Rect(W//2-100, H//2+50, 200, 60)
            _draw_btn_frame(codex_rect)
            codex_text = btn_font.render("Codex", True, (255, 255, 255))
            screen.blit(codex_text, (W//2 - codex_text.get_width()//2, H//2 + 50 + 30 - codex_text.get_height()//2))
            # Exit button
            exit_rect = pygame.Rect(W//2-100, H//2+130, 200, 60)
            _draw_btn_frame(exit_rect)
            exit_text = btn_font.render("Exit", True, (255, 255, 255))
            screen.blit(exit_text, (W//2 - exit_text.get_width()//2, H//2 + 130 + 30 - exit_text.get_height()//2))
        elif state == "codex":
            # Centered grid of nation flags with names
            _codex_nations = ["Great Britain", "France", "Spain", "Russia", "Denmark", "United States", "Confederate States", "Mexico", "Haiti", "Texas", "Canada", "Iroquois", "Wabanaki", "Comanche", "Cree", "Dakota"]
            _codex_flags = {"Great Britain":"images/flags/britian.png","France":"images/flags/france.png","Spain":"images/flags/spain.png","Russia":"images/flags/russia.webp","Denmark":"images/flags/denmark.webp","United States":"images/flags/usa.webp","Confederate States":"images/flags/confederatestates.png","Mexico":"images/flags/mexico.png","Haiti":"images/flags/haiti.png","Texas":"images/flags/texas.webp","Canada":"images/flags/canada.png","Iroquois":"images/flags/iroquois.png","Wabanaki":"images/flags/wabanaki.png","Comanche":"images/flags/comanche.png","Cree":"images/flags/cree.webp","Dakota":"images/flags/dakota.png"}
            _codex_alt_flags = {
                "Spain": [("images/flags/spain.png", "Bourbon (pre-1785)"), ("images/flags/kingdomofspain.webp", "Kingdom (1785+)")],
                "France": [("images/flags/france.png", "Monarchy"), ("images/flags/francerp.webp", "Republic (1792+)")],
                "United States": [("images/continent.webp", "Continental"), ("images/flags/usa.webp", "Stars & Stripes")],
            }
            _bold_font = pygame.font.SysFont(None, 22, bold=True)
            if not hasattr(show_menu, '_codex_sel') or show_menu._codex_sel is None:
                # Grid view — show all nations centered
                codex_title = btn_font.render("Codex — Nations", True, (255, 220, 100))
                screen.blit(codex_title, (W//2 - codex_title.get_width()//2, 30))
                cols = 5; flag_sz = 80; cell_w = 180; cell_h = 130
                total_rows = (len(_codex_nations) + cols - 1) // cols
                grid_w = cols * cell_w; grid_h = total_rows * cell_h
                gx = W//2 - grid_w//2; gy = H//2 - grid_h//2
                for i, nation in enumerate(_codex_nations):
                    row, col_i = i // cols, i % cols
                    cx = gx + col_i * cell_w + cell_w//2
                    cy_n = gy + row * cell_h
                    # Flag
                    try:
                        flag = pygame.transform.scale(pygame.image.load(_codex_flags[nation]).convert_alpha(), (flag_sz, flag_sz))
                        screen.blit(flag, (cx - flag_sz//2, cy_n))
                    except: pass
                    # Name under flag (bold)
                    name_surf = _bold_font.render(nation, True, (255, 255, 255))
                    screen.blit(name_surf, (cx - name_surf.get_width()//2, cy_n + flag_sz + 5))
                # Back button
                back_rect = pygame.Rect(W//2-60, gy + grid_h + 20, 120, 40)
                _draw_btn_frame(back_rect)
                screen.blit(small_font.render("Back", True, (255, 255, 255)), (W//2 - 18, gy + grid_h + 30))
            else:
                # Detail view for selected nation
                sel = show_menu._codex_sel
                if not hasattr(show_menu, '_codex_variant'): show_menu._codex_variant = 0
                codex_title = btn_font.render(f"Codex — {sel}", True, (255, 220, 100))
                screen.blit(codex_title, (W//2 - codex_title.get_width()//2, 30))
                # Unit variants per nation: (label, inf_path, cav_path, date_range)
                _codex_variants = {
                    "Great Britain": [("Standard", "images/sprites/britinf.png", "images/sprites/britishcav.png", "1754-1800"), ("1800s", "images/sprites/britinf1800.png", "images/sprites/britcav1800.png", "1800+")],
                    "France": [("Monarchy", "images/sprites/frenchinf.png", "images/sprites/frenchcav.png", "1754-1792"), ("Republic", "images/sprites/frenchinfrv.png", "images/sprites/frenchcavrv.png", "1792-1800"), ("1800s", "images/sprites/frenchinf1800.png", "images/sprites/frenchcav1800.png", "1800+")],
                    "Spain": [("Standard", "images/sprites/spaininf.png", "images/sprites/spanishcav.png", "1754-1800"), ("1800s", "images/sprites/spaininf1800.png", "images/sprites/spaincav1800.png", "1800+")],
                    "Russia": [("Standard", "images/sprites/rusinf.png", "images/sprites/russiancav.png", "1754-1800"), ("1800s", "images/sprites/rusinf1800.png", "images/sprites/ruscav1800.png", "1800+")],
                    "Denmark": [("Standard", "images/sprites/daneinf.png", "images/sprites/danecav.png", "1754-1800"), ("1800s", "images/sprites/daneinf1800.png", "images/sprites/danecav1800.png", "1800+")],
                    "United States": [("Standard", "images/sprites/americaninf.png", "images/sprites/americancav.png", "1775-1800"), ("1800s", "images/sprites/americaninf1800.png", "images/sprites/americancav1800.png", "1800-1850"), ("Union", "images/sprites/unioninf.png", "images/sprites/unioncav.png", "1850+")],
                    "Mexico": [("Standard", "images/sprites/mexinf.png", "images/sprites/mexcav.png", "1810+")],
                    "Haiti": [("Standard", "images/sprites/haitinf.png", "images/sprites/haiticav.png", "1804+")],
                    "Texas": [("Standard", "images/sprites/texinf.png", "images/sprites/texcav.png", "1836+")],
                    "Confederate States": [("Standard", "images/sprites/confedinf.png", "images/sprites/confedcav.png", "1861+")],
                    "Iroquois": [("Warrior", "images/sprites/gunbearer.png", None, "All eras")],
                    "Wabanaki": [("Warrior", "images/sprites/gunbearer.png", None, "All eras")],
                    "Comanche": [("Warrior", "images/sprites/gunbearer.png", "images/sprites/horseman.png", "All eras")],
                    "Cree": [("Bowman", "images/sprites/bowman.png", None, "All eras")],
                    "Dakota": [("Bowman", "images/sprites/bowman.png", "images/sprites/horseman.png", "All eras")],
                    "Canada": [("Standard", "images/sprites/caninf.png", "images/sprites/cancav.png", "1867+")],
                }
                variants = _codex_variants.get(sel, [("Standard", None, None, "")])
                var_idx = show_menu._codex_variant % len(variants)
                var_label, var_inf_path, var_cav_path, var_dates = variants[var_idx]
                # Main flag large
                try:
                    main_flag = pygame.transform.scale(pygame.image.load(_codex_flags[sel]).convert_alpha(), (120, 120))
                    screen.blit(main_flag, (W//2 - 60, 70))
                except: pass
                screen.blit(btn_font.render(sel, True, (255, 255, 255)), (W//2 - btn_font.size(sel)[0]//2, 200))
                # Alternate flags
                alt_flags = _codex_alt_flags.get(sel, [])
                if alt_flags:
                    screen.blit(_bold_font.render("Historical Flags:", True, (200, 200, 200)), (W//2 - 80, 235))
                    fx = W//2 - len(alt_flags) * 50
                    for flag_path, label in alt_flags:
                        try:
                            af = pygame.transform.scale(pygame.image.load(flag_path).convert_alpha(), (50, 50))
                            screen.blit(af, (fx, 258))
                        except: pass
                        lbl = small_font.render(label, True, (180, 180, 180))
                        screen.blit(lbl, (fx + 25 - lbl.get_width()//2, 312))
                        fx += 120
                # Unit variant section with arrows (only show arrows if more than 1 variant)
                unit_y = 350
                if len(variants) > 1:
                    # Left arrow
                    left_arrow = pygame.Rect(W//2 - 220, unit_y + 60, 30, 30)
                    pygame.draw.polygon(screen, (255, 220, 100), [(left_arrow.right, left_arrow.top), (left_arrow.left, left_arrow.centery), (left_arrow.right, left_arrow.bottom)])
                    # Right arrow
                    right_arrow = pygame.Rect(W//2 + 190, unit_y + 60, 30, 30)
                    pygame.draw.polygon(screen, (255, 220, 100), [(right_arrow.left, right_arrow.top), (right_arrow.right, right_arrow.centery), (right_arrow.left, right_arrow.bottom)])
                # Variant label and dates
                var_title = _bold_font.render(f"{var_label}  ({var_dates})", True, (255, 220, 100))
                screen.blit(var_title, (W//2 - var_title.get_width()//2, unit_y - 20))
                # Infantry walking animation
                if var_inf_path:
                    try:
                        _cinf = sprites._load_infantry_sheet(var_inf_path)
                        if "walk_right" in _cinf:
                            frames = _cinf["walk_right"]
                            idx = (_march_tick // 10) % len(frames)
                            frame = frames[idx]
                            scaled = pygame.transform.scale(frame, (80, 160))
                            screen.blit(scaled, (W//2 - 150, unit_y + 10))
                            screen.blit(_bold_font.render("Infantry", True, (200, 200, 200)), (W//2 - 140, unit_y + 175))
                    except: pass
                # Cavalry walking animation
                if var_cav_path:
                    try:
                        _ccav = sprites._load_cavalry_sheet(var_cav_path)
                        if "walk_right" in _ccav:
                            frames = _ccav["walk_right"]
                            idx = (_march_tick // 10) % len(frames)
                            frame = frames[idx]
                            scaled = pygame.transform.scale(frame, (160, 160))
                            screen.blit(scaled, (W//2 + 30, unit_y + 10))
                            screen.blit(_bold_font.render("Cavalry", True, (200, 200, 200)), (W//2 + 70, unit_y + 175))
                    except: pass
                # Back button
                back_rect = pygame.Rect(W//2-60, H - 80, 120, 40)
                _draw_btn_frame(back_rect)
                screen.blit(small_font.render("Back", True, (255, 255, 255)), (W//2 - 18, H - 70))
        elif state == "maps":
            # Map selection header
            header = btn_font.render("Select Scenario", True, (255, 255, 255))
            screen.blit(header, (W//2 - header.get_width()//2, H//4 - 30))
            # Scenario cards — scrollable horizontally
            card_w, card_h = 320, 240; gap = 40
            total_w = len(scenarios) * (card_w + gap) - gap if 'scenarios' in dir() else (card_w + gap) * 5 - gap
            # Clamp scroll
            max_scroll = max(0, total_w - W + 100)
            _menu_scroll = max(0, min(max_scroll, _menu_scroll))
            start_x = 50 - _menu_scroll
            cy = H//2 - card_h//2
            # Load scenario images (cached)
            if not hasattr(show_menu, '_scen_imgs'):
                show_menu._scen_imgs = [
                    pygame.transform.scale(pygame.image.load("images/7yearswar.jpg").convert(), (card_w, card_h)),
                    pygame.transform.scale(pygame.image.load("images/continentalwars.png").convert(), (card_w, card_h)),
                    pygame.transform.scale(pygame.image.load("images/warof1812.png").convert(), (card_w, card_h)),
                    pygame.transform.scale(pygame.image.load("images/mexicanamericanwar.png").convert(), (card_w, card_h)),
                    pygame.transform.scale(pygame.image.load("images/americancivilwar.png").convert(), (card_w, card_h)),
                ]
            _card_font = pygame.font.SysFont(None, 32, bold=True)
            _card_title_font = pygame.font.SysFont(None, 28, bold=True)
            scenarios = [
                (show_menu._scen_imgs[0], "1754", "7 Years War"),
                (show_menu._scen_imgs[1], "1790", "Continental Wars"),
                (show_menu._scen_imgs[2], "1809", "War of 1812"),
                (show_menu._scen_imgs[3], "1835", "Mexican American War"),
                (show_menu._scen_imgs[4], "1858", "American Civil War"),
            ]
            for i, (img, date, title) in enumerate(scenarios):
                cx = start_x + i * (card_w + gap)
                # Draw card image
                screen.blit(img, (cx, cy))
                # Border
                pygame.draw.rect(screen, (200, 180, 80), (cx, cy, card_w, card_h), 2)
                # Date on top (bold, with dark shadow)
                date_surf = _card_font.render(date, True, (255, 255, 255))
                date_shad = _card_font.render(date, True, (0, 0, 0))
                screen.blit(date_shad, (cx + card_w//2 - date_surf.get_width()//2 + 1, cy + 9))
                screen.blit(date_surf, (cx + card_w//2 - date_surf.get_width()//2, cy + 8))
                # Title on bottom (bold, with dark shadow)
                title_surf = _card_title_font.render(title, True, (255, 220, 100))
                title_shad = _card_title_font.render(title, True, (0, 0, 0))
                screen.blit(title_shad, (cx + card_w//2 - title_surf.get_width()//2 + 1, cy + card_h - 30))
                screen.blit(title_surf, (cx + card_w//2 - title_surf.get_width()//2, cy + card_h - 31))
            # Back button
            back_rect = pygame.Rect(W//2-60, cy + card_h + 20, 120, 40)
            _draw_btn_frame(back_rect)
            back_text = small_font.render("Back", True, (255, 255, 255))
            screen.blit(back_text, (W//2 - back_text.get_width()//2, cy + card_h + 30))
            # Scroll hint arrows
            if _menu_scroll > 0:
                pygame.draw.polygon(screen, (255, 220, 100), [(20, H//2), (40, H//2-20), (40, H//2+20)])
            num_scenarios = len(scenarios)
            max_scroll = max(0, num_scenarios * (card_w + gap) - gap - W + 100)
            if _menu_scroll < max_scroll:
                pygame.draw.polygon(screen, (255, 220, 100), [(W-20, H//2), (W-40, H//2-20), (W-40, H//2+20)])
            screen.blit(small_font.render("Scroll to see more", True, (150,150,150)), (W//2-60, cy + card_h + 65))
        # Draw marching units along bottom (not in codex)
        if state != "codex":
            _march_tick += 1
            for mu in _march_units:
                if mu.get("dead"): continue
                mu["x"] += 0.8  # march speed
                if mu["x"] > W + 160: mu["x"] = -160 - random.randint(0, 200)
                idx = (_march_tick // 12) % len(mu["frames"])
                frame = mu["frames"][idx]
                if mu["is_cav"]:
                    scaled = pygame.transform.scale(frame, (260, 260))
                    screen.blit(scaled, (int(mu["x"]), H - 270))
                else:
                    scaled = pygame.transform.scale(frame, (120, 240))
                    screen.blit(scaled, (int(mu["x"]), H - 250))
                    # Draw flag above flag bearer
                    if mu.get("flag"):
                        fx = int(mu["x"]) + 60
                        fy_top = H - 250 - 15
                        fy_bottom = H - 250 + 65
                        # Brown pole
                        pygame.draw.line(screen, (139, 90, 43), (fx, fy_bottom), (fx, fy_top), 3)
                        # Flag at top (left edge of flag at pole)
                        screen.blit(mu["flag"], (fx - 70, fy_top - 40))
            # Draw explosions
            for exp in _menu_explosions[:]:
                exp["tick"] += 1
                fidx = exp["tick"] // 3
                if fidx >= len(_menu_exp_frames):
                    _menu_explosions.remove(exp)
                else:
                    screen.blit(_menu_exp_frames[fidx], (exp["x"], exp["y"]))
        pygame.display.update(); clock.tick(60)

_selected_map = show_menu()

# === Apply 1790 scenario if selected ===
if _selected_map == "1790":
    game_day, game_month, game_year = 1, 1, 1790
    # Spain already changed flag by 1785
    try: faction_images["Spain"] = pygame.transform.scale(pygame.image.load("images/flags/kingdomofspain.webp").convert_alpha(), (30, 30))
    except: pass
    # Add United States
    FACTION_COLORS["United States"] = (100, 180, 255)
    if "United States" not in FACTIONS: FACTIONS.append("United States")
    if "United States" not in faction_materials: faction_materials["United States"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["United States"][_mat] = _val
    faction_materials["United States"]["Gold"] = 50
    try: faction_images["United States"] = pygame.transform.scale(pygame.image.load("images/flags/usa.webp"), (30, 30))
    except: pass
    # France is still monarchy in 1790 — revolution happens in 1792
    # Remove Iroquois and Wabanaki
    eliminated_factions.add("Iroquois"); eliminated_factions.add("Wabanaki")
    # Reassign cities
    _1800_assignments = {
        "Washington": ("United States", True),
        "Boston": ("United States", False),
        "New York": ("United States", False),
        "Charleston": ("United States", False),
        "Williamsburg": ("United States", False),
        "Richmond": ("United States", False),
        "Onondaga": ("United States", False),
        "Norridgewock": ("United States", False),
        "New Orleans": ("France", True),
        "Saint-Louis": ("France", False),
        "York Factory": ("Rupert's Land", True),
        "Quebec": ("Great Britain", False),
        "Montreal": ("Great Britain", False),
        "Tadoussac": ("Great Britain", False),
        "Plaisance": ("Great Britain", False),
        "Nassau": ("Great Britain", False),
        "Kingston": ("Great Britain", False),
        "Halifax": ("Great Britain", False),
        "Mexico City": ("Spain", True),
        "Havana": ("Spain", False),
        "Merida": ("Spain", False),
        "Albuquerque": ("Spain", False),
        "San Agustin": ("Spain", False),
        "Monterrey": ("Spain", False),
        "Caracas": ("Spain", False),
        "Port-au-Prince": ("France", False),
        "San Antonio": ("Spain", False),
    }
    # Fort Detroit renamed to Detroit, transferred to US
    for c in cities:
        if c["name"] == "Fort Detroit":
            c["name"] = "Detroit"; c["owner"] = "United States"; c["sovereign"] = "United States"
            c["color"] = (100, 180, 255); c["is_capital"] = False
    # Apply assignments
    for c in cities:
        if c["name"] in _1800_assignments:
            new_owner, is_cap = _1800_assignments[c["name"]]
            c["owner"] = new_owner; c["sovereign"] = new_owner
            c["color"] = FACTION_COLORS.get(new_owner, c["color"])
            c["is_capital"] = is_cap
            c["occupier"] = None
    # Remove units/merchants from eliminated factions
    units[:] = [u for u in units if u.owner["owner"] not in ("Iroquois", "Wabanaki")]
    merchants[:] = [m for m in merchants if m.owner_faction not in ("Iroquois", "Wabanaki")]
    settlers[:] = [s for s in settlers if s.owner_faction not in ("Iroquois", "Wabanaki")]
    # Init focus for United States
    _faction_focus["United States"] = random.choice(FOCUSES)
    _faction_focus_timer["United States"] = random.randint(DAY_TICKS * 30, DAY_TICKS * 1095)
    # Starting alliance: France & United States
    alliances[frozenset({"France", "United States"})] = DAY_TICKS * 730

# === Apply 1809 scenario (War of 1812) if selected ===
if _selected_map == "1809":
    game_day, game_month, game_year = 1, 1, 1809
    # Spain changed flag by 1785
    try: faction_images["Spain"] = pygame.transform.scale(pygame.image.load("images/flags/kingdomofspain.webp").convert_alpha(), (30, 30))
    except: pass
    # Add United States
    FACTION_COLORS["United States"] = (100, 180, 255)
    if "United States" not in FACTIONS: FACTIONS.append("United States")
    if "United States" not in faction_materials: faction_materials["United States"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["United States"][_mat] = _val
    faction_materials["United States"]["Gold"] = 60
    try: faction_images["United States"] = pygame.transform.scale(pygame.image.load("images/flags/usa.webp").convert_alpha(), (30, 30))
    except: pass
    # France republic flag
    try: faction_images["France"] = pygame.transform.scale(pygame.image.load("images/flags/francerp.webp").convert_alpha(), (30, 30))
    except: pass
    # French revolutionary sprites
    try:
        sprites.infantry_anims["France"] = sprites._load_infantry_sheet("images/sprites/frenchinfrv.png")
        sprites.cavalry_anims["France"] = sprites._load_cavalry_sheet("images/sprites/frenchcavrv.png")
    except: pass
    # Remove Iroquois and Wabanaki
    eliminated_factions.add("Iroquois"); eliminated_factions.add("Wabanaki")
    # Add Haiti (independent since 1804)
    FACTION_COLORS["Haiti"] = (0, 0, 150)
    if "Haiti" not in FACTIONS: FACTIONS.append("Haiti")
    if "Haiti" not in faction_materials: faction_materials["Haiti"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["Haiti"][_mat] = _val
    faction_materials["Haiti"]["Gold"] = 20
    try: faction_images["Haiti"] = pygame.transform.scale(pygame.image.load("images/flags/haiti.png").convert_alpha(), (30, 30))
    except: pass
    try:
        sprites.infantry_anims["Haiti"] = sprites._load_infantry_sheet("images/sprites/haitinf.png")
        sprites.cavalry_anims["Haiti"] = sprites._load_cavalry_sheet("images/sprites/haiticav.png")
    except: pass
    # Reassign cities
    _1809_assignments = {
        "Washington": ("United States", True),
        "Boston": ("United States", False),
        "New York": ("United States", False),
        "Charleston": ("United States", False),
        "Williamsburg": ("United States", False),
        "Richmond": ("United States", False),
        "New Orleans": ("United States", False),
        "Saint-Louis": ("United States", False),
        "Onondaga": ("United States", False),
        "Norridgewock": ("United States", False),
        "York Factory": ("Rupert's Land", True),
        "Quebec": ("Great Britain", False),
        "Montreal": ("Great Britain", False),
        "Tadoussac": ("Great Britain", False),
        "Plaisance": ("Great Britain", False),
        "Nassau": ("Great Britain", False),
        "Kingston": ("Great Britain", False),
        "Halifax": ("Great Britain", False),
        "Mexico City": ("Spain", True),
        "Havana": ("Spain", False),
        "Merida": ("Spain", False),
        "Albuquerque": ("Spain", False),
        "San Agustin": ("Spain", False),
        "Monterey": ("Spain", False),
        "San Diego": ("Spain", False),
        "Chihuahua": ("Spain", False),
        "Caracas": ("Spain", False),
        "Port-au-Prince": ("Haiti", True),
        "Saint-Pierre": ("France", True),
    }
    # Fort Detroit renamed to Detroit, US owned
    for c in cities:
        if c["name"] == "Fort Detroit":
            c["name"] = "Detroit"; c["owner"] = "United States"; c["sovereign"] = "United States"
            c["color"] = (100, 180, 255); c["is_capital"] = False
    # Apply assignments
    for c in cities:
        if c["name"] in _1809_assignments:
            new_owner, is_cap = _1809_assignments[c["name"]]
            c["owner"] = new_owner; c["sovereign"] = new_owner
            c["color"] = FACTION_COLORS.get(new_owner, c["color"])
            c["is_capital"] = is_cap
            c["occupier"] = None
    # Remove units from eliminated factions
    units[:] = [u for u in units if u.owner["owner"] not in ("Iroquois", "Wabanaki")]
    merchants[:] = [m for m in merchants if m.owner_faction not in ("Iroquois", "Wabanaki")]
    settlers[:] = [s for s in settlers if s.owner_faction not in ("Iroquois", "Wabanaki")]
    # Init focuses
    _faction_focus["United States"] = "Aggressive"
    _faction_focus_timer["United States"] = random.randint(DAY_TICKS * 60, DAY_TICKS * 365)
    _faction_focus["Haiti"] = random.choice(FOCUSES)
    _faction_focus_timer["Haiti"] = random.randint(DAY_TICKS * 30, DAY_TICKS * 1095)
    # US and Britain start at war
    declare_war("United States", "Great Britain", "")
    # Mark events as fired
    _revolution_fired = True; _haiti_fired = True

# === Apply 1845 scenario if selected ===
if _selected_map == "1835":
    game_day, game_month, game_year = 1, 1, 1835
    # Spain changed flag by 1785
    try: faction_images["Spain"] = pygame.transform.scale(pygame.image.load("images/flags/kingdomofspain.webp").convert_alpha(), (30, 30))
    except: pass
    # Add United States
    FACTION_COLORS["United States"] = (100, 180, 255)
    if "United States" not in FACTIONS: FACTIONS.append("United States")
    if "United States" not in faction_materials: faction_materials["United States"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["United States"][_mat] = _val
    faction_materials["United States"]["Gold"] = 80
    try: faction_images["United States"] = pygame.transform.scale(pygame.image.load("images/flags/usa.webp").convert_alpha(), (30, 30))
    except: pass
    # France republic flag
    try: faction_images["France"] = pygame.transform.scale(pygame.image.load("images/flags/francerp.webp").convert_alpha(), (30, 30))
    except: pass
    # Add Mexico
    FACTION_COLORS["Mexico"] = (0, 100, 50)
    if "Mexico" not in FACTIONS: FACTIONS.append("Mexico")
    if "Mexico" not in faction_materials: faction_materials["Mexico"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["Mexico"][_mat] = _val
    faction_materials["Mexico"]["Gold"] = 40
    try: faction_images["Mexico"] = pygame.transform.scale(pygame.image.load("images/flags/mexico.png").convert_alpha(), (30, 30))
    except: pass
    try:
        sprites.infantry_anims["Mexico"] = sprites._load_infantry_sheet("images/sprites/mexinf.png")
        sprites.cavalry_anims["Mexico"] = sprites._load_cavalry_sheet("images/sprites/mexcav.png")
    except: pass
    # Add Haiti
    FACTION_COLORS["Haiti"] = (0, 0, 150)
    if "Haiti" not in FACTIONS: FACTIONS.append("Haiti")
    if "Haiti" not in faction_materials: faction_materials["Haiti"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["Haiti"][_mat] = _val
    faction_materials["Haiti"]["Gold"] = 20
    try: faction_images["Haiti"] = pygame.transform.scale(pygame.image.load("images/flags/haiti.png").convert_alpha(), (30, 30))
    except: pass
    try:
        sprites.infantry_anims["Haiti"] = sprites._load_infantry_sheet("images/sprites/haitinf.png")
        sprites.cavalry_anims["Haiti"] = sprites._load_cavalry_sheet("images/sprites/haiticav.png")
    except: pass
    # Remove Iroquois and Wabanaki
    eliminated_factions.add("Iroquois"); eliminated_factions.add("Wabanaki")
    # Reassign cities
    _1845_assignments = {
        # United States — east coast + Louisiana + Detroit
        "Washington": ("United States", True),
        "Boston": ("United States", False),
        "New York": ("United States", False),
        "Charleston": ("United States", False),
        "Williamsburg": ("United States", False),
        "Richmond": ("United States", False),
        "New Orleans": ("United States", False),
        "Saint-Louis": ("United States", False),
        "Onondaga": ("United States", False),
        "Norridgewock": ("United States", False),
        "San Agustin": ("Spain", False),
        # Mexico — southwest + central america
        "Mexico City": ("Mexico", True),
        "Chihuahua": ("Mexico", False),
        "Merida": ("Mexico", False),
        "Oaxaca": ("Mexico", False),
        "Albuquerque": ("Mexico", False),
        "San Diego": ("Mexico", False),
        "Monterey": ("Mexico", False),
        "San Antonio": ("Mexico", False),
        # Haiti
        "Port-au-Prince": ("Haiti", True),
        # Great Britain — Canada
        "York Factory": ("Rupert's Land", True),
        "Quebec": ("Great Britain", False),
        "Montreal": ("Great Britain", False),
        "Tadoussac": ("Great Britain", False),
        "Plaisance": ("Great Britain", False),
        "Nassau": ("Great Britain", False),
        "Kingston": ("Great Britain", False),
        # Spain — Caribbean
        "Havana": ("Spain", True),
        "Caracas": ("Spain", False),
        # France
        "Saint-Pierre": ("France", True),
    }
    # Fort Detroit renamed to Detroit
    for c in cities:
        if c["name"] == "Fort Detroit":
            c["name"] = "Detroit"; c["owner"] = "United States"; c["sovereign"] = "United States"
            c["color"] = (100, 180, 255); c["is_capital"] = False
    # Apply assignments
    for c in cities:
        if c["name"] in _1845_assignments:
            new_owner, is_cap = _1845_assignments[c["name"]]
            c["owner"] = new_owner; c["sovereign"] = new_owner
            c["color"] = FACTION_COLORS.get(new_owner, c["color"])
            c["is_capital"] = is_cap
            c["occupier"] = None
    # Remove units from eliminated factions
    units[:] = [u for u in units if u.owner["owner"] not in ("Iroquois", "Wabanaki")]
    merchants[:] = [m for m in merchants if m.owner_faction not in ("Iroquois", "Wabanaki")]
    settlers[:] = [s for s in settlers if s.owner_faction not in ("Iroquois", "Wabanaki")]
    # Init focuses
    for f in ["United States", "Mexico", "Haiti"]:
        _faction_focus[f] = random.choice(FOCUSES)
        _faction_focus_timer[f] = random.randint(DAY_TICKS * 30, DAY_TICKS * 1095)
    # Swap to 1800 sprites (it's past 1800)
    _swap_to_1800()
    # Mark events as already fired
    _revolution_fired = True; _mexico_fired = True; _haiti_fired = True

# === Apply 1858 scenario (American Civil War) if selected ===
if _selected_map == "1858":
    game_day, game_month, game_year = 1, 1, 1858
    # Spain changed flag
    try: faction_images["Spain"] = pygame.transform.scale(pygame.image.load("images/flags/kingdomofspain.webp").convert_alpha(), (30, 30))
    except: pass
    # Add United States
    FACTION_COLORS["United States"] = (100, 180, 255)
    if "United States" not in FACTIONS: FACTIONS.append("United States")
    if "United States" not in faction_materials: faction_materials["United States"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["United States"][_mat] = _val
    faction_materials["United States"]["Gold"] = 100
    try: faction_images["United States"] = pygame.transform.scale(pygame.image.load("images/flags/usa.webp").convert_alpha(), (30, 30))
    except: pass
    # France republic flag
    try: faction_images["France"] = pygame.transform.scale(pygame.image.load("images/flags/francerp.webp").convert_alpha(), (30, 30))
    except: pass
    # Add Mexico
    FACTION_COLORS["Mexico"] = (0, 100, 50)
    if "Mexico" not in FACTIONS: FACTIONS.append("Mexico")
    if "Mexico" not in faction_materials: faction_materials["Mexico"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["Mexico"][_mat] = _val
    faction_materials["Mexico"]["Gold"] = 40
    try: faction_images["Mexico"] = pygame.transform.scale(pygame.image.load("images/flags/mexico.png").convert_alpha(), (30, 30))
    except: pass
    try:
        sprites.infantry_anims["Mexico"] = sprites._load_infantry_sheet("images/sprites/mexinf.png")
        sprites.cavalry_anims["Mexico"] = sprites._load_cavalry_sheet("images/sprites/mexcav.png")
    except: pass
    # Add Haiti
    FACTION_COLORS["Haiti"] = (0, 0, 150)
    if "Haiti" not in FACTIONS: FACTIONS.append("Haiti")
    if "Haiti" not in faction_materials: faction_materials["Haiti"] = {m: 0 for m in materials + LUXURY_RESOURCES}
    for _mat, _val in config.STARTING_RESOURCES.items(): faction_materials["Haiti"][_mat] = _val
    try: faction_images["Haiti"] = pygame.transform.scale(pygame.image.load("images/flags/haiti.png").convert_alpha(), (30, 30))
    except: pass
    try:
        sprites.infantry_anims["Haiti"] = sprites._load_infantry_sheet("images/sprites/haitinf.png")
        sprites.cavalry_anims["Haiti"] = sprites._load_cavalry_sheet("images/sprites/haiticav.png")
    except: pass
    # Texas was annexed by US in 1845, not independent in 1858
    eliminated_factions.add("Texas")
    # Remove Iroquois and Wabanaki
    eliminated_factions.add("Iroquois"); eliminated_factions.add("Wabanaki")
    # Reassign cities
    _1858_assignments = {
        # United States — large territory
        "Washington": ("United States", True),
        "Boston": ("United States", False),
        "New York": ("United States", False),
        "Charleston": ("United States", False),
        "Richmond": ("United States", False),
        "Williamsburg": ("United States", False),
        "New Orleans": ("United States", False),
        "Saint-Louis": ("United States", False),
        "Onondaga": ("United States", False),
        "Norridgewock": ("United States", False),
        "San Agustin": ("United States", False),
        "Albuquerque": ("United States", False),
        "Halifax": ("Great Britain", False),
        "Victoria": ("Great Britain", False),
        # Great Britain — Canada
        "York Factory": ("Rupert's Land", True),
        "Quebec": ("Great Britain", False),
        "Montreal": ("Great Britain", False),
        "Tadoussac": ("Great Britain", False),
        "Plaisance": ("Great Britain", False),
        "Nassau": ("Great Britain", False),
        "Kingston": ("Great Britain", False),
        # Mexico
        "Mexico City": ("Mexico", True),
        "Chihuahua": ("Mexico", False),
        "Merida": ("Mexico", False),
        "Oaxaca": ("Mexico", False),
        "Monterey": ("Mexico", False),
        "San Diego": ("Mexico", False),
        # Spain — Caribbean
        "Havana": ("Spain", True),
        "Caracas": ("Spain", False),
        # Haiti
        "Port-au-Prince": ("Haiti", True),
        # France
        "Saint-Pierre": ("France", True),
        # Texas cities are part of US by 1858
        "San Antonio": ("United States", False),
        "Austin": ("United States", False),
    }
    # Fort Detroit renamed to Detroit
    for c in cities:
        if c["name"] == "Fort Detroit":
            c["name"] = "Detroit"; c["owner"] = "United States"; c["sovereign"] = "United States"
            c["color"] = (100, 180, 255); c["is_capital"] = False
    # Austin is part of US in this scenario
    for c in cities:
        if c["name"] == "Austin": c["owner"] = "United States"; c["sovereign"] = "United States"; c["color"] = (100, 180, 255); c["is_capital"] = False
    # Apply assignments
    for c in cities:
        if c["name"] in _1858_assignments:
            new_owner, is_cap = _1858_assignments[c["name"]]
            c["owner"] = new_owner; c["sovereign"] = new_owner
            c["color"] = FACTION_COLORS.get(new_owner, c["color"])
            c["is_capital"] = is_cap
            c["occupier"] = None
    # Remove units from eliminated factions
    units[:] = [u for u in units if u.owner["owner"] not in ("Iroquois", "Wabanaki")]
    merchants[:] = [m for m in merchants if m.owner_faction not in ("Iroquois", "Wabanaki")]
    settlers[:] = [s for s in settlers if s.owner_faction not in ("Iroquois", "Wabanaki")]
    # Init focuses
    for f in ["United States", "Mexico", "Haiti"]:
        _faction_focus[f] = random.choice(FOCUSES)
        _faction_focus_timer[f] = random.randint(DAY_TICKS * 30, DAY_TICKS * 1095)
    # Swap to 1800 sprites and Union sprites (past 1850)
    _swap_to_1800()
    _swap_us_to_union()
    # Mark events as already fired
    _revolution_fired = True; _mexico_fired = True; _haiti_fired = True; _texas_fired = True

# Spawn any founded cities that should already exist at game start
for fc in _FOUNDED_CITIES[:]:
    if game_year >= fc["_founded_year"] and not any(c["name"] == fc["name"] for c in cities):
        fc["_region"] = _region_at(fc["x"], fc["y"])  # tag its territory at spawn
        cities.append(fc)
        _FOUNDED_CITIES.remove(fc)

# If the scenario starts in/after 1834, York already goes by Toronto
if game_year >= 1834:
    for c in cities:
        if c["name"] == "York":
            c["name"] = "Toronto"
            break

# Ensure Washington is US capital in all scenarios after 1790
if game_year >= 1790 and "United States" not in eliminated_factions:
    for c in cities:
        if c["name"] == "Washington" and c["owner"] == "United States":
            c["is_capital"] = True
        elif c["owner"] == "United States" and c["is_capital"] and c["name"] != "Washington":
            c["is_capital"] = False

# Ensure all cities start at full health
for _c in cities:
    if _c.get("is_camp"): _max = 30
    elif _c.get("is_village"): _max = 50
    elif _c.get("is_capital"): _max = 200
    else: _max = 100
    _max += _c.get("tier", 0) * 25
    _c["troops"] = _max

# === Pre-built buildings based on scenario year (historical development) ===
def _assign_starting_buildings():
    """Give cities pre-built buildings based on scenario start year. Later = more developed."""
    for city in cities:
        if city.get("is_fort") or city.get("is_camp"): continue
        owner = city["owner"]
        is_native = owner in NATIVE_FACTIONS
        buildings = city.get("buildings", [])
        name = city["name"]
        is_island = name in _ISLAND_CITY_NAMES

        if is_native:
            # Natives always start with Council Lodge
            if "Council Lodge" not in buildings:
                buildings.append("Council Lodge")
        else:
            # All colonial cities get Town Hall by default
            if "Town Hall" not in buildings:
                buildings.append("Town Hall")

        # 1754: minimal development — just Town Hall/Council Lodge
        if game_year <= 1754:
            # Major cities get a Road
            if not is_native and city["is_capital"] and not is_island and "Road" not in buildings:
                buildings.append("Road")

        # All scenarios: every settlement has a Farm
        if "Farm" not in buildings:
            buildings.append("Farm")

        # 1790+: some development
        if game_year >= 1790:
            if not is_native:
                if city["is_capital"] and not is_island and "Road" not in buildings:
                    buildings.append("Road")
                if city["is_capital"] and "Stables" not in buildings:
                    buildings.append("Stables")
                if city["is_capital"] and "Church" not in buildings:
                    buildings.append("Church")
                # Major trade cities get markets
                if name in ("New York", "Boston", "Havana", "Quebec", "Mexico City", "New Orleans") and "Market" not in buildings:
                    buildings.append("Market")

        # 1809+: more infrastructure
        if game_year >= 1809:
            if not is_native:
                if not city.get("is_village") and not is_island and "Road" not in buildings:
                    buildings.append("Road")
                if not city.get("is_village") and "Stables" not in buildings:
                    buildings.append("Stables")
                if city["is_capital"] and "Barracks" not in buildings:
                    buildings.append("Barracks")
                if name in ("New York", "Boston", "Havana", "Mexico City", "Quebec", "Kingston") and "Warehouse" not in buildings:
                    buildings.append("Warehouse")
                if city["material"] == "Lumber" and "Lumber Mill" not in buildings:
                    buildings.append("Lumber Mill")

        # 1835+: well-developed colonies
        if game_year >= 1835:
            if not is_native:
                if not city.get("is_village") and "Church" not in buildings:
                    buildings.append("Church")
                if not city.get("is_village") and "Barracks" not in buildings:
                    buildings.append("Barracks")
                if not city.get("is_village") and "Market" not in buildings:
                    buildings.append("Market")
                # Plantations in southern/tropical cities
                if name in ("Charleston", "Havana", "Kingston", "New Orleans", "Port-au-Prince", "Nassau") and "Plantation" not in buildings:
                    buildings.append("Plantation")
                # Foundries in industrial cities
                if name in ("New York", "Boston", "Mexico City", "Richmond") and "Foundry" not in buildings:
                    buildings.append("Foundry")

        # 1858+: fully developed
        if game_year >= 1858:
            if not is_native:
                if not city.get("is_village") and "Warehouse" not in buildings:
                    buildings.append("Warehouse")
                if not city.get("is_village") and "Stables" not in buildings:
                    buildings.append("Stables")
                # More plantations
                if name in ("San Antonio", "Merida", "San Agustin", "Caracas") and "Plantation" not in buildings:
                    buildings.append("Plantation")
                # More foundries
                if name in ("Halifax", "Quebec", "Charleston", "Saint-Louis") and "Foundry" not in buildings:
                    buildings.append("Foundry")
                # Shipyards in port cities
                if name in ("Boston", "New York", "Halifax", "Havana", "Kingston") and "Shipyard" not in buildings:
                    buildings.append("Shipyard")
            else:
                # Natives get some development too by 1858
                if "Market" not in buildings:
                    buildings.append("Market")

        city["buildings"] = buildings

        # === Assign tiers based on scenario year and city importance ===
        if city.get("is_fort") or city.get("is_camp"): continue
        # Major capitals and long-established cities get higher tiers in later scenarios
        _tier2_by_1790 = {"Mexico City", "Boston", "New York", "Havana", "Quebec", "Montreal"}
        _tier2_by_1809 = {"Mexico City", "Boston", "New York", "Havana", "Quebec", "Montreal", "Charleston", "Kingston", "New Orleans", "Richmond", "Halifax", "Washington", "Oaxaca"}
        _tier3_by_1835 = {"Mexico City", "New York", "Boston", "Havana", "Montreal"}
        _tier2_by_1835 = {"Charleston", "Kingston", "New Orleans", "Richmond", "Halifax", "Quebec", "Saint-Louis", "Port-au-Prince", "Monterey", "San Agustin", "Chicago", "Washington"}
        _tier3_by_1858 = {"Mexico City", "New York", "Boston", "Havana", "New Orleans", "Charleston", "Quebec", "Montreal", "Chicago"}
        _tier2_by_1858 = {"Richmond", "Halifax", "Kingston", "Saint-Louis", "Port-au-Prince", "Monterey", "San Agustin", "Merida", "Chihuahua", "San Diego", "Sitka", "Godthaab", "Caracas", "Barranquilla"}

        if game_year >= 1858:
            if name in _tier3_by_1858: city["tier"] = 3
            elif name in _tier2_by_1858 or city["is_capital"]: city["tier"] = 2
            else: city["tier"] = max(city.get("tier", 1), 1)
        elif game_year >= 1835:
            if name in _tier3_by_1835: city["tier"] = 3
            elif name in _tier2_by_1835 or city["is_capital"]: city["tier"] = 2
            else: city["tier"] = max(city.get("tier", 1), 1)
        elif game_year >= 1809:
            if name in _tier2_by_1809 or city["is_capital"]: city["tier"] = 2
            else: city["tier"] = max(city.get("tier", 1), 1)
        elif game_year >= 1790:
            if name in _tier2_by_1790: city["tier"] = 2
            elif city["is_capital"]: city["tier"] = 2
            else: city["tier"] = max(city.get("tier", 1), 1)

_assign_starting_buildings()
_get_road_connections()
_init_manpower()

# Rupert's Land starts as a puppet of Great Britain (becomes Canada in 1867, still a puppet)
if "Rupert's Land" in FACTIONS and "Great Britain" in FACTIONS:
    set_puppet("Rupert's Land", "Great Britain")

# === Newspaper Event Popup ===
_news_popup_img = None

def show_newspaper(title, description):
    """Show a newspaper popup that pauses the game. Player clicks X to close."""
    global game_speed, _news_popup_img
    old_speed = game_speed; game_speed = 0
    if _news_popup_img is None:
        _news_popup_img = pygame.image.load("images/news.png").convert_alpha()
    title_font = pygame.font.SysFont(None, 56)
    desc_font = pygame.font.SysFont(None, 36)
    W, H = screen.get_size()
    # Capture and blur the current screen
    snapshot = screen.copy()
    small = pygame.transform.smoothscale(snapshot, (W//8, H//8))
    blurred = pygame.transform.smoothscale(small, (W, H))
    # Scale newspaper image 2x bigger
    nw, nh = 800, 600
    paper = pygame.transform.scale(_news_popup_img, (nw, nh))
    px, py = (W - nw) // 2, (H - nh) // 2
    # X button position
    x_btn = pygame.Rect(px + nw - 55, py + 15, 40, 40)
    while True:
        for event in pygame.event.get():
            if event.type == pygame.QUIT: pygame.quit(); sys.exit()
            if event.type == pygame.KEYDOWN:
                if event.key in (pygame.K_ESCAPE, pygame.K_RETURN, pygame.K_SPACE): game_speed = old_speed; return
            if event.type == pygame.MOUSEBUTTONDOWN and event.button == 1:
                if x_btn.collidepoint(event.pos): game_speed = old_speed; return
        # Draw blurred background
        screen.blit(blurred, (0, 0))
        # Draw newspaper
        screen.blit(paper, (px, py))
        # X button
        pygame.draw.rect(screen, (180, 40, 40), x_btn, border_radius=4)
        x_text = desc_font.render("X", True, (255, 255, 255))
        screen.blit(x_text, (x_btn.centerx - x_text.get_width()//2, x_btn.centery - x_text.get_height()//2))
        # Title text centered
        title_surf = title_font.render(title, True, (30, 30, 30))
        screen.blit(title_surf, (px + nw//2 - title_surf.get_width()//2, py + 160))
        # Description text (wrap if needed)
        words = description.split(); lines = []; current = ""
        for word in words:
            test = current + " " + word if current else word
            if desc_font.size(test)[0] < nw - 120: current = test
            else: lines.append(current); current = word
        if current: lines.append(current)
        for i, line in enumerate(lines):
            line_surf = desc_font.render(line, True, (40, 40, 40))
            screen.blit(line_surf, (px + nw//2 - line_surf.get_width()//2, py + 250 + i*36))
        pygame.display.update(); clock.tick(60)

def show_pause_menu():
    """Show pause menu overlay with blurred game background. Returns True to continue, False to quit."""
    btn_font = pygame.font.SysFont(None, 42)
    # Capture and blur the current screen
    W, H = screen.get_size()
    snapshot = screen.copy()
    # Blur by scaling down then back up
    small = pygame.transform.smoothscale(snapshot, (W//8, H//8))
    blurred = pygame.transform.smoothscale(small, (W, H))
    while True:
        for event in pygame.event.get():
            if event.type == pygame.QUIT: pygame.quit(); sys.exit()
            if event.type == pygame.KEYDOWN:
                if event.key == pygame.K_ESCAPE: return True  # resume game
                if event.key == pygame.K_F11: toggle_fullscreen()
            if event.type == pygame.MOUSEBUTTONDOWN and event.button == 1:
                mx, my = event.pos; W, H = screen.get_size()
                # Resume button
                resume_rect = pygame.Rect(W//2-100, H//2-80, 200, 50)
                if resume_rect.collidepoint(mx, my): return True
                # Main Menu button
                menu_rect = pygame.Rect(W//2-100, H//2-10, 200, 50)
                if menu_rect.collidepoint(mx, my): os.execv(sys.executable, [sys.executable] + sys.argv)
                # Exit button
                exit_rect = pygame.Rect(W//2-100, H//2+60, 200, 50)
                if exit_rect.collidepoint(mx, my): return False
        W, H = screen.get_size()
        # Draw blurred background
        screen.blit(blurred, (0, 0))
        # Slight dark tint
        overlay = pygame.Surface((W, H), pygame.SRCALPHA); overlay.fill((0, 0, 0, 80))
        screen.blit(overlay, (0, 0))
        # Paused title
        pause_text = btn_font.render("PAUSED", True, (255, 220, 100))
        screen.blit(pause_text, (W//2 - pause_text.get_width()//2, H//2 - 110))
        # Resume button
        resume_rect = pygame.Rect(W//2-100, H//2-80, 200, 50)
        _draw_btn_frame(resume_rect)
        resume_text = btn_font.render("Resume", True, (255, 255, 255))
        screen.blit(resume_text, (W//2 - resume_text.get_width()//2, H//2 - 80 + 25 - resume_text.get_height()//2))
        # Main Menu button
        menu_rect = pygame.Rect(W//2-100, H//2-10, 200, 50)
        _draw_btn_frame(menu_rect)
        menu_text = btn_font.render("Main Menu", True, (255, 255, 255))
        screen.blit(menu_text, (W//2 - menu_text.get_width()//2, H//2 - 10 + 25 - menu_text.get_height()//2))
        # Exit button
        exit_rect = pygame.Rect(W//2-100, H//2+60, 200, 50)
        _draw_btn_frame(exit_rect)
        exit_text = btn_font.render("Exit", True, (255, 255, 255))
        screen.blit(exit_text, (W//2 - exit_text.get_width()//2, H//2 + 60 + 25 - exit_text.get_height()//2))
        pygame.display.update(); clock.tick(60)

running = True
while running:
    for event in pygame.event.get():
        if event.type == pygame.QUIT: running = False
        elif event.type == pygame.KEYDOWN:
            if event.key == pygame.K_F11: toggle_fullscreen()
            elif event.key == pygame.K_ESCAPE:
                game_speed = 0  # pause the game
                if not show_pause_menu(): running = False
        elif event.type == pygame.MOUSEBUTTONDOWN and event.button in (1, 2):
            if event.button == 1 and _news_btn.collidepoint(event.pos): news_panel_open = not news_panel_open
            elif event.button == 1:
                clicked_speed = False
                for rect, spd_val in _speed_btn_rects:
                    if rect.collidepoint(event.pos): game_speed = spd_val; clicked_speed = True; break
                if not clicked_speed:
                    # Check laws panel clicks
                    if _handle_laws_click(*event.pos):
                        pass
                    # Check battle click (fighting units)
                    elif _check_battle_click(*event.pos):
                        pass
                    else:
                        # Check if clicked on a city
                        _click_mx, _click_my = event.pos
                        city_clicked = None
                        for c in cities:
                            sx, sy = world_to_screen(c["x"], c["y"])
                            if math.hypot(_click_mx-sx, _click_my-sy) < max(5, int(7*zoom)):
                                city_clicked = c; break
                        if city_clicked:
                            _selected_city = city_clicked if _selected_city is not city_clicked else None
                            _selected_region = -1
                        else:
                            _selected_city = None
                            # Select the territory/region under the cursor for its info panel.
                            _wx = (_click_mx - cam_x) / zoom
                            _wy = (_click_my - cam_y) / zoom
                            _rid = _region_at(_wx, _wy)
                            _selected_region = _rid if _rid != _selected_region else -1
                            panning = True; pan_start = event.pos; cam_start = (cam_x, cam_y)
            else: panning = True; pan_start = event.pos; cam_start = (cam_x, cam_y)
        elif event.type == pygame.MOUSEBUTTONUP and event.button in (1, 2): panning = False
        elif event.type == pygame.MOUSEWHEEL:
            W, H = get_screen_size(); mx, my = pygame.mouse.get_pos()
            if event.y > 0:
                if mx > W-426: news_scroll += 1
                else: old_zoom = zoom; zoom = min(ZOOM_MAX, zoom*1.1); cam_x = int(mx-(mx-cam_x)*zoom/old_zoom); cam_y = int(my-(my-cam_y)*zoom/old_zoom); clamp_camera()
            elif event.y < 0:
                if mx > W-426: news_scroll = max(0, news_scroll-1)
                else: old_zoom = zoom; zoom = max(ZOOM_MIN, zoom/1.1); cam_x = int(mx-(mx-cam_x)*zoom/old_zoom); cam_y = int(my-(my-cam_y)*zoom/old_zoom); clamp_camera()
    if panning: mx, my = pygame.mouse.get_pos(); cam_x = cam_start[0]+mx-pan_start[0]; cam_y = cam_start[1]+my-pan_start[1]; clamp_camera()
    keys = pygame.key.get_pressed(); moved = False
    if keys[pygame.K_LEFT] or keys[pygame.K_a]: cam_x += PAN_SPEED; moved = True
    if keys[pygame.K_RIGHT] or keys[pygame.K_d]: cam_x -= PAN_SPEED; moved = True
    if keys[pygame.K_UP] or keys[pygame.K_w]: cam_y += PAN_SPEED; moved = True
    if keys[pygame.K_DOWN] or keys[pygame.K_s]: cam_y -= PAN_SPEED; moved = True
    if moved: clamp_camera()
    screen.fill((10,10,30)); draw_map(); _draw_regions()
    season = get_season()
    if season != "Spring":
        W, H = get_screen_size(); overlay = pygame.Surface((W, H), pygame.SRCALPHA)
        if season == "Winter": overlay.fill((220,230,255,25))
        elif season == "Autumn": overlay.fill((255,140,40,18))
        elif season == "Summer": overlay.fill((255,255,180,12))
        screen.blit(overlay, (0, 0))
    _explosion_tick += 1; _brit_anim_tick += 1; sprites.anim_tick = _brit_anim_tick; draw_factions()
    day_timer += game_speed
    if day_timer >= DAY_TICKS: day_timer = 0; advance_date(); mark_claims_dirty()
    spawn_timer += game_speed
    if spawn_timer > DAY_TICKS * 5:
        spawn_timer = 0
        for city in cities:
            if city["owner"] in eliminated_factions: continue
            # Tick down spawn cooldown
            if city.get("spawn_cd", 0) > 0: city["spawn_cd"] -= game_speed; continue
            faction = city["owner"]; mats = faction_materials[faction]
            ucount = sum(1 for u in units if u.owner["owner"]==faction); ucap = _get_unit_cap(faction)
            at_war_now = any(at_war(faction, f) for f in active_factions() if f != faction)
            fill_ratio = ucount/max(1, ucap)
            focus = _get_faction_focus(faction)
            if focus == "Aggressive":
                if at_war_now: w_mil = 0.75 if fill_ratio < 0.8 else 0.4; w_mer = 0.1
                else: w_mil = 0.5 if fill_ratio < 0.7 else 0.2; w_mer = 0.2
            elif focus == "Economic":
                if at_war_now: w_mil = 0.3 if fill_ratio < 0.5 else 0.1; w_mer = 0.5
                else: w_mil = 0.1 if fill_ratio < 0.3 else 0.02; w_mer = 0.6
            else:  # Neutral
                if at_war_now: w_mil = 0.6 if fill_ratio < 0.7 else 0.2; w_mer = 0.25
                else: w_mil = 0.2 if fill_ratio < 0.5 else 0.05; w_mer = 0.4
            roll = random.random()
            if roll < w_mil:
                if ucount >= ucap and not city.get("is_fort"): continue
                enemies = [c for c in cities if c["owner"]!=faction and not at_peace(faction, c["owner"]) and math.hypot(c["x"]-city["x"], c["y"]-city["y"])<=(NATIVE_MAX_RANGE if faction in NATIVE_FACTIONS else UNIT_MAX_RANGE)]
                # Forts only spawn if enemies are in proximity (bypass cap)
                if city.get("is_fort"):
                    nearby_enemies = [c for c in cities if c["owner"]!=faction and not at_peace(faction, c["owner"]) and math.hypot(c["x"]-city["x"], c["y"]-city["y"])<=150]
                    nearby_enemy_units = [u for u in units if u.owner["owner"]!=faction and not at_peace(faction, u.owner["owner"]) and math.hypot(u.x-city["x"], u.y-city["y"])<=150]
                    if not nearby_enemies and not nearby_enemy_units: continue
                if enemies:
                    fm = _pick_formation(faction, city)
                    cost = _unit_cost(faction, fm)
                    if can_afford(faction, cost) and _can_sustain_new_unit(faction, fm) and _faction_manpower.get(faction, 0) >= 150:
                        pay(faction, cost); tgt = random.choice(enemies); units.append(Unit(city, tgt, fm))
                        key = pair(faction, tgt["owner"]); war_pressure[key] = war_pressure.get(key, 0)+1
                        city["spawn_cd"] = DAY_TICKS*90 if city.get("is_fort") else (DAY_TICKS*60 if city.get("is_village") else (DAY_TICKS*30 if city["is_capital"] else DAY_TICKS*45))
                        if _city_has_building(city, "Barracks"): city["spawn_cd"] = int(city["spawn_cd"] * 0.7)
                else:
                    friendly = [c for c in cities if c["owner"]==faction and c is not city and math.hypot(c["x"]-city["x"], c["y"]-city["y"]) < 120 and not _is_water((city["x"]+c["x"])/2, (city["y"]+c["y"])/2)]
                    fm = _pick_formation(faction, city)
                    cost = _unit_cost(faction, fm)
                    if can_afford(faction, cost) and ucount < ucap and _can_sustain_new_unit(faction, fm):
                        pay(faction, cost)
                        if friendly: u = Unit(city, random.choice(friendly), fm); u.patrolling = True
                        else: u = Unit(city, city, fm); u.idle = True
                        units.append(u)
                        city["spawn_cd"] = DAY_TICKS*90 if city.get("is_fort") else (DAY_TICKS*60 if city.get("is_village") else (DAY_TICKS*30 if city["is_capital"] else DAY_TICKS*45))
            elif roll < w_mil + w_mer:
                _mcost = config.MERCHANT_COST
                if faction in NATIVE_FACTIONS:
                    can_spawn = mats["Food"] >= 10 and mats["Hide"] >= 5
                else:
                    can_spawn = can_afford(faction, _mcost)
                if can_spawn and sum(1 for m in merchants if m.owner_faction==faction) < _get_merchant_cap(faction) and _can_sustain_new_merchant(faction):
                    others = [c for c in cities if c is not city and not at_war(faction, c["owner"]) and math.hypot(c["x"]-city["x"], c["y"]-city["y"])<=MERCHANT_MAX_TRAVEL]
                    if others:
                        if faction in NATIVE_FACTIONS: mats["Food"] -= 10; mats["Hide"] -= 5
                        else: pay(faction, _mcost)
                        merchants.append(Merchant(city, random.choice(others)))
    material_timer += game_speed
    if material_timer >= DAY_TICKS: material_timer = 0; update_gold_tax(); update_city_regen(); update_floods(); [_clamp_resources(f) for f in active_factions()]
    if game_speed > 0: update_materials(); [_clamp_resources(f) for f in active_factions()]
    if game_speed > 0: update_passive_city_regen()
    upkeep_timer += game_speed
    if upkeep_timer >= DAY_TICKS:
        upkeep_timer = 0
        update_upkeep(); update_unit_upgrades(); update_city_upgrades(); update_construction(); update_faction_focuses()
        # AI building construction — check idle cities once per day (was every frame: too heavy)
        for city in cities:
            if city["owner"] not in eliminated_factions and not city.get("construction"):
                _ai_try_build(city)
    merchant_timer += game_speed
    if merchant_timer >= DAY_TICKS * 10:
        merchant_timer = 0
        for city in cities:
            if random.random() < 0.05:
                owner = city["owner"]
                if sum(1 for m in merchants if m.owner_faction==owner) >= _get_merchant_cap(owner): continue
                can_spawn = (faction_materials[owner]["Food"]>=10 and faction_materials[owner]["Hide"]>=5) if owner in NATIVE_FACTIONS else can_afford(owner, config.MERCHANT_COST)
                if can_spawn:
                    others = [c for c in cities if c is not city and not at_war(owner, c["owner"]) and math.hypot(c["x"]-city["x"], c["y"]-city["y"])<=MERCHANT_MAX_TRAVEL]
                    if others:
                        if owner in NATIVE_FACTIONS: faction_materials[owner]["Food"] -= 10; faction_materials[owner]["Hide"] -= 5
                        else: pay(owner, config.MERCHANT_COST)
                        merchants.append(Merchant(city, random.choice(others)))
    settler_timer += game_speed
    if settler_timer >= DAY_TICKS * 90:
        settler_timer = 0
        for city in cities:
            if random.random() < 0.03:
                owner = city["owner"]
                if owner in NATIVE_FACTIONS:
                    # Native settler cost: 40 Food + 20 Hide + 10 Lumber
                    camp_count = sum(1 for c in cities if c["owner"]==owner and c.get("is_camp"))
                    if camp_count < 3:
                        if faction_materials[owner]["Food"] >= 40 and faction_materials[owner]["Hide"] >= 20 and faction_materials[owner]["Lumber"] >= 10:
                            faction_materials[owner]["Food"] -= 40; faction_materials[owner]["Hide"] -= 20; faction_materials[owner]["Lumber"] -= 10
                            settlers.append(Settler(owner, city["x"], city["y"]))
                else:
                    # Colonial settler cost: 30 Gold + 50 Food + 40 Lumber + 5 Iron
                    _settler_cost = {"Gold": 30, "Food": 50, "Lumber": 40, "Iron": 5}
                    if can_afford(owner, _settler_cost):
                        if sum(1 for c in cities if c["owner"]==owner and c.get("is_fort")) < FORT_CAP:
                            pay(owner, _settler_cost); settlers.append(Settler(owner, city["x"], city["y"]))
    treaty_timer += game_speed
    if treaty_timer >= DAY_TICKS * 5: treaty_timer = 0; check_stalemate_treaties(); check_forced_treaties(); check_alliance_opportunities(); check_war_fatigue()
    war_timer += game_speed
    if war_timer >= DAY_TICKS * 10: war_timer = 0; check_territorial_demands(); check_war_declarations()
    disaster_timer += game_speed
    if disaster_timer >= DAY_TICKS * 10: disaster_timer = 0; check_disasters(); _check_pirate_spawn()
    update_treaties(); update_alliances(); check_eliminations()
    # Enforce: each faction can only have one capital
    for f in active_factions():
        caps = [c for c in cities if c["owner"]==f and c.get("is_capital")]
        if len(caps) > 1:
            for c in caps[1:]: c["is_capital"] = False
        elif len(caps) == 0:
            owned = [c for c in cities if c["owner"]==f]
            if owned: random.choice(owned)["is_capital"] = True
    if game_speed > 0:
        for u in units[:]: u.update()
        check_battles()
        for m in merchants[:]: m.update()
        check_merchant_encounters()
        for s in settlers[:]: s.update()
    _draw_mountains(); _draw_mississippi(); _draw_rio_grande(); _draw_roads()
    for city in cities: _draw_city_plantations(city)
    for city in cities: _draw_city_farm(city)
    for city in cities: draw_city(city)
    for city in cities: _draw_city_smoke(city)
    draw_city_tooltip()
    for u in units: u.draw()
    for m in merchants: m.draw()
    for s in settlers: s.draw()
    draw_unit_tooltip()
    draw_city_panel()
    if _selected_city: _draw_governor_portrait(_selected_city)
    if _selected_city: _draw_demographics(_selected_city)
    _highlight_selected_region(); _draw_region_panel()
    _draw_battle_viewer()
    _draw_siege_viewer()
    _news_btn = draw_news(); _factions_btn = draw_factions(); draw_date()
    pygame.display.update(); clock.tick(60)
pygame.quit()
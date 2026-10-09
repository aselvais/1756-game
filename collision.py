"""
collision.py — Collision detection and A* pathfinding system.
Uses collisionmap.png: Red = land, Blue = water.
Units/merchants/settlers can only walk on RED (land).
Ships can only sail on BLUE (water).
"""

import pygame
import math
import heapq

# === Map Constants ===
_COORD_SCALE = 3
MAP_W, MAP_H = 533 * _COORD_SCALE, int(533 * (7717 / 5595)) * _COORD_SCALE  # enlarged 3x, South America included

# === City/Port Identifiers (game rules, not collision) ===
ISLAND_CITY_NAMES = {"Nassau", "George Town", "Saint-Pierre", "Tadoussac", "Port-au-Prince", "Kingston", "Havana", "Plaisance"}
TINY_ISLAND_NAMES = {"Nassau", "George Town", "Saint-Pierre", "Port-au-Prince", "Kingston", "Havana"}
PORT_CITIES = {
    "Merida", "San Lorenzo", "Port-au-Prince", "Saint-Pierre", "Sitka",
    "George Town", "Kingston", "Boston", "Caracas", "San Agustin", "Havana",
    "Monterrey", "Williamsburg", "New York", "New Orleans", "Nassau",
    "Plaisance", "Charleston"
}


def is_port(city):
    """Check if a city dict has a port."""
    return city["name"] in PORT_CITIES


# === Collision Map ===
_collision_map = None


def init(collision_surface=None):
    """Initialize the collision system. Pass an already-loaded surface or load from file."""
    global _collision_map
    if collision_surface is not None:
        _collision_map = collision_surface
    else:
        _collision_map = pygame.transform.scale(
            pygame.image.load("images/collisonmap.png"), (MAP_W, MAP_H)
        )
    build_nav_grid()


def is_water(wx, wy):
    """Strictly check collision map. Blue = water, Red = land. No overrides."""
    ix, iy = int(wx), int(wy)
    if ix < 0 or ix >= MAP_W or iy < 0 or iy >= MAP_H:
        return True
    r, g, b = _collision_map.get_at((ix, iy))[:3]
    return b > r


def is_land(wx, wy):
    """Inverse of is_water — True if red/land on collision map."""
    return not is_water(wx, wy)


def path_needs_boat(x1, y1, x2, y2):
    """Sample the path between two points — returns True if water blocks the direct route."""
    dist = math.hypot(x2 - x1, y2 - y1)
    if dist < 10:
        return False
    steps = min(10, max(3, int(dist / 20)))
    water_hits = 0
    for i in range(1, steps):
        t = i / steps
        sx = x1 + (x2 - x1) * t
        sy = y1 + (y2 - y1) * t
        if is_water(sx, sy):
            water_hits += 1
    return water_hits >= 2


# === Navigation Grid ===
NAV_CELL = 10
NAV_W = MAP_W // NAV_CELL  # 53
NAV_H = MAP_H // NAV_CELL  # 49
_nav_grid = None  # True = water cell, False = land cell


def build_nav_grid():
    """Build navigation grid strictly from collision map. True = water cell."""
    global _nav_grid
    _nav_grid = [[False] * NAV_H for _ in range(NAV_W)]
    for gx in range(NAV_W):
        for gy in range(NAV_H):
            wx = gx * NAV_CELL + NAV_CELL // 2
            wy = gy * NAV_CELL + NAV_CELL // 2
            _nav_grid[gx][gy] = is_water(wx, wy)


# === A* Pathfinding ===

def find_water_path(sx, sy, tx, ty):
    """A* pathfinding on water (for ships). Returns list of (world_x, world_y) waypoints or None."""
    sgx = max(0, min(NAV_W - 1, int(sx) // NAV_CELL))
    sgy = max(0, min(NAV_H - 1, int(sy) // NAV_CELL))
    tgx = max(0, min(NAV_W - 1, int(tx) // NAV_CELL))
    tgy = max(0, min(NAV_H - 1, int(ty) // NAV_CELL))
    if sgx == tgx and sgy == tgy:
        return [(tx, ty)]
    return _astar(sgx, sgy, tgx, tgy, water=True)


def find_land_path(sx, sy, tx, ty):
    """A* pathfinding on land (for units/merchants). Returns list of (world_x, world_y) waypoints or None."""
    sgx = max(0, min(NAV_W - 1, int(sx) // NAV_CELL))
    sgy = max(0, min(NAV_H - 1, int(sy) // NAV_CELL))
    tgx = max(0, min(NAV_W - 1, int(tx) // NAV_CELL))
    tgy = max(0, min(NAV_H - 1, int(ty) // NAV_CELL))
    if sgx == tgx and sgy == tgy:
        return [(tx, ty)]
    return _astar(sgx, sgy, tgx, tgy, water=False, max_nodes=500)


def _astar(sgx, sgy, tgx, tgy, water=True, max_nodes=None):
    """Core A* implementation. water=True navigates on water cells, False on land cells."""
    open_set = [(0, sgx, sgy)]
    came_from = {}
    g_score = {(sgx, sgy): 0}
    closed = set()

    while open_set:
        _, cx, cy = heapq.heappop(open_set)
        if (cx, cy) in closed:
            continue
        closed.add((cx, cy))

        if max_nodes and len(closed) > max_nodes:
            return None

        if cx == tgx and cy == tgy:
            # Reconstruct path
            path = []
            node = (tgx, tgy)
            while node in came_from:
                path.append((node[0] * NAV_CELL + NAV_CELL // 2, node[1] * NAV_CELL + NAV_CELL // 2))
                node = came_from[node]
            path.reverse()
            if not path:
                return [(tgx * NAV_CELL + NAV_CELL // 2, tgy * NAV_CELL + NAV_CELL // 2)]
            # Simplify: remove collinear waypoints
            if len(path) > 2:
                simplified = [path[0]]
                for i in range(1, len(path) - 1):
                    dx1 = path[i][0] - path[i-1][0]
                    dy1 = path[i][1] - path[i-1][1]
                    dx2 = path[i+1][0] - path[i][0]
                    dy2 = path[i+1][1] - path[i][1]
                    if dx1 != dx2 or dy1 != dy2:
                        simplified.append(path[i])
                simplified.append(path[-1])
                path = simplified
            return path

        # Expand 8-directional neighbors
        for dx, dy in [(-1,0),(1,0),(0,-1),(0,1),(-1,-1),(1,-1),(-1,1),(1,1)]:
            nx, ny = cx + dx, cy + dy
            if 0 <= nx < NAV_W and 0 <= ny < NAV_H and (nx, ny) not in closed:
                # Check if cell is navigable
                cell_is_water = _nav_grid[nx][ny]
                navigable = cell_is_water if water else (not cell_is_water)
                # Always allow target cell
                if navigable or (nx == tgx and ny == tgy):
                    move_cost = 1.414 if (dx != 0 and dy != 0) else 1.0
                    new_g = g_score[(cx, cy)] + move_cost
                    if new_g < g_score.get((nx, ny), 9999):
                        g_score[(nx, ny)] = new_g
                        h = abs(nx - tgx) + abs(ny - tgy)
                        heapq.heappush(open_set, (new_g + h, nx, ny))
                        came_from[(nx, ny)] = (cx, cy)

    return None  # no path found


# === Water Deflection (for units hitting water edges) ===

def deflect_from_water(x, y, dx, dy, spd, trace_side=1):
    """Wall-tracing deflection: trace along the edge of water until clear path to target."""
    base_ang = math.atan2(dy, dx)
    perp_primary = base_ang + (math.pi / 2 * trace_side)
    perp_secondary = base_ang - (math.pi / 2 * trace_side)

    # Check which perpendicular side has land
    px, py = x + math.cos(perp_primary) * spd * 2, y + math.sin(perp_primary) * spd * 2
    sx2, sy2 = x + math.cos(perp_secondary) * spd * 2, y + math.sin(perp_secondary) * spd * 2

    if not is_water(px, py):
        trace_ang = perp_primary
    elif not is_water(sx2, sy2):
        trace_ang = perp_secondary
    else:
        # Both sides water — try wider angles
        for angle_off in [0.3, -0.3, 0.6, -0.6, 0.9, -0.9, 1.2, -1.2, 1.5, -1.5, 2.0, -2.0, 2.5, -2.5, 3.14]:
            ang = base_ang + angle_off * trace_side
            nx, ny = x + math.cos(ang) * spd, y + math.sin(ang) * spd
            if not is_water(nx, ny):
                return math.cos(ang), math.sin(ang)
        return 0, 0

    # Blend between trace direction and target direction
    for blend in [0.0, 0.15, 0.3, 0.5, 0.7, 0.85, 1.0]:
        ang = trace_ang * (1 - blend) + base_ang * blend
        cx, cy = math.cos(ang), math.sin(ang)
        nx1, ny1 = x + cx * spd, y + cy * spd
        nx2, ny2 = x + cx * spd * 3, y + cy * spd * 3
        if not is_water(nx1, ny1) and not is_water(nx2, ny2):
            return cx, cy

    # Fallback: just use trace direction
    cx, cy = math.cos(trace_ang), math.sin(trace_ang)
    if not is_water(x + cx * spd, y + cy * spd):
        return cx, cy

    # Last resort: any non-water direction
    for angle_off in [0.4, -0.4, 0.8, -0.8, 1.2, -1.2, 1.6, -1.6, 2.0, -2.0, 2.5, -2.5, 3.14]:
        ang = base_ang + angle_off
        nx, ny = x + math.cos(ang) * spd, y + math.sin(ang) * spd
        if not is_water(nx, ny):
            return math.cos(ang), math.sin(ang)

    return 0, 0  # truly stuck

# 1762 — Grand Strategy Colonial Simulation

A pygame-based grand strategy game set in colonial North America (1762-1790s).

## Project Structure

```
1756-game/
├── index.py        Main game loop, entities, rendering, events (~2380 lines)
├── collision.py    Collision detection & A* pathfinding
├── sprites.py      Spritesheet loading for units, ships, merchants
├── config.py       Game constants, balance values (edit to rebalance)
├── game_state.py   Shared mutable state (entity lists, diplomacy data)
├── warfare.py      War scoring constants & system documentation
├── utilities.py    Regiment/settlement name generators
└── images/         All game assets (spritesheets, maps, flags)
```

## How Collision Works

- `images/collisionmap.jpg` — The collision map (1200x800)
  - **RED pixels** = Land (units/merchants/settlers walk here)
  - **BLUE pixels** = Water (ships sail here)
- Edit this image to change coastlines, add/remove islands, etc.
- `images/nasatelliteview.jpg` — Visual background only (no gameplay effect)

## Modules

### `config.py` — Balance & Constants
Edit this file to tweak: starting resources, unit costs, movement speeds,
disaster chances, treaty lengths, war thresholds, etc.

### `collision.py` — Pathfinding
- `is_water(x, y)` / `is_land(x, y)` — pixel-level terrain check
- `find_water_path(sx, sy, tx, ty)` — A* for ships (navigates water)
- `find_land_path(sx, sy, tx, ty)` — A* for units (navigates land)
- `deflect_from_water(x, y, dx, dy, spd, trace_side)` — wall-tracing for units at water edges
- `path_needs_boat(x1, y1, x2, y2)` — checks if route crosses water

### `sprites.py` — Asset Loading
- `init()` — loads all spritesheets
- `infantry_anims[faction]` — dict of animation frames per faction
- `ship_anims` — ship directional animations
- `merchant_anims` / `native_merchant_anims` — merchant sprites
- `get_explosion_frame(size)` — animated explosion

### `game_state.py` — Shared State
Central module for all mutable game state. Any module can import this to
access `cities`, `units`, `merchants`, `ships`, `wars`, etc.

### `warfare.py` — War System
Documents the war declaration AI scoring system.
War score constants (e.g. WAR_SCORE_TERRITORIAL_CLAIM = 30) control
how aggressively AI declares war.

## index.py Sections (in order)

1. **Camera & Display Utilities** — zoom, pan, fullscreen
2. **Faction Definitions & Elimination** — FACTIONS, check_eliminations
3. **Rendering Utilities** — draw_outlined_text
4. **Economy & Unit Definitions** — materials, costs, caps
5. **News System** — news panel
6. **Disasters & Weather** — hurricanes, floods, storms
7. **Pirates** — pirate spawning mechanics
8. **Warfare & Diplomacy** — war declarations, treaties, alliances
9. **City Definitions** — all starting cities/forts/villages
10. **Entity Classes** — Unit, Merchant, Settler, Ship
11. **Combat & Encounters** — battle resolution, ship combat
12. **Economy Updates** — material generation, upkeep, upgrades
13. **Factions Panel & UI** — faction display
14. **Terrain** — mountains, rivers, drawing
15. **City Rendering** — draw_city
16. **Date/Time & Events** — calendar, American Revolution
17. **Game Loop** — main event loop, rendering, updates

## How to Run

```
python index.py
```

Requires: pygame-ce (or pygame), Python 3.10+
Optional: Pillow (for animated explosion GIF)

## Factions

- **Spain** — Mexico City (capital), Havana, Merida, Caracas, etc.
- **France** — Quebec (capital), New Orleans, Port-au-Prince, etc.
- **Great Britain** — Boston (capital), New York, Charleston, Nassau, etc.
- **Russia** — Sitka (capital)
- **Iroquois** — Onondaga (native faction)
- **Wabanaki** — Norridgewock (native faction)
- **Comanche** — Comancheria (native faction, horsemen)
- **Pirates** — spawn randomly at Caribbean ports
- **United States** — spawns April 19, 1775 (American Revolution)

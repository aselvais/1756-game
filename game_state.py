"""
game_state.py — Shared mutable game state.
All entity classes and systems reference this module for global lists and variables.
This avoids circular imports and makes state accessible from any module.
"""

# === Entity Lists ===
cities = []
units = []
merchants = []
settlers = []
ships = []

# === Economy ===
faction_materials = {}  # {faction_name: {material: amount}}

# === Faction State ===
FACTIONS = []  # populated at init
eliminated_factions = set()
_surrendered = set()

# === War/Diplomacy ===
wars = set()
treaties = {}
treaty_cooldowns = {}
war_pressure = {}
alliances = {}
refused_demands = {}
war_fatigue = {}

# === Game Speed & Time ===
game_speed = 1
game_day = 1
game_month = 1
game_year = 1762

# === Camera/Display ===
zoom = 1.0
cam_x = 0
cam_y = -150

# === News ===
news_log = []
news_scroll = 0


def active_factions():
    """Return list of non-eliminated factions."""
    return [f for f in FACTIONS if f not in eliminated_factions]

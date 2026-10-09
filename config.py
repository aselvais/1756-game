"""
config.py — Game constants, faction definitions, economy costs, and balance values.
Central place to tweak game balance without touching logic code.
"""

# === Display ===
MAP_W, MAP_H = 533, 735
ZOOM_MIN, ZOOM_MAX = 1.0, 3.0
PAN_SPEED = 8

# === Time ===
DAY_TICKS = 12  # 5 days per second at 60fps

# === Factions ===
FACTION_COLORS = {
    "France": (0, 100, 255),
    "Spain": (255, 255, 0),
    "Great Britain": (255, 0, 0),
    "Russia": (0, 120, 0),
    "Denmark": (0, 255, 0),
    "Iroquois": (150, 50, 200),
    "Wabanaki": (128, 0, 0),
    "Comanche": (180, 120, 40),
    "Cree": (60, 180, 130),
    "Dakota": (180, 160, 80),
    "Pirates": (20, 20, 20),
}
NATIVE_FACTIONS = {"Iroquois", "Wabanaki", "Comanche", "Cree", "Dakota"}

# === Unit/Formation ===
STRATEGIES = ["Aggressive", "Defensive", "Balanced"]
FORMATIONS = ["Line", "Column", "Square"]
FORMATION_STATS = {
    "Line": {"speed": 0.12, "power": 5, "size": 5},
    "Column": {"speed": 0.18, "power": 3, "size": 4},
    "Square": {"speed": 0.25, "power": 8, "size": 7},
}
FORMATION_COSTS = {
    "Line": {"Gold": 15},
    "Column": {"Gold": 10},
    "Square": {"Gold": 25, "Hide": 5},
}

# === Merchant Cost ===
MERCHANT_COST = {"Gold": 40, "Food": 20}

# === Economy ===
MATERIALS = ["Lumber", "Hide", "Iron", "Coal", "Gold", "Food"]
CITY_MATERIALS = ["Lumber", "Hide", "Iron", "Coal"]
FORT_COST = {"Gold": 15, "Lumber": 12}
FORT_CAP = 3
SHIP_COST = {"Lumber": 15, "Iron": 5}
MERCHANT_CAP = 3
NATIVE_MERCHANT_CAP = 1

# Starting resources per faction
STARTING_RESOURCES = {
    "Gold": 80, "Food": 100, "Lumber": 50,
    "Hide": 25, "Iron": 8, "Coal": 4,
}

# === Combat/Movement ===
# Scaled 3x to match the enlarged map coordinate space
RETREAT_RADIUS = 36 * 3
RETREAT_CHANCE = 0.08
RETREAT_DIST = 27 * 3
UNIT_MAX_RANGE = 133 * 3
NATIVE_MAX_RANGE = 34 * 3
MERCHANT_MAX_TRAVEL = 133 * 3

# === Treaties/War ===
TREATY_MIN_TICKS = DAY_TICKS * 30
TREATY_MAX_TICKS = DAY_TICKS * 1825
TREATY_COOLDOWN_TICKS = DAY_TICKS * 180
STALEMATE_THRESHOLD_TICKS = DAY_TICKS * 60

# === Disasters ===
DISASTER_CHANCE = 0.001
SPRING_FLOOD_CHANCE = 0.003
SUMMER_STORM_CHANCE = 0.004

# === Visual ===
STAR_POINTS = [(0,-35),(10,-10),(35,-10),(15,5),(23,30),(0,18),(-23,30),(-15,5),(-35,-10),(-10,-10)]
NEWS_VISIBLE = 5

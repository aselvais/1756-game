"""
warfare.py — War, diplomacy, treaties, and alliances system documentation.

NOTE: The actual warfare functions are still in index.py because they deeply
reference global game state (cities, units, factions, news). This module
documents the warfare API and provides utility constants.

Functions in index.py that belong to this system:
- pair(a, b) -> frozenset
- at_war(a, b) -> bool
- at_peace(a, b) -> bool
- declare_war(a, b, reason)
- end_war(a, b)
- check_territorial_demands()
- check_war_declarations()
- resolve_treaty(a, b)
- make_treaty(a, b, reason)
- allied(a, b) -> bool
- form_alliance(a, b, reason)
- update_treaties()
- update_alliances()
- check_alliance_opportunities()
- check_stalemate_treaties()
- check_forced_treaties()
- _add_war_fatigue(faction, enemy)
- check_war_fatigue()

Data structures (in game_state.py):
- wars: set of frozenset pairs
- treaties: dict {frozenset: ticks_remaining}
- treaty_cooldowns: dict {frozenset: ticks_remaining}
- war_pressure: dict {frozenset: pressure_score}
- alliances: dict {frozenset: ticks_remaining}
- refused_demands: dict {frozenset: count}
- war_fatigue: dict {frozenset: {faction: losses}}
"""

# War declaration scoring weights (for AI decision making)
WAR_SCORE_TERRITORIAL_CLAIM = 30
WAR_SCORE_RESOURCE_SCARCITY = 25
WAR_SCORE_REVENGE = 35
WAR_SCORE_REFUSED_DEMAND = 25  # per refusal
WAR_SCORE_ENEMY_AT_WAR = 30
WAR_SCORE_HAS_ALLIES = 15
WAR_SCORE_MILITARY_ADVANTAGE = 20
WAR_SCORE_MILITARY_DISADVANTAGE = -20
WAR_SCORE_MUCH_WEAKER = -40
WAR_SCORE_VASTLY_WEAKER = -30
WAR_SCORE_LOW_RESOURCES = -15
WAR_SCORE_BORDER_THREAT = 15
WAR_THRESHOLD = 50  # score must exceed this to declare war
WAR_DECLARE_CHANCE = 0.008  # random chance per check when score > threshold

//! Balance constants carried over from `config.py`.

pub const COORD_SCALE: f32 = 3.0;
pub const MAP_W: u32 = 533 * 3;
pub const MAP_H: u32 = (533.0 * (7717.0 / 5595.0)) as u32 * 3;
pub const PAN_SPEED: f32 = 8.0;
pub const DAY_TICKS: i32 = 12;

pub const NATIVE: &[&str] = &["Iroquois", "Wabanaki", "Comanche", "Cree", "Dakota"];

#[allow(dead_code)]
pub const RETREAT_DIST: f32 = 27.0 * 3.0;
pub const UNIT_MAX_RANGE: f32 = 133.0 * 3.0;
pub const NATIVE_MAX_RANGE: f32 = 34.0 * 3.0;
pub const MERCHANT_MAX_TRAVEL: f32 = 133.0 * 3.0;

pub const TREATY_MIN: i32 = DAY_TICKS * 30;
pub const TREATY_MAX: i32 = DAY_TICKS * 1825;
pub const TREATY_COOLDOWN: i32 = DAY_TICKS * 180;
pub const STALEMATE_THRESHOLD: i32 = DAY_TICKS * 60;
pub const WAR_THRESHOLD: i32 = 50;
pub const WAR_DECLARE_CHANCE: f32 = 0.008;

pub const DISASTER_CHANCE: f32 = 0.001;
pub const FORT_CAP: i32 = 3;
pub const MERCHANT_CAP: i32 = 3;
pub const NATIVE_MERCHANT_CAP: i32 = 1;

pub const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];
pub const MONTH_DAYS: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

pub const ISLANDS: &[&str] = &[
    "Nassau", "George Town", "Saint-Pierre", "Tadoussac", "Port-au-Prince", "Kingston",
    "Havana", "Plaisance",
];

pub const PIRATE_TARGETS: &[&str] = &["Havana", "Nassau", "Kingston", "Port-au-Prince", "Caracas"];

pub fn is_native(faction: &str) -> bool {
    NATIVE.contains(&faction)
}

pub fn faction_color(name: &str) -> [u8; 3] {
    match name {
        "France" => [0, 100, 255],
        "Spain" => [255, 255, 0],
        "Great Britain" => [255, 0, 0],
        "Russia" => [0, 120, 0],
        "Denmark" => [0, 255, 0],
        "Iroquois" => [150, 50, 200],
        "Wabanaki" => [128, 0, 0],
        "Comanche" => [180, 120, 40],
        "Cree" => [60, 180, 130],
        "Dakota" => [180, 160, 80],
        "Pirates" => [20, 20, 20],
        "United States" => [100, 180, 255],
        "Mexico" => [0, 100, 50],
        "Haiti" => [0, 0, 150],
        "Texas" => [0, 50, 150],
        "Confederate States" => [150, 0, 0],
        "Rupert's Land" => [200, 120, 60],
        "Canada" => [200, 50, 50],
        _ => [180, 180, 180],
    }
}

/// (move, food, lumber, hide, gold, defense)
pub fn biome_mods(biome: u8) -> (f32, f32, f32, f32, f32, f32) {
    match biome {
        1 => (0.75, 0.75, 2.0, 2.0, 1.0, 1.25), // taiga
        2 => (0.60, 0.50, 0.0, 2.0, 1.0, 1.0),  // tundra
        3 => (1.25, 1.50, 0.50, 1.50, 1.0, 1.0), // grassland
        4 => (0.75, 0.50, 1.0, 1.25, 1.50, 1.0), // desert
        5 => (0.50, 1.25, 1.0, 1.0, 1.0, 1.50), // swamp
        6 => (0.50, 0.75, 1.50, 1.50, 1.0, 1.0), // tropical
        7 => (0.40, 0.40, 1.0, 1.50, 1.75, 2.0), // mountains
        8 => (0.45, 0.85, 2.0, 1.75, 1.0, 1.25), // rainforest
        _ => (1.0, 1.0, 1.0, 1.0, 1.0, 1.0),    // temperate
    }
}

pub fn biome_name(biome: u8) -> &'static str {
    match biome {
        1 => "taiga",
        2 => "tundra",
        3 => "grassland",
        4 => "desert",
        5 => "swamp",
        6 => "tropical",
        7 => "mountains",
        8 => "rainforest",
        _ => "temperate",
    }
}

#[derive(Clone, Copy)]
pub struct Building {
    pub name: &'static str,
    pub gold: i32,
    pub lumber: i32,
    pub hide: i32,
    pub iron: i32,
    pub coal: i32,
    pub food: i32,
    pub days: i32,
    pub colonial_only: bool,
    pub native_only: bool,
    pub tier_req: i32,
}

pub const BUILDINGS: &[Building] = &[
    Building { name: "Town Hall", gold: 200, lumber: 30, hide: 0, iron: 0, coal: 0, food: 0, days: 30, colonial_only: true, native_only: false, tier_req: 0 },
    Building { name: "Council Lodge", gold: 0, lumber: 20, hide: 10, iron: 0, coal: 0, food: 40, days: 30, colonial_only: false, native_only: true, tier_req: 0 },
    Building { name: "Road", gold: 150, lumber: 15, hide: 0, iron: 0, coal: 0, food: 0, days: 20, colonial_only: true, native_only: false, tier_req: 0 },
    Building { name: "Farm", gold: 80, lumber: 20, hide: 5, iron: 0, coal: 0, food: 0, days: 12, colonial_only: false, native_only: false, tier_req: 0 },
    Building { name: "Market", gold: 120, lumber: 20, hide: 0, iron: 0, coal: 0, food: 0, days: 15, colonial_only: false, native_only: false, tier_req: 1 },
    Building { name: "Warehouse", gold: 80, lumber: 30, hide: 0, iron: 3, coal: 0, food: 0, days: 20, colonial_only: false, native_only: false, tier_req: 1 },
    Building { name: "Lumber Mill", gold: 60, lumber: 15, hide: 0, iron: 2, coal: 0, food: 0, days: 10, colonial_only: false, native_only: false, tier_req: 1 },
    Building { name: "Stables", gold: 150, lumber: 25, hide: 10, iron: 0, coal: 0, food: 0, days: 20, colonial_only: true, native_only: false, tier_req: 1 },
    Building { name: "Barracks", gold: 180, lumber: 20, hide: 0, iron: 3, coal: 0, food: 0, days: 25, colonial_only: true, native_only: false, tier_req: 1 },
    Building { name: "Church", gold: 100, lumber: 20, hide: 0, iron: 0, coal: 0, food: 0, days: 15, colonial_only: true, native_only: false, tier_req: 1 },
    Building { name: "Plantation", gold: 300, lumber: 30, hide: 0, iron: 3, coal: 0, food: 0, days: 30, colonial_only: true, native_only: false, tier_req: 2 },
    Building { name: "Fur Trading Post", gold: 250, lumber: 25, hide: 0, iron: 0, coal: 0, food: 0, days: 25, colonial_only: false, native_only: false, tier_req: 2 },
    Building { name: "Shipyard", gold: 300, lumber: 40, hide: 0, iron: 5, coal: 0, food: 0, days: 35, colonial_only: true, native_only: false, tier_req: 2 },
    Building { name: "Foundry", gold: 250, lumber: 20, hide: 0, iron: 8, coal: 5, food: 0, days: 30, colonial_only: true, native_only: false, tier_req: 2 },
];

pub fn building(name: &str) -> Option<&'static Building> {
    BUILDINGS.iter().find(|b| b.name == name)
}

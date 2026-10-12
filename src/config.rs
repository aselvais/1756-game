//! Balance constants carried over from `config.py`.

pub const COORD_SCALE: f32 = 3.0;
pub const MAP_W: u32 = 533 * 3;
pub const MAP_H: u32 = (533.0 * (7717.0 / 5595.0)) as u32 * 3;
pub const PAN_SPEED: f32 = 8.0;
pub const DAY_TICKS: i32 = 12;

pub const NATIVE: &[&str] = &["Iroquois", "Wabanaki", "Cherokee", "Comanche", "Cree", "Dakota"];

/// Native confederations that hold and claim territory (colored on the map),
/// as opposed to the mobile tribes that keep only camps and villages.
pub const NATIVE_CONFEDERATION: &[&str] = &["Iroquois", "Wabanaki", "Cherokee"];

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

/// Whether a faction claims and colors map territory. Every non-native nation
/// does; among native nations only the settled confederations do, while the
/// mobile tribes (Comanche, Cree, Dakota) hold camps without claiming land.
pub fn claims_territory(faction: &str) -> bool {
    !is_native(faction) || NATIVE_CONFEDERATION.contains(&faction)
}

pub fn faction_color(name: &str) -> [u8; 3] {
    match name {
        "France" | "Louisiana" | "Saint-Domingue" => [0, 100, 255],
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => [255, 255, 0],
        "Portugal" => [0, 130, 110],
        "Netherlands" => [255, 130, 0],
        "Great Britain" => [255, 0, 0],
        "Russia" => [0, 120, 0],
        "Denmark" => [0, 255, 0],
        "Iroquois" => [150, 50, 200],
        "Wabanaki" => [128, 0, 0],
        "Cherokee" => [200, 150, 40],
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

// ---------------------------------------------------------------------------
// Laws
//
// Each nation holds one choice per law category. Categories list four options
// in order from most permissive/liberal to most extreme, each with a short
// effect description. Starting choices are set per nation and year to be
// roughly historically accurate.
// ---------------------------------------------------------------------------

/// One selectable option within a law category.
pub struct LawOption {
    pub name: &'static str,
    pub effect: &'static str,
}

/// A law category and its four ordered options.
pub struct LawCategory {
    pub name: &'static str,
    pub options: [LawOption; 4],
}

/// Returns the full set of law categories, each with four ordered options.
/// The index of a chosen option is what gets stored per nation in `Game.laws`.
pub fn law_categories() -> &'static [LawCategory] {
    const fn opt(name: &'static str, effect: &'static str) -> LawOption {
        LawOption { name, effect }
    }
    static CATEGORIES: &[LawCategory] = &[
        LawCategory {
            name: "Ethnic Laws",
            options: [
                opt("Protected Rights", "+15% happiness, -5% gold tax"),
                opt("Segregated", "+5% gold tax, -5% happiness for minorities"),
                opt("Suppressed", "+10% gold tax, -15% happiness, +5% troops"),
                opt("Enslaved", "+25% material output, -30% happiness, unrest risk"),
            ],
        },
        LawCategory {
            name: "Conscription Laws",
            options: [
                opt("Volunteer Service", "small army, +10% happiness"),
                opt("Selective Service", "+15% manpower"),
                opt("Mandatory Service", "+35% manpower, -5% happiness"),
                opt("Mass Mobilization", "+70% manpower, -15% happiness, -10% economy"),
            ],
        },
        LawCategory {
            name: "Religion Laws",
            options: [
                opt("Free Faith", "+15% happiness, -5% gold tax"),
                opt("Limited Faith", "+5% gold tax, -5% happiness for minorities"),
                opt("Suppressed", "+10% gold tax, -15% happiness, unrest risk"),
                opt("Banned", "+15% gold tax, -25% happiness, high unrest"),
            ],
        },
        LawCategory {
            name: "Taxation Laws",
            options: [
                opt("No Taxation", "no gold tax, +15% happiness"),
                opt("Low Taxation", "-50% gold tax, +8% happiness"),
                opt("Standard Taxation", "normal gold tax"),
                opt("Heavy Taxation", "+50% gold tax, -15% happiness"),
            ],
        },
    ];
    CATEGORIES
}

/// Historically plausible starting law choices for a nation at a given year,
/// one option index per category in `law_categories()` order:
/// [Ethnic Laws, Conscription Laws].
pub fn starting_laws(nation: &str, year: i32) -> Vec<usize> {
    // Ethnic Laws: 0 Protected, 1 Segregated, 2 Suppressed, 3 Enslaved.
    let ethnic = match nation {
        // Major slaveholding colonial empires and plantation economies.
        "Great Britain" | "France" | "Louisiana" | "Saint-Domingue" | "Spain" | "Portugal" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" | "Netherlands" => {
            if year >= 1835 { 2 } else { 3 }
        }
        "United States" => {
            if year >= 1858 { 2 } else { 3 }
        }
        "Confederate States" => 3,
        // Haiti abolished slavery at its founding.
        "Haiti" => 0,
        "Mexico" => {
            if year >= 1835 { 1 } else { 2 }
        }
        "Texas" => 3,
        // Northern European powers with little plantation slavery by this era.
        "Russia" | "Denmark" => 2,
        // Native nations: communal, no racialized ethnic law.
        "Iroquois" | "Wabanaki" | "Cherokee" | "Comanche" | "Cree" | "Dakota" => 0,
        "Rupert's Land" | "Canada" => 1,
        _ => 1,
    };
    // Conscription: 0 Volunteer, 1 Selective, 2 Mandatory, 3 Mass Mobilization.
    let conscription = match nation {
        // Napoleonic France pioneered mass conscription (the levee en masse).
        "France" if year >= 1793 && year < 1815 => 3,
        "France" => 2,
        "Russia" => 2, // long-service recruit levies
        "Great Britain" | "United States" | "Canada" | "Rupert's Land" => 0, // volunteer/militia tradition
        "Confederate States" => 2, // 1862 conscription acts
        "Mexico" | "Texas" | "Haiti" => 1,
        "Denmark" => 1,
        "Iroquois" | "Wabanaki" | "Cherokee" | "Comanche" | "Cree" | "Dakota" => 0,
        _ => 1,
    };
    // Religion: 0 Free Faith, 1 Limited Faith, 2 Suppressed, 3 Banned.
    let religion = match nation {
        "France" | "Louisiana" | "Saint-Domingue" | "Spain" | "Portugal" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => 3,       // Catholic empires enforced the one faith
        "Mexico" | "Russia" => 2,
        "Great Britain" | "United States" | "Canada" | "Rupert's Land" | "Confederate States" | "Texas" | "Denmark" | "Netherlands" => 1,
        "Haiti" => 0,
        "Iroquois" | "Wabanaki" | "Cherokee" | "Comanche" | "Cree" | "Dakota" => 0,
        _ => 1,
    };
    // Taxation: 0 None, 1 Low, 2 Standard, 3 Heavy.
    let taxation = match nation {
        // Mercantile empires taxed their colonies heavily.
        "Great Britain" | "Spain" | "France" | "Louisiana" | "Saint-Domingue" | "Portugal" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" | "Netherlands" => 3,
        "Russia" | "Denmark" => 2,
        // Young republics kept taxes low to win loyalty.
        "United States" | "Mexico" | "Haiti" | "Texas" => 1,
        "Confederate States" => 2,
        // Native nations have no formal taxation.
        "Iroquois" | "Wabanaki" | "Cherokee" | "Comanche" | "Cree" | "Dakota" => 0,
        _ => 2,
    };
    vec![ethnic, conscription, religion, taxation]
}

/// The dominant (ruling) ethnicity image-key for a nation. Used as the slow
/// drift target so a conquered settlement gradually sees the ruling nation's
/// people move in rather than changing overnight.
pub fn owner_ethnicity_key(nation: &str) -> &'static str {
    match nation {
        "Great Britain" | "Rupert's Land" | "Canada" => "british",
        "United States" | "Confederate States" | "Texas" => "americans",
        "France" | "Louisiana" | "Saint-Domingue" => "french",
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => "spanish",
        "Portugal" => "portugese",
        "Netherlands" => "dutch",
        "Mexico" => "latinamericans",
        "Haiti" => "africans",
        "Russia" => "russians",
        "Denmark" => "danes",
        _ => "indigenous",
    }
}

/// The established-faith image-key for a nation, used as the religion drift
/// target (missions, settlers, and converts spread the state religion slowly).
pub fn owner_religion_key(nation: &str) -> &'static str {
    match nation {
        "Great Britain" | "United States" | "Confederate States" | "Canada" | "Rupert's Land" | "Texas"
        | "Denmark" | "Netherlands" => "protestant",
        "France" | "Louisiana" | "Saint-Domingue" | "Spain" | "Portugal" | "Mexico" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => "catholic",
        "Russia" => "orthodox",
        "Haiti" => "voodoo",
        _ => "animism",
    }
}

// ---------------------------------------------------------------------------
// Government types
// ---------------------------------------------------------------------------

/// The government type of a nation at a given year, as (icon key, display
/// name). Icon keys match files in images/governments/.
pub fn government_of(nation: &str, year: i32) -> (&'static str, &'static str) {
    match nation {
        // Settled woodland confederacies that hold and claim territory.
        "Iroquois" | "Wabanaki" | "Cherokee" => ("confederation", "Confederation"),
        // Mobile tribal nations: camps and villages, but no claimed territory.
        "Comanche" | "Cree" | "Dakota" => ("tribal", "Tribal"),
        // Chartered colonies and dominions governed from a mother country.
        "Rupert's Land" | "Canada" => ("colonialgovernment", "Colonial Government"),
        // Britain: a constitutional monarchy throughout this era.
        "Great Britain" => ("constitutionalmonarchy", "Constitutional Monarchy"),
        // France: Bourbon absolutism, then Revolution/Republic, Napoleonic
        // empire, the July Monarchy, and the Second Empire.
        "France" => {
            if year >= 1852 {
                ("monarchy", "Monarchy")
            } else if year >= 1830 {
                ("constitutionalmonarchy", "Constitutional Monarchy")
            } else if year >= 1804 {
                ("monarchy", "Monarchy")
            } else if year >= 1792 {
                ("republic", "Republic")
            } else {
                ("monarchy", "Monarchy")
            }
        }
        // Absolute monarchies.
        "Spain" | "Portugal" | "Netherlands" | "Russia" | "Denmark" => ("monarchy", "Monarchy"),
        // Spanish viceroyalties: governed from Madrid by an appointed viceroy,
        // not sovereign monarchies of their own.
        "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" | "Louisiana" | "Saint-Domingue" => ("colonialgovernment", "Colonial Government"),
        // Republics.
        "United States" | "Texas" => ("republic", "Republic"),
        // The Confederacy: a confederation of states.
        "Confederate States" => ("confederation", "Confederation"),
        // Latin American republics, often under caudillo military rule.
        "Mexico" => {
            if (1835..1846).contains(&year) {
                ("militarygovernment", "Military Government")
            } else {
                ("republic", "Republic")
            }
        }
        "Haiti" => {
            if year >= 1849 && year < 1859 {
                ("monarchy", "Monarchy") // Faustin I's empire
            } else {
                ("republic", "Republic")
            }
        }
        _ => ("colonialgovernment", "Colonial Government"),
    }
}

/// Short description and gameplay effects for a government icon key. Effects
/// are placeholders for the AI/decision tuning to come.
pub fn government_info(key: &str) -> (&'static str, &'static str) {
    match key {
        "tribal" => ("Councils of elders and war chiefs.", "+ mobile warriors, - economy, raids over conquest"),
        "colonialgovernment" => ("Ruled on behalf of a distant crown.", "+ trade income, - autonomy, defers to overlord"),
        "constitutionalmonarchy" => ("A crown checked by parliament.", "+ stability, + trade, moderate expansion"),
        "monarchy" => ("Absolute rule by the sovereign.", "+ fast decisions, + army, - happiness"),
        "republic" => ("Elected government and senate.", "+ happiness, + economy, slower to war"),
        "confederation" => ("A league of sovereign states.", "+ defense, - central control, cautious"),
        "militarygovernment" => ("Rule by the army and caudillos.", "+ army, + aggression, - stability"),
        _ => ("", ""),
    }
}

// ---------------------------------------------------------------------------
// Politics: parties and elections
//
// Only elective governments (Republic, Constitutional Monarchy, Confederation)
// hold elections. Each nation has four parties. A settlement's enfranchised
// population favors parties based on local conditions; the national result is
// the electorate-weighted sum of its settlements.
// ---------------------------------------------------------------------------

/// Whether a government type holds party elections.
pub fn is_elective(gov_key: &str) -> bool {
    matches!(gov_key, "republic" | "constitutionalmonarchy" | "confederation")
}

/// The four parties for a nation (names in a fixed order). Index 0..3 maps to
/// the stance weights in `party_stance`.
pub fn parties_of(nation: &str) -> [&'static str; 4] {
    match nation {
        "United States" => ["Federalist", "Democrat", "Whig", "Republican"],
        "Confederate States" => ["Fire-Eater", "Cooperationist", "Planter", "Unionist"],
        "Great Britain" | "Canada" => ["Whig", "Tory", "Radical", "Loyalist"],
        "France" => ["Republican", "Legitimist", "Orleanist", "Bonapartist"],
        "Mexico" | "Haiti" | "Texas" => ["Liberal", "Conservative", "Federalist", "Nationalist"],
        // Native confederation councils. Mapped to the same four archetypes as
        // every other nation (see `party_stance`): 0 reform/accommodation,
        // 1 tradition, 2 trade, 3 war/sovereignty — but named for the real
        // factions in a woodland council rather than European party labels.
        "Iroquois" | "Wabanaki" | "Cherokee" => ["Accommodationists", "Traditionalists", "Trade Council", "War Society"],
        _ => ["Liberal", "Conservative", "Federalist", "Nationalist"],
    }
}

/// A party's leaning as weights applied to settlement conditions. Each tuple is
/// (tax_relief, rights, war_hawk, trade, tradition). Positive means the party
/// is favored when that condition is high; the scorer multiplies these by the
/// settlement's measured conditions.
///
/// Index matches `parties_of`. We key stances by a canonical archetype derived
/// from the party's position so per-nation names still behave sensibly:
/// 0 ~ Liberal/left, 1 ~ Conservative/right, 2 ~ Federalist/centrist-trade,
/// 3 ~ Nationalist.
pub fn party_stance(index: usize) -> (f32, f32, f32, f32, f32) {
    match index {
        // (tax_relief, rights, war_hawk, trade, tradition)
        0 => (0.9, 1.0, -0.3, 0.3, -0.6), // Liberal/left: low tax, rights, reform
        1 => (-0.2, -0.6, 0.2, 0.4, 1.0), // Conservative: tradition, order
        2 => (0.1, 0.3, 0.0, 1.0, 0.1),   // Federalist: trade and institutions
        _ => (0.0, -0.4, 1.0, 0.2, 0.6),  // Nationalist: war, identity
    }
}

/// Legal status rank (0 best .. 4 worst) of an ethnicity, keyed by its image
/// key, under a nation at a year. Mirrors the UI's `ethnic_group_status` but is
/// callable from the simulation. Used to decide who is enfranchised.
pub fn ethnic_status_rank(nation: &str, key: &str, year: i32) -> u8 {
    // Image key -> display label used by the match below.
    let ethnicity = match key {
        "americans" => "American",
        "british" => "British",
        "french" => "French",
        "spanish" => "Spanish",
        "latinamericans" => "Latin American",
        "africans" => "African",
        "indigenous" => "Indigenous",
        "danes" => "Scandinavian",
        "russians" => "Russian",
        "portugese" => "Portuguese",
        "dutch" => "Dutch",
        _ => "Other",
    };
    let ruling = match nation {
        "Great Britain" | "Rupert's Land" | "Canada" => "British",
        "United States" | "Confederate States" | "Texas" => "American",
        "France" | "Louisiana" | "Saint-Domingue" => "French",
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => "Spanish",
        "Portugal" => "Portuguese",
        "Netherlands" => "Dutch",
        "Mexico" => "Latin American",
        "Haiti" => "African",
        "Russia" => "Russian",
        "Denmark" => "Scandinavian",
        _ => "Indigenous",
    };
    if ethnicity == ruling {
        return 0;
    }
    if ethnicity == "African" {
        return match nation {
            "Haiti" => 0,
            "United States" | "Confederate States" | "Texas" => 4,
            "Great Britain" => if year >= 1835 { 2 } else { 4 },
            "France" | "Louisiana" | "Saint-Domingue" => if year >= 1794 && year < 1803 { 2 } else { 4 },
            "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => if year >= 1835 { 3 } else { 4 },
            "Portugal" => if year >= 1835 { 3 } else { 4 },
            // Dutch Suriname: plantation slavery, abolished only in 1863.
            "Netherlands" => 4,
            "Mexico" => 1,
            _ => 3,
        };
    }
    if ethnicity == "Indigenous" {
        return match nation {
            "United States" | "Confederate States" | "Texas" => 3,
            "Spain" | "Mexico" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => 2,
            "Portugal" => 2,
            "Russia" | "Denmark" => 3,
            _ => 3,
        };
    }
    match nation {
        "Spain" | "Portugal" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => 2,
        "United States" | "Great Britain" | "Canada" | "Netherlands" => 1,
        _ => 2,
    }
}

// ---------------------------------------------------------------------------
// UI font scale
//
// A global multiplier applied to every text size so a settings slider can
// scale the whole UI. Clamped to a range the fixed panel layouts can tolerate.
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicU32, Ordering};

static FONT_SCALE_BITS: AtomicU32 = AtomicU32::new(0x3f800000); // 1.0 as f32 bits

pub const FONT_SCALE_MIN: f32 = 0.8;
pub const FONT_SCALE_MAX: f32 = 1.3;

/// The current global font scale.
pub fn font_scale() -> f32 {
    f32::from_bits(FONT_SCALE_BITS.load(Ordering::Relaxed))
}

/// Sets the global font scale, clamped to the safe range.
pub fn set_font_scale(scale: f32) {
    let clamped = scale.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX);
    FONT_SCALE_BITS.store(clamped.to_bits(), Ordering::Relaxed);
}

/// Scales a base font size by the global font scale.
pub fn scaled_font(size: f32) -> f32 {
    size * font_scale()
}

// ---------------------------------------------------------------------------
// Sound volume
//
// Session-wide, like the font scale. 0 is mute. The simulation never reads
// this; the renderer applies it when it plays queued cues.
// ---------------------------------------------------------------------------

static SOUND_VOLUME_BITS: AtomicU32 = AtomicU32::new(0.7f32.to_bits());

pub const SOUND_VOLUME_MIN: f32 = 0.0;
pub const SOUND_VOLUME_MAX: f32 = 1.0;

/// Current effect volume, from mute to full.
pub fn sound_volume() -> f32 {
    f32::from_bits(SOUND_VOLUME_BITS.load(Ordering::Relaxed))
}

/// Sets the effect volume, clamped to mute..full.
pub fn set_sound_volume(volume: f32) {
    let clamped = volume.clamp(SOUND_VOLUME_MIN, SOUND_VOLUME_MAX);
    SOUND_VOLUME_BITS.store(clamped.to_bits(), Ordering::Relaxed);
}

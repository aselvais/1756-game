//! Game rules ported from `index.py`: time, economy, armies, trade, diplomacy,
//! disasters, and the historical scenario events.

use crate::config::*;
use crate::world::Maps;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Serde adapters for maps keyed by [`Pair`]. `Pair` wraps a `u64`, and JSON
/// object keys must be strings, so we round-trip these maps as a `Vec` of
/// `(u64, value)` entries. Used via `#[serde(with = "pair_map")]`.
mod pair_map {
    use super::Pair;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S, V>(map: &HashMap<Pair, V>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: Serialize,
    {
        let entries: Vec<(u64, &V)> = map.iter().map(|(k, v)| (k.0, v)).collect();
        entries.serialize(s)
    }

    pub fn deserialize<'de, D, V>(d: D) -> Result<HashMap<Pair, V>, D::Error>
    where
        D: Deserializer<'de>,
        V: Deserialize<'de>,
    {
        let entries: Vec<(u64, V)> = Vec::deserialize(d)?;
        Ok(entries.into_iter().map(|(k, v)| (Pair(k), v)).collect())
    }
}

/// Serde adapter for a `HashSet<Pair>`, serialized as a `Vec<u64>`.
mod pair_set {
    use super::Pair;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashSet;

    pub fn serialize<S>(set: &HashSet<Pair>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let ids: Vec<u64> = set.iter().map(|p| p.0).collect();
        ids.serialize(s)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<HashSet<Pair>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ids: Vec<u64> = Vec::deserialize(d)?;
        Ok(ids.into_iter().map(Pair).collect())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Pair(pub u64);

impl Pair {
    pub fn new(a: &str, b: &str) -> Self {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        Self(hash_pair(a, b))
    }
}

fn hash_pair(a: &str, b: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for byte in a.bytes().chain(std::iter::once(0)).chain(b.bytes()) {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Stock {
    pub gold: i32,
    pub food: i32,
    pub lumber: i32,
    pub hide: i32,
    pub iron: i32,
    pub coal: i32,
}

impl Stock {
    fn starting() -> Self {
        Self { gold: 80, food: 100, lumber: 50, hide: 25, iron: 8, coal: 4 }
    }
    fn zero() -> Self {
        Self { gold: 0, food: 0, lumber: 0, hide: 0, iron: 0, coal: 0 }
    }
    fn get(&self, name: &str) -> i32 {
        match name {
            "Gold" => self.gold,
            "Food" => self.food,
            "Lumber" => self.lumber,
            "Hide" => self.hide,
            "Iron" => self.iron,
            "Coal" => self.coal,
            _ => 0,
        }
    }
    fn add(&mut self, name: &str, n: i32) {
        let slot = match name {
            "Gold" => &mut self.gold,
            "Food" => &mut self.food,
            "Lumber" => &mut self.lumber,
            "Hide" => &mut self.hide,
            "Iron" => &mut self.iron,
            "Coal" => &mut self.coal,
            _ => return,
        };
        *slot = (*slot + n).max(0);
    }
    pub fn afford(&self, cost: &[( &str, i32)]) -> bool {
        cost.iter().all(|(n, a)| self.get(n) >= *a)
    }
    fn pay(&mut self, cost: &[(&str, i32)]) {
        for (n, a) in cost {
            self.add(n, -a);
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct City {
    pub id: u32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub owner: String,
    pub sovereign: String,
    pub troops: i32,
    pub tier: i32,
    pub is_capital: bool,
    pub is_village: bool,
    pub is_fort: bool,
    pub is_camp: bool,
    pub material: String,
    pub buildings: Vec<String>,
    pub construction: Option<(String, i32)>,
    pub spawn_cd: i32,
    pub happiness: i32,
    pub regen_cooldown: i32,
    pub population: i32,
    pub pop_acc: f32,
    pub region: i32,
    /// Stored demographics (group key -> share), seeded at scenario start and
    /// drifting slowly over time. Persist through conquest so capturing a city
    /// does not instantly erase its people.
    pub ethnicity: Vec<(String, f32)>,
    pub religion: Vec<(String, f32)>,
    /// Fraction of the settlement that is aristocracy (the rest are commoners).
    pub aristocracy: f32,
    /// Monotonic settle order. Scenario-start cities share 0; each settler-
    /// founded settlement gets an increasing value so the most recent settler
    /// in a region wins ownership of that region.
    pub founded_order: u64,
    /// The real-world year this settlement was founded (first permanent
    /// establishment on the site). Native camps/villages predate written
    /// records, so they carry an approximate traditional founding year. Shown
    /// in the city tooltip and panel; it does not affect simulation.
    pub founded_year: i32,
    /// The nation that originally founded/built this settlement. Its
    /// architecture (building sprites) follows the builder, not the current
    /// owner, so a conquered city keeps its original look (e.g. Quebec stays
    /// French under British rule). Set once at creation; never changed.
    pub builder: String,
}

fn default_behavior() -> String {
    "Attack".to_string()
}

fn neg_one() -> i32 {
    -1
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Unit {
    pub uid: u32,
    pub faction: String,
    pub target: u32,
    pub x: f32,
    pub y: f32,
    pub formation: String,
    pub speed: f32,
    pub power: i32,
    pub men: i32,
    pub max_men: i32,
    pub tier: i32,
    pub retreating: bool,
    pub retreat: Option<(f32, f32)>,
    pub idle: bool,
    pub patrolling: bool,
    pub opponent: Option<u32>,
    pub name: String,
    /// Standing order governing what the unit does when it has no enemy to
    /// fight: "Attack" (march on the target city), "Patrol" (cycle between the
    /// faction's cities), or "Guard" (hold a border region within a radius).
    /// Defaults to "Attack" for old save files.
    #[serde(default = "default_behavior")]
    pub behavior: String,
    /// Plot-to-plot route: the remaining region ids to pass through to reach
    /// the current destination. Recomputed when the destination region changes.
    #[serde(default)]
    pub path: Vec<i32>,
    /// Destination region the current `path` was computed for; -1 when none.
    #[serde(default = "neg_one")]
    pub path_goal: i32,
    /// For "Guard": the region the unit anchors on, and its world-space center
    /// and radius. The unit holds within `guard_radius` of the anchor point.
    #[serde(default = "neg_one")]
    pub guard_region: i32,
    #[serde(default)]
    pub guard_anchor: Option<(f32, f32)>,
    pub siege_timer: i32,
    pub siege_phase: u8,
    pub phase_timer: i32,
    pub battle_phase: u8,
    pub skirmish_ticks: i32,
    pub alive: bool,
    pub ship: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Merchant {
    pub faction: String,
    pub x: f32,
    pub y: f32,
    pub source: u32,
    pub dest: u32,
    pub material: String,
    /// 0 travelling, 1 trading, 2 resting
    pub status: u8,
    pub timer: i32,
    pub ship: bool,
    #[allow(dead_code)]
    pub hp: i32,
    pub alive: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settler {
    pub faction: String,
    pub x: f32,
    pub y: f32,
    pub tx: f32,
    pub ty: f32,
    pub native: bool,
    /// True while the settler is crossing water, drawn as a ship (or a canoe
    /// for native factions) and moving at sea speed.
    pub ship: bool,
    pub alive: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Game {
    pub day: i32,
    pub month: i32,
    pub year: i32,
    pub speed: i32,
    pub cities: Vec<City>,
    founded: Vec<(i32, City)>,
    pub units: Vec<Unit>,
    pub merchants: Vec<Merchant>,
    pub settlers: Vec<Settler>,
    pub factions: Vec<String>,
    pub eliminated: HashSet<String>,
    surrendered: HashSet<String>,
    materials: HashMap<String, Stock>,
    manpower: HashMap<String, i32>,
    focus: HashMap<String, String>,
    focus_timer: HashMap<String, i32>,
    #[serde(with = "pair_set")]
    wars: HashSet<Pair>,
    #[serde(with = "pair_map")]
    war_names: HashMap<Pair, (String, String)>,
    #[serde(with = "pair_map")]
    treaties: HashMap<Pair, i32>,
    #[serde(with = "pair_map")]
    treaty_cd: HashMap<Pair, i32>,
    #[serde(with = "pair_map")]
    pressure: HashMap<Pair, i32>,
    #[serde(with = "pair_map")]
    alliances: HashMap<Pair, i32>,
    #[serde(with = "pair_map")]
    refused: HashMap<Pair, i32>,
    #[serde(with = "pair_map")]
    fatigue: HashMap<Pair, HashMap<String, i32>>,
    puppets: HashMap<String, String>,
    pub news: VecDeque<String>,
    pub paper: Option<(String, String)>,
    /// Per-nation law choices: nation -> (law category index -> option index).
    pub laws: HashMap<String, Vec<usize>>,
    /// Per-nation ruling party index (into that nation's party list). Only set
    /// for elective governments.
    pub ruling_party: HashMap<String, usize>,
    /// Days of game time until the next national election.
    election_timer: i32,
    /// Authored starting territory ownership (region id -> country name) loaded
    /// from a scenario map painted in the editor. Empty when no map is loaded.
    /// Cities still win a region where one exists; authored claims color every
    /// other painted region. Persisted with saves (defaults to empty for old
    /// save files).
    #[serde(default)]
    region_claims: HashMap<i32, String>,
    pub claims_dirty: bool,
    next_id: u32,
    /// Monotonic counter stamped onto each settler-founded city so later
    /// settlers outrank earlier ones when claiming a shared region.
    settle_order: u64,
    next_uid: u32,
    day_timer: i32,
    spawn_timer: i32,
    material_timer: i32,
    merchant_timer: i32,
    settler_timer: i32,
    upkeep_timer: i32,
    treaty_timer: i32,
    war_timer: i32,
    disaster_timer: i32,
    food_timer: i32,
    lumber_timer: i32,
    hide_timer: i32,
    iron_timer: i32,
    regen_timer: i32,
    flooded: HashMap<u32, i32>,
    revolution_fired: bool,
    haiti_fired: bool,
    mexico_fired: bool,
    texas_fired: bool,
    confederate_fired: bool,
    canada_fired: bool,
    pirates_spawned: bool,
    pirate_cd: i32,
}

impl Game {
    pub fn new() -> Self {
        let mut g = Self {
            day: 1,
            month: 1,
            year: 1754,
            speed: 1,
            cities: Vec::new(),
            founded: Vec::new(),
            units: Vec::new(),
            merchants: Vec::new(),
            settlers: Vec::new(),
            factions: Vec::new(),
            eliminated: HashSet::new(),
            surrendered: HashSet::new(),
            materials: HashMap::new(),
            manpower: HashMap::new(),
            focus: HashMap::new(),
            focus_timer: HashMap::new(),
            wars: HashSet::new(),
            war_names: HashMap::new(),
            treaties: HashMap::new(),
            treaty_cd: HashMap::new(),
            pressure: HashMap::new(),
            alliances: HashMap::new(),
            refused: HashMap::new(),
            fatigue: HashMap::new(),
            puppets: HashMap::new(),
            news: VecDeque::new(),
            paper: None,
            laws: HashMap::new(),
            ruling_party: HashMap::new(),
            election_timer: 0,
            region_claims: HashMap::new(),
            claims_dirty: true,
            next_id: 1,
            settle_order: 0,
            next_uid: 1,
            day_timer: 0,
            spawn_timer: 0,
            material_timer: 0,
            merchant_timer: 0,
            settler_timer: 0,
            upkeep_timer: 0,
            treaty_timer: 0,
            war_timer: 0,
            disaster_timer: 0,
            food_timer: 0,
            lumber_timer: 0,
            hide_timer: 0,
            iron_timer: 0,
            regen_timer: 0,
            flooded: HashMap::new(),
            revolution_fired: false,
            haiti_fired: false,
            mexico_fired: false,
            texas_fired: false,
            confederate_fired: false,
            canada_fired: false,
            pirates_spawned: false,
            pirate_cd: 0,
        };
        g.build_cities();
        g.boot_factions();
        g
    }

    /// Serializes the full game state (dates, cities, units, merchants,
    /// settlers, nations/countries, their stockpiles and manpower, diplomacy,
    /// laws, politics, and all timers/flags) to `path` as JSON. Everything
    /// needed to resume a game lives in `Game`, so this is a complete snapshot.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Loads a game previously written by [`Game::save`]. The returned `Game`
    /// is fully self-contained; the caller only needs to refresh view-side
    /// caches (map overlay, farm/plantation decorations).
    #[allow(dead_code)]
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let mut game: Game = serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        game.claims_dirty = true;
        Ok(game)
    }

    /// Installs an authored starting-territory map: a list of
    /// (region id, country name) claims painted in the map editor. Only regions
    /// within `[0, region_count)` and countries that actually claim territory
    /// are kept. Marks claims dirty so the overlay rebuilds. Returns how many
    /// claims were applied.
    pub fn set_region_claims(&mut self, claims: &[(i32, String)], region_count: i32) -> usize {
        self.region_claims.clear();
        for (region, nation) in claims {
            if *region < 0 || *region >= region_count {
                continue;
            }
            if !claims_territory(nation) {
                continue;
            }
            self.region_claims.insert(*region, nation.clone());
        }
        self.claims_dirty = true;
        self.region_claims.len()
    }

    pub fn start_scenario(&mut self, year: i32, maps: &Maps) {
        self.year = year;
        self.day = 1;
        self.month = 1;
        self.units.clear();
        self.merchants.clear();
        self.settlers.clear();
        self.wars.clear();
        self.war_names.clear();
        self.treaties.clear();
        self.alliances.clear();
        self.pressure.clear();
        self.eliminated.clear();
        // Authored territory maps are per-scenario; the loader (App::begin)
        // reinstalls one for 1754 after this runs. Clear any stale claims so a
        // previous scenario's map does not bleed through.
        self.region_claims.clear();
        self.revolution_fired = year > 1775;
        self.haiti_fired = year >= 1804;
        self.mexico_fired = year >= 1810;
        self.texas_fired = year >= 1836;
        self.confederate_fired = year >= 1861;
        self.canada_fired = year >= 1867;
        match year {
            1790 => self.scenario_1790(),
            1809 => self.scenario_1809(),
            1835 => self.scenario_1835(),
            1858 => self.scenario_1858(),
            _ => {}
        }
        self.found_due();
        if self.year >= 1834 {
            for c in &mut self.cities {
                if c.name == "York" {
                    c.name = "Toronto".into();
                }
            }
        }
        if self.year >= 1790 && !self.eliminated.contains("United States") {
            for c in &mut self.cities {
                if c.owner == "United States" {
                    c.is_capital = c.name == "Washington";
                }
            }
        }
        self.apply_historical_tiers();
        self.init_laws();
        self.init_puppets();
        self.init_classes();
        for c in &mut self.cities {
            c.troops = max_troops(c);
            c.region = maps.region_at(c.x, c.y);
        }
        self.assign_buildings();
        self.init_population();
        self.init_manpower();
        self.run_elections();
        self.election_timer = DAY_TICKS * 365 * 4; // elections every 4 years
        self.claims_dirty = true;
        self.news(format!("Scenario {} begins.", self.year));
    }

    /// Sets each starting settlement's tier to roughly match its real
    /// historical scale at the scenario's start year. Tiers run 0 (hamlet)
    /// through 3 (major city). The table lists, per city, the tier at each
    /// scenario date `[1754, 1790, 1809, 1835, 1858]`. Cities not listed keep
    /// the tier they were created with. Villages, forts, and native camps are
    /// left untouched so they stay small.
    fn apply_historical_tiers(&mut self) {
        // Index of the current scenario year within the date columns below.
        let col = match self.year {
            y if y < 1790 => 0,
            y if y < 1809 => 1,
            y if y < 1835 => 2,
            y if y < 1858 => 3,
            _ => 4,
        };
        // (city, [1754, 1790, 1809, 1835, 1858])
        let table: &[(&str, [i32; 5])] = &[
            // British / American eastern seaboard.
            ("Boston", [1, 2, 2, 3, 3]),
            ("New York", [1, 2, 3, 3, 3]),
            ("Charleston", [1, 2, 2, 2, 2]),
            ("Richmond", [0, 1, 1, 2, 2]),
            ("Halifax", [0, 1, 1, 2, 2]),
            // French North America.
            ("Quebec", [1, 2, 2, 2, 2]),
            ("Montreal", [1, 1, 2, 2, 3]),
            ("New Orleans", [1, 1, 2, 2, 3]),
            ("Saint-Louis", [0, 0, 1, 2, 2]),
            // Spanish America — Mexico City was the largest city in the Americas.
            ("Mexico City", [2, 3, 3, 3, 3]),
            ("Havana", [2, 2, 2, 3, 3]),
            ("Oaxaca", [1, 1, 2, 2, 2]),
            ("Merida", [1, 1, 1, 2, 2]),
            ("Caracas", [1, 1, 2, 2, 2]),
            ("Guatemala", [1, 1, 2, 2, 2]),
            ("Santo Domingo", [1, 1, 1, 1, 2]),
            ("San Antonio", [0, 0, 1, 1, 2]),
            // Spanish South America — viceregal and audiencia seats.
            ("Lima", [2, 2, 2, 2, 2]),       // viceregal capital of Peru
            ("Cusco", [2, 2, 2, 2, 2]),      // former Inca capital, large highland city
            ("Bogota", [1, 1, 2, 2, 2]),     // capital of New Granada
            ("Quito", [1, 2, 2, 2, 2]),      // Audiencia seat
            ("Santiago", [1, 1, 2, 2, 3]),   // Chile; booms after independence
            ("La Paz", [1, 1, 1, 2, 2]),     // Andean highland city
            ("Guayaquil", [0, 1, 1, 2, 2]),  // main Pacific port / shipyards
            ("Medellin", [0, 0, 1, 1, 2]),   // small town, grows with mining later
            ("Panama", [1, 1, 1, 1, 2]),     // transit port, mid-century decline
            ("Montevideo", [0, 1, 1, 2, 2]), // founded 1724, fast-growing port
            ("Buenos Aires", [1, 2, 2, 3, 3]), // viceregal capital from 1776, booms
            ("Asuncion", [0, 1, 1, 1, 2]),   // older inland colonial town
            // Portuguese Brazil.
            ("Salvador", [2, 2, 2, 2, 2]),   // first colonial capital, major port
            ("Rio de Janeiro", [1, 2, 3, 3, 3]), // capital 1763, royal court 1808, booms
            ("São Paulo", [0, 0, 1, 1, 2]),  // small inland town, grows late
            // French Caribbean — Port-au-Prince, a rich sugar port before 1804.
            ("Port-au-Prince", [1, 2, 2, 2, 2]),
            // Later-founded cities (present only in the matching scenarios).
            ("Washington", [0, 1, 1, 2, 2]),
            ("Chicago", [0, 0, 0, 1, 3]),
            ("Austin", [0, 0, 0, 0, 1]),
            ("Toronto", [0, 0, 1, 2, 2]),
            ("York", [0, 0, 1, 1, 1]),
            ("Ottawa", [0, 0, 0, 0, 1]),
        ];
        for c in &mut self.cities {
            if c.is_village || c.is_fort || c.is_camp {
                continue;
            }
            if let Some((_, tiers)) = table.iter().find(|(name, _)| *name == c.name) {
                c.tier = tiers[col];
            }
        }
    }

    /// Sets each nation's starting law choices to a historically plausible
    /// stance for the scenario year. Index per category matches the option
    /// order in `law_categories()`.
    fn init_laws(&mut self) {
        let year = self.year;
        let factions: Vec<String> = self.cities.iter().map(|c| c.sovereign.clone()).collect();
        let mut unique: Vec<String> = Vec::new();
        for f in factions {
            if !f.is_empty() && !unique.contains(&f) {
                unique.push(f);
            }
        }
        for f in &self.factions {
            if !unique.contains(f) {
                unique.push(f.clone());
            }
        }
        for nation in unique {
            let laws = starting_laws(&nation, year);
            self.laws.insert(nation, laws);
        }
    }

    /// Seeds each settlement's aristocracy fraction. Capitals and high-tier
    /// cities hold more of the ruling class; villages, forts, and native camps
    /// have little or none.
    fn init_classes(&mut self) {
        for c in &mut self.cities {
            c.aristocracy = if is_native(&c.owner) {
                0.0
            } else if c.is_capital {
                0.12
            } else if c.is_village || c.is_fort || c.is_camp {
                0.02
            } else {
                0.04 + c.tier as f32 * 0.02
            };
        }
    }

    /// Sets the historically pre-existing puppet relationships (puppet ->
    /// overlord) for the scenario. Rupert's Land was a British-chartered
    /// territory under the Hudson's Bay Company; Canada becomes a British
    /// dominion when confederation fires.
    fn init_puppets(&mut self) {
        let active: std::collections::HashSet<&String> = self.cities.iter().map(|c| &c.sovereign).collect();
        if active.iter().any(|s| *s == "Rupert's Land") && active.iter().any(|s| *s == "Great Britain") {
            self.puppets.insert("Rupert's Land".into(), "Great Britain".into());
        }
        // Spanish colonial viceroyalties start as puppets of Spain. They later
        // break away into the independent South American republics.
        if active.iter().any(|s| *s == "Spain") {
            for vice in ["New Granada", "Peru", "Rio de la Plata", "New Spain"] {
                if active.iter().any(|s| *s == vice) {
                    self.puppets.insert(vice.into(), "Spain".into());
                }
            }
        }
        // Louisiana is a French colonial puppet. It is dissolved into the
        // United States at the 1803 Louisiana Purchase.
        if active.iter().any(|s| *s == "France") && active.iter().any(|s| *s == "Louisiana") {
            self.puppets.insert("Louisiana".into(), "France".into());
        }
        // Saint-Domingue is a French colonial puppet. It becomes independent
        // Haiti at the 1804 Haitian Revolution.
        if active.iter().any(|s| *s == "France") && active.iter().any(|s| *s == "Saint-Domingue") {
            self.puppets.insert("Saint-Domingue".into(), "France".into());
        }
    }

    /// Makes `puppet` a vassal of `overlord`, ending any war between them.
    pub fn make_puppet(&mut self, overlord: &str, puppet: &str) {
        if overlord == puppet || self.eliminated.contains(puppet) {
            return;
        }
        self.puppets.insert(puppet.to_string(), overlord.to_string());
        let key = Pair::new(overlord, puppet);
        self.wars.remove(&key);
        // Becoming a puppet changes the territory's map shade.
        self.claims_dirty = true;
        self.news(format!("{puppet} becomes a puppet of {overlord}!"));
    }

    /// Frees a puppet from its overlord.
    pub fn release_puppet(&mut self, puppet: &str) {
        if let Some(overlord) = self.puppets.remove(puppet) {
            // Reverting to a sovereign color is a visible map change.
            self.claims_dirty = true;
            self.news(format!("{puppet} breaks free from {overlord}!"));
        }
    }

    /// The overlord of a nation, if it is a puppet.
    pub fn overlord_of(&self, nation: &str) -> Option<&str> {
        self.puppets.get(nation).map(|s| s.as_str())
    }

    /// The puppets a nation currently holds.
    pub fn puppets_of(&self, overlord: &str) -> Vec<String> {
        self.puppets.iter().filter(|(_, o)| *o == overlord).map(|(p, _)| p.clone()).collect()
    }

    /// Periodic puppet upkeep: puppets pay tribute to their overlord, dead
    /// relationships are cleared, and a struggling or ambitious puppet may
    /// break free — more easily if its overlord has grown weak.
    fn puppet_dynamics(&mut self) {
        // Drop links where either side no longer exists.
        let alive: std::collections::HashSet<String> =
            self.cities.iter().map(|c| c.sovereign.clone()).collect();
        self.puppets
            .retain(|p, o| alive.contains(p) && alive.contains(o) && !self.eliminated.contains(p));

        let pairs: Vec<(String, String)> = self.puppets.iter().map(|(p, o)| (p.clone(), o.clone())).collect();
        let mut freed: Vec<String> = Vec::new();
        for (puppet, overlord) in pairs {
            // Tribute: a little gold flows from puppet to overlord.
            let tribute = self.materials.get(&puppet).map(|s| s.get("Gold")).unwrap_or(0) / 50;
            if tribute > 0 {
                self.transfer(&puppet, &overlord, "Gold", tribute);
            }
            // Break-free chance, higher when the overlord holds fewer cities
            // than the puppet (a weak master cannot hold its vassal).
            let o_cities = self.cities.iter().filter(|c| c.sovereign == overlord).count();
            let p_cities = self.cities.iter().filter(|c| c.sovereign == puppet).count();
            let mut chance = 0.004;
            if p_cities > o_cities {
                chance += 0.03;
            }
            if self.at_war(&overlord, puppet.as_str()) {
                chance += 0.05;
            }
            if rand::random::<f32>() < chance {
                freed.push(puppet);
            }
        }
        for p in freed {
            self.release_puppet(&p);
        }
    }

    /// Nudges every settlement's stored ethnicity and religion a tiny step
    /// toward the ruling nation's group, so demographics shift gradually (via
    /// settlers, missionaries, and migration) instead of snapping on conquest.
    /// Native-held settlements are left alone. Called on a slow cadence.
    fn drift_demographics(&mut self) {
        const RATE: f32 = 0.00012; // very slow: full shift takes many in-game years
        for c in &mut self.cities {
            if is_native(&c.owner) {
                continue;
            }
            drift_toward(&mut c.ethnicity, owner_ethnicity_key(&c.owner), RATE);
            drift_toward(&mut c.religion, owner_religion_key(&c.owner), RATE);
        }
    }

    pub fn tick(&mut self, maps: &Maps) {
        if self.speed <= 0 {
            return;
        }
        self.day_timer += self.speed;
        if self.day_timer >= DAY_TICKS {
            self.day_timer = 0;
            self.advance_date(maps);
        }
        self.spawn_armies(maps);
        self.material_timer += self.speed;
        if self.material_timer >= DAY_TICKS {
            self.material_timer = 0;
            self.gold_tax(maps);
            self.heal_garrisons();
            self.update_floods();
            self.drift_demographics();
        }
        self.produce(maps);
        self.passive_regen();
        self.upkeep_timer += self.speed;
        if self.upkeep_timer >= DAY_TICKS {
            self.upkeep_timer = 0;
            self.upkeep();
            self.upgrade_units();
            self.upgrade_cities();
            self.finish_construction();
            self.ai_build(maps);
        }
        self.maybe_extra_merchants();
        self.maybe_settlers(maps);
        self.treaty_timer += self.speed;
        if self.treaty_timer >= DAY_TICKS * 5 {
            self.treaty_timer = 0;
            self.stalemates();
            self.forced_peace();
            self.alliance_chances();
            self.war_fatigue();
            self.puppet_dynamics();
        }
        self.election_timer -= self.speed;
        if self.election_timer <= 0 {
            self.election_timer = DAY_TICKS * 365 * 4;
            self.run_elections();
        }
        self.war_timer += self.speed;
        if self.war_timer >= DAY_TICKS * 10 {
            self.war_timer = 0;
            self.territorial_demands();
            self.war_declarations();
        }
        self.disaster_timer += self.speed;
        if self.disaster_timer >= DAY_TICKS * 10 {
            self.disaster_timer = 0;
            self.disasters();
            self.pirate_spawn();
        }
        self.tick_relations();
        self.eliminations();
        self.one_capital();
        self.move_units(maps);
        self.battles(maps);
        self.move_merchants(maps);
        self.move_settlers(maps);
        self.units.retain(|u| u.alive);
        self.merchants.retain(|m| m.alive);
        self.settlers.retain(|s| s.alive);
    }

    pub fn relations(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for pair in &self.wars {
            if let Some((a, b)) = self.war_names.get(pair) {
                if !self.eliminated.contains(a) && !self.eliminated.contains(b) {
                    lines.push(format!("WAR: {a} vs {b}"));
                }
            }
        }
        for (pair, frames) in &self.treaties {
            if let Some((a, b)) = self.war_names.get(pair) {
                lines.push(format!("TREATY: {a} & {b} ({}d)", frames / DAY_TICKS));
            }
        }
        for (pair, frames) in &self.alliances {
            if let Some((a, b)) = self.war_names.get(pair) {
                lines.push(format!("ALLIANCE: {a} & {b} ({}d)", frames / DAY_TICKS));
            }
        }
        lines
    }

    pub fn is_flooded(&self, id: u32) -> bool {
        self.flooded.contains_key(&id)
    }

    pub fn manpower_of(&self, faction: &str) -> i32 {
        self.manpower.get(faction).copied().unwrap_or(0)
    }

    pub fn purse(&self, faction: &str) -> (i32, i32, i32, i32, i32, i32) {
        match self.materials.get(faction) {
            Some(s) => (s.gold, s.food, s.lumber, s.hide, s.iron, s.coal),
            None => (0, 0, 0, 0, 0, 0),
        }
    }

    #[allow(dead_code)]
    pub fn stock_line(&self, faction: &str) -> String {
        let s = self.materials.get(faction).cloned().unwrap_or_else(Stock::zero);
        format!("Gold {}  Food {}  Lumber {}  Hide {}  Iron {}  Coal {}", s.gold, s.food, s.lumber, s.hide, s.iron, s.coal)
    }

    pub fn active(&self) -> Vec<String> {
        self.factions.iter().filter(|f| !self.eliminated.contains(*f)).cloned().collect()
    }

    /// Region ids a nation controls: the regions of its territory-claiming
    /// cities plus any authored starting claims. Used to place the big faded
    /// nation-name label across the country's land (see `draw_nation_labels`).
    /// Returns an empty list for mobile tribes that hold camps without claiming
    /// territory, so they get no sprawling label.
    pub fn owned_regions(&self, faction: &str) -> Vec<i32> {
        if !crate::config::claims_territory(faction) {
            return Vec::new();
        }
        let mut owned: std::collections::HashSet<i32> = self
            .cities
            .iter()
            .filter(|c| c.owner == faction && c.region >= 0 && crate::config::claims_territory(&c.owner))
            .map(|c| c.region)
            .collect();
        for (region, nation) in &self.region_claims {
            if nation == faction {
                owned.insert(*region);
            }
        }
        owned.into_iter().collect()
    }

    /// Public wrapper so the renderer can tint a nation's label with the same
    /// color its territory overlay uses (puppets shade toward their overlord).
    pub fn label_color(&self, faction: &str) -> [u8; 3] {
        self.display_color(faction)
    }

    fn news(&mut self, msg: impl Into<String>) {
        self.news.push_front(msg.into());
        while self.news.len() > 40 {
            self.news.pop_back();
        }
    }

    fn paper(&mut self, title: &str, body: &str) {
        self.paper = Some((title.into(), body.into()));
    }

    fn touch(&mut self, faction: &str) {
        self.materials.entry(faction.to_string()).or_insert_with(Stock::starting);
        self.manpower.entry(faction.to_string()).or_insert(0);
        self.focus.entry(faction.to_string()).or_insert_with(|| "Neutral".into());
        self.focus_timer.entry(faction.to_string()).or_insert(DAY_TICKS * 180);
    }

    fn add_faction(&mut self, name: &str, gold: i32) {
        if !self.factions.iter().any(|f| f == name) {
            self.factions.push(name.into());
        }
        self.touch(name);
        if let Some(s) = self.materials.get_mut(name) {
            *s = Stock::starting();
            s.gold = gold;
        }
        self.eliminated.remove(name);
    }

    fn build_cities(&mut self) {
        let mut rng = rand::thread_rng();
        let strategies = ["Aggressive", "Defensive", "Balanced"];
        let mats = ["Lumber", "Hide", "Iron", "Coal"];
        let mut add = |g: &mut Game, name: &str, x: f32, y: f32, owner: &str, capital: bool, village: bool, fort: bool, camp: bool, troops: i32, scaled: bool, founded: Option<i32>| {
            let id = g.next_id;
            g.next_id += 1;
            let scale = if scaled { 1.0 } else { COORD_SCALE };
            let city = City {
                id,
                name: name.into(),
                x: x * scale,
                y: y * scale,
                owner: owner.into(),
                sovereign: owner.into(),
                troops,
                tier: if village || fort { 0 } else { 1 },
                is_capital: capital,
                is_village: village,
                is_fort: fort,
                is_camp: camp,
                material: if camp { "Hide".into() } else { mats[rng.gen_range(0..mats.len())].into() },
                buildings: Vec::new(),
                construction: None,
                spawn_cd: 0,
                happiness: 100,
                regen_cooldown: 0,
                population: 0,
                pop_acc: 0.0,
                region: -1,
                ethnicity: Vec::new(),
                religion: Vec::new(),
                aristocracy: 0.0,
                founded_order: 0,
                founded_year: founding_year(name),
                builder: owner.into(),
                };
            if let Some(year) = founded {
                g.founded.push((year, city));
            } else {
                g.cities.push(city);
            }
        };
        // Scaled colonial cities (coordinates match cities_data.make_city inputs).
        let cities = [
            ("Mexico City", 171.0, 340.0, "New Spain", true, false),
            ("Havana", 260.0, 318.0, "Spain", false, false),
            ("Oaxaca", 187.0, 354.0, "New Spain", false, false),
            ("Merida", 220.0, 334.0, "New Spain", false, false),
            ("Albuquerque", 145.0, 258.0, "New Spain", false, false),
            ("San Agustin", 256.0, 284.0, "Spain", false, false),
            ("Monterey", 82.0, 233.0, "New Spain", false, false),
            ("San Diego", 95.0, 258.0, "New Spain", false, false),
            ("Chihuahua", 145.0, 290.0, "New Spain", false, false),
            ("Caracas", 343.0, 370.0, "New Granada", false, false),
            ("Barranquilla", 301.0, 371.0, "New Granada", false, false),
            ("Santo Domingo", 317.0, 328.0, "Spain", false, false),
            ("Quebec", 280.0, 180.0, "France", true, false),
            ("Montreal", 272.0, 189.0, "France", false, false),
            ("New Orleans", 217.0, 286.0, "Louisiana", true, false),
            ("Mobile", 228.0, 283.0, "France", false, false),
            ("Saint-Louis", 212.0, 242.0, "Louisiana", false, false),
            ("Fort Detroit", 241.0, 218.0, "France", false, false),
            ("Port-au-Prince", 306.0, 331.0, "Saint-Domingue", true, false),
            ("Plaisance", 332.0, 161.0, "France", false, false),
            ("Boston", 287.0, 208.0, "Great Britain", true, false),
            ("New York", 280.0, 218.0, "Great Britain", false, false),
            ("Halifax", 309.0, 186.0, "Great Britain", false, false),
            ("Charleston", 262.0, 264.0, "Great Britain", false, false),
            ("Richmond", 267.0, 237.0, "Great Britain", false, false),
            ("Kingston", 285.0, 338.0, "Great Britain", false, false),
            ("Nassau", 276.0, 302.0, "Great Britain", false, false),
            ("Belize Town", 228.0, 350.0, "Great Britain", false, false),
            ("Guatemala", 216.0, 364.0, "New Spain", false, false),
            ("San Jose", 249.0, 382.0, "New Spain", false, false),
            ("Sitka", 84.0, 120.0, "Russia", true, false),
            ("Kodiak", 52.0, 91.0, "Russia", false, false),
            ("Godthaab", 284.0, 81.0, "Denmark", true, false),
            ("Reykjavik", 317.0, 46.0, "Denmark", false, false),
            ("San Antonio", 174.0, 293.0, "New Spain", false, false),
            ("Panama", 270.0, 386.0, "New Granada", false, false),
            ("Bogota", 308.0, 398.0, "New Granada", true, false),
            ("Quito", 282.0, 428.0, "New Granada", false, false),
            ("Lima", 290.0, 492.0, "Peru", true, false),
            ("Cusco", 317.0, 499.0, "Peru", false, false),
            ("La Paz", 338.0, 512.0, "Peru", false, false),
            ("Guayaquil", 273.0, 440.0, "New Granada", false, false),
            ("Medellin", 294.0, 394.0, "New Granada", false, false),
            ("Santiago", 333.0, 607.0, "Spain", false, false),
            ("Montevideo", 395.0, 607.0, "Rio de la Plata", false, false),
            ("Buenos Aires", 391.0, 613.0, "Rio de la Plata", true, false),
            ("Asuncion", 395.0, 559.0, "Rio de la Plata", false, false),
            ("Salvador", 497.0, 497.0, "Portugal", true, false),
            ("Rio de Janeiro", 470.0, 550.0, "Portugal", false, false),
            ("São Paulo", 447.0, 556.0, "Portugal", false, false),
            ("Cayenne", 424.0, 399.0, "France", false, false),
            ("Paramaribo", 410.0, 391.0, "Netherlands", true, false),
        ];
        for (n, x, y, o, cap, _v) in cities {
            add(self, n, x, y, o, cap, false, false, false, if cap { 200 } else { 100 }, false, None);
        }
        let villages = [
            ("Tadoussac", 283.0, 171.0, "France"),
            ("Williamsburg", 272.0, 239.0, "Great Britain"),
            ("Onondaga", 265.0, 211.0, "Iroquois"),
            ("Norridgewock", 287.0, 192.0, "Wabanaki"),
            ("Chota", 229.0, 264.0, "Cherokee"),
            ("York Factory", 202.0, 143.0, "Rupert's Land"),
            ("Moose Factory", 228.0, 170.0, "Rupert's Land"),
            ("Moosonee", 242.0, 176.0, "Cree"),
            ("Opaskwayak", 174.0, 163.0, "Cree"),
            ("Chisasibi", 246.0, 163.0, "Cree"),
            ("Waskahigan", 133.0, 157.0, "Cree"),
            ("Mdewakantonwan", 200.0, 210.0, "Dakota"),
            ("Wahpekute", 204.0, 214.0, "Dakota"),
        ];
        for (n, x, y, o) in villages {
            add(self, n, x, y, o, false, true, false, false, 50, false, None);
        }
        // Unscaled in the Python data. Scaled here.
        add(self, "Saint-Pierre", 376.0, 350.0, "France", false, true, false, false, 50, false, None);
        add(self, "George Town", 263.0, 336.0, "Great Britain", false, true, false, false, 50, false, None);
        add(self, "Comancheria", 161.0, 260.0, "Comanche", false, false, true, true, 30, false, None);
        add(self, "Quahadi Camp", 165.0, 265.0, "Comanche", false, false, true, true, 30, false, None);
        add(self, "Penateka Camp", 175.0, 276.0, "Comanche", false, false, true, true, 30, false, None);
        add(self, "Oglala Camp", 163.0, 215.0, "Dakota", false, false, true, true, 30, false, None);
        let founded = [
            ("Washington", 267.0, 229.0, "United States", 1790),
            ("Austin", 178.0, 287.0, "United States", 1836),
            ("Victoria", 95.0, 170.0, "Great Britain", 1843),
            ("Ottawa", 268.0, 197.0, "Great Britain", 1855),
            ("Chicago", 223.0, 223.0, "United States", 1833),
            ("York", 254.0, 206.0, "Great Britain", 1787),
        ];
        for (n, x, y, o, yr) in founded {
            add(self, n, x, y, o, false, false, false, false, 100, false, Some(yr));
            if let Some((_, c)) = self.founded.last_mut() {
                c.tier = if n == "Washington" || n == "Chicago" { 1 } else { 0 };
            }
        }
        let _ = strategies;
        for f in ["Spain", "France", "Great Britain", "Russia", "Denmark", "Portugal", "Netherlands", "New Granada", "Peru", "Rio de la Plata", "New Spain", "Louisiana", "Saint-Domingue", "Rupert's Land", "Iroquois", "Wabanaki", "Cherokee", "Comanche", "Cree", "Dakota"] {
            let owned: Vec<usize> = self.cities.iter().enumerate().filter(|(_, c)| c.owner == f).map(|(i, _)| i).collect();
            if owned.is_empty() {
                continue;
            }
            if !self.cities.iter().any(|c| c.owner == f && c.material == "Hide") {
                let i = owned[rng.gen_range(0..owned.len())];
                self.cities[i].material = "Hide".into();
            }
            if !self.cities.iter().any(|c| c.owner == f && c.material == "Lumber") {
                let rest: Vec<usize> = owned.iter().copied().filter(|i| self.cities[*i].material != "Hide").collect();
                let pick = if rest.is_empty() { owned[0] } else { rest[rng.gen_range(0..rest.len())] };
                self.cities[pick].material = "Lumber".into();
            }
        }
    }

    fn boot_factions(&mut self) {
        let names = [
            "France", "Spain", "Great Britain", "Russia", "Denmark", "Portugal", "Netherlands", "New Granada", "Peru", "Rio de la Plata", "New Spain", "Louisiana", "Saint-Domingue", "Iroquois", "Wabanaki", "Cherokee",
            "Comanche", "Cree", "Dakota", "Pirates", "Rupert's Land",
        ];
        for n in names {
            self.factions.push(n.into());
            self.touch(n);
        }
        let gb_ir = Pair::new("Great Britain", "Iroquois");
        let fr_wa = Pair::new("France", "Wabanaki");
        let gb_ch = Pair::new("Great Britain", "Cherokee");
        self.alliances.insert(gb_ir, DAY_TICKS * 1095);
        self.alliances.insert(fr_wa, DAY_TICKS * 1095);
        self.alliances.insert(gb_ch, DAY_TICKS * 1095);
        self.war_names.insert(gb_ir, ("Great Britain".into(), "Iroquois".into()));
        self.war_names.insert(fr_wa, ("France".into(), "Wabanaki".into()));
        self.war_names.insert(gb_ch, ("Great Britain".into(), "Cherokee".into()));
    }

    fn give(&mut self, name: &str, owner: &str, capital: bool) {
        for c in &mut self.cities {
            if c.name == name {
                c.owner = owner.into();
                c.sovereign = owner.into();
                c.is_capital = capital;
            }
        }
    }

    fn scenario_1790(&mut self) {
        self.add_faction("United States", 50);
        self.eliminated.insert("Iroquois".into());
        self.eliminated.insert("Wabanaki".into());
        self.eliminated.insert("Cherokee".into());
        self.rename_detroit("United States");
        let rows = [
            ("Washington", "United States", true),
            ("Boston", "United States", false),
            ("New York", "United States", false),
            ("Charleston", "United States", false),
            ("Williamsburg", "United States", false),
            ("Richmond", "United States", false),
            ("Onondaga", "United States", false),
            ("Norridgewock", "United States", false),
            ("New Orleans", "France", true),
            ("Saint-Louis", "France", false),
            ("York Factory", "Rupert's Land", false),
            ("Quebec", "Great Britain", false),
            ("Montreal", "Great Britain", false),
            ("Tadoussac", "Great Britain", false),
            ("Plaisance", "Great Britain", false),
            ("Nassau", "Great Britain", false),
            ("Kingston", "Great Britain", false),
            ("Halifax", "Great Britain", false),
            ("Mexico City", "New Spain", true),
            ("Havana", "Spain", false),
            ("Merida", "New Spain", false),
            ("Albuquerque", "New Spain", false),
            ("San Agustin", "Spain", false),
            ("Monterey", "New Spain", false),
            ("Caracas", "Spain", false),
            ("Port-au-Prince", "France", false),
            ("San Antonio", "New Spain", false),
            // Mobile: Spanish West Florida after 1780.
            ("Mobile", "Spain", false),
        ];
        for (n, o, cap) in rows {
            self.give(n, o, cap);
        }
        let key = Pair::new("France", "United States");
        self.alliances.insert(key, DAY_TICKS * 730);
        self.war_names.insert(key, ("France".into(), "United States".into()));
        self.focus.insert("United States".into(), "Neutral".into());
    }

    fn scenario_1809(&mut self) {
        self.add_faction("United States", 60);
        self.add_faction("Haiti", 20);
        self.eliminated.insert("Iroquois".into());
        self.eliminated.insert("Wabanaki".into());
        self.eliminated.insert("Cherokee".into());
        self.rename_detroit("United States");
        let rows = [
            ("Washington", "United States", true),
            ("Boston", "United States", false),
            ("New York", "United States", false),
            ("Charleston", "United States", false),
            ("Williamsburg", "United States", false),
            ("Richmond", "United States", false),
            ("New Orleans", "United States", false),
            ("Saint-Louis", "United States", false),
            ("Onondaga", "United States", false),
            ("Norridgewock", "United States", false),
            ("York Factory", "Rupert's Land", false),
            ("Quebec", "Great Britain", false),
            ("Montreal", "Great Britain", false),
            ("Tadoussac", "Great Britain", false),
            ("Plaisance", "Great Britain", false),
            ("Nassau", "Great Britain", false),
            ("Kingston", "Great Britain", false),
            ("Halifax", "Great Britain", false),
            ("Mexico City", "New Spain", true),
            ("Havana", "Spain", false),
            ("Merida", "New Spain", false),
            ("Albuquerque", "New Spain", false),
            ("San Agustin", "Spain", false),
            ("Monterey", "New Spain", false),
            ("San Diego", "New Spain", false),
            ("Chihuahua", "New Spain", false),
            ("Caracas", "Spain", false),
            ("Port-au-Prince", "Haiti", true),
            ("Saint-Pierre", "France", true),
            // Mobile: still Spanish West Florida (the US seizes it in 1813).
            ("Mobile", "Spain", false),
        ];
        for (n, o, cap) in rows {
            self.give(n, o, cap);
        }
        self.declare_war("United States", "Great Britain", "");
        self.focus.insert("United States".into(), "Aggressive".into());
    }

    fn scenario_1835(&mut self) {
        self.add_faction("United States", 80);
        self.add_faction("Mexico", 40);
        self.add_faction("Haiti", 20);
        self.eliminated.insert("Iroquois".into());
        self.eliminated.insert("Wabanaki".into());
        self.eliminated.insert("Cherokee".into());
        self.rename_detroit("United States");
        let rows = [
            ("Washington", "United States", true),
            ("Boston", "United States", false),
            ("New York", "United States", false),
            ("Charleston", "United States", false),
            ("Williamsburg", "United States", false),
            ("Richmond", "United States", false),
            ("New Orleans", "United States", false),
            ("Saint-Louis", "United States", false),
            ("Onondaga", "United States", false),
            ("Norridgewock", "United States", false),
            ("San Agustin", "Spain", false),
            ("Mexico City", "Mexico", true),
            ("Chihuahua", "Mexico", false),
            ("Merida", "Mexico", false),
            ("Oaxaca", "Mexico", false),
            ("Albuquerque", "Mexico", false),
            ("San Diego", "Mexico", false),
            ("Monterey", "Mexico", false),
            ("San Antonio", "Mexico", false),
            ("Port-au-Prince", "Haiti", true),
            ("York Factory", "Rupert's Land", false),
            ("Quebec", "Great Britain", false),
            ("Montreal", "Great Britain", false),
            ("Tadoussac", "Great Britain", false),
            ("Plaisance", "Great Britain", false),
            ("Nassau", "Great Britain", false),
            ("Kingston", "Great Britain", false),
            ("Havana", "Spain", true),
            ("Caracas", "Spain", false),
            ("Saint-Pierre", "France", true),
            // Mobile: American since 1813.
            ("Mobile", "United States", false),
        ];
        for (n, o, cap) in rows {
            self.give(n, o, cap);
        }
    }

    fn scenario_1858(&mut self) {
        self.add_faction("United States", 100);
        self.add_faction("Mexico", 40);
        self.add_faction("Haiti", 20);
        self.eliminated.insert("Iroquois".into());
        self.eliminated.insert("Wabanaki".into());
        self.eliminated.insert("Cherokee".into());
        self.eliminated.insert("Texas".into());
        self.rename_detroit("United States");
        let rows = [
            ("Washington", "United States", true),
            ("Boston", "United States", false),
            ("New York", "United States", false),
            ("Charleston", "United States", false),
            ("Richmond", "United States", false),
            ("Williamsburg", "United States", false),
            ("New Orleans", "United States", false),
            ("Saint-Louis", "United States", false),
            ("Onondaga", "United States", false),
            ("Norridgewock", "United States", false),
            ("San Agustin", "United States", false),
            ("Albuquerque", "United States", false),
            ("Halifax", "Great Britain", false),
            ("Victoria", "Great Britain", false),
            ("York Factory", "Rupert's Land", false),
            ("Quebec", "Great Britain", false),
            ("Montreal", "Great Britain", false),
            ("Tadoussac", "Great Britain", false),
            ("Plaisance", "Great Britain", false),
            ("Nassau", "Great Britain", false),
            ("Kingston", "Great Britain", false),
            ("Mexico City", "Mexico", true),
            ("Chihuahua", "Mexico", false),
            ("Merida", "Mexico", false),
            ("Oaxaca", "Mexico", false),
            ("Monterey", "Mexico", false),
            ("San Diego", "Mexico", false),
            ("Havana", "Spain", true),
            ("Caracas", "Spain", false),
            ("Port-au-Prince", "Haiti", true),
            ("Saint-Pierre", "France", true),
            ("San Antonio", "United States", false),
            ("Austin", "United States", false),
            // Mobile: an American Gulf port (Confederate once the war fires).
            ("Mobile", "United States", false),
        ];
        for (n, o, cap) in rows {
            self.give(n, o, cap);
        }
    }

    fn rename_detroit(&mut self, owner: &str) {
        for c in &mut self.cities {
            if c.name == "Fort Detroit" {
                c.name = "Detroit".into();
                c.owner = owner.into();
                c.sovereign = owner.into();
                c.is_capital = false;
            }
        }
    }

    fn found_due(&mut self) {
        let year = self.year;
        let pending = std::mem::take(&mut self.founded);
        let mut later = Vec::new();
        let mut names = Vec::new();
        for (y, city) in pending {
            if y <= year && !self.cities.iter().any(|c| c.name == city.name) {
                names.push(city.name.clone());
                self.cities.push(city);
            } else if y > year {
                later.push((y, city));
            }
        }
        self.founded = later;
        for name in names {
            self.news(format!("{name} is founded!"));
        }
    }

    fn init_population(&mut self) {
        let year = self.year;
        let mut rng = rand::thread_rng();
        for c in &mut self.cities {
            let native = is_native(&c.owner);
            let mut pop = pop_override(&c.name, year).unwrap_or_else(|| {
                // No exact historical figure: pick a class-appropriate base and
                // vary it by a stable per-name fraction so each settlement has
                // its own population instead of all sharing one number. Native
                // camps and villages stay realistically small and do not grow
                // with the colonial year multiplier.
                let seed = name_seed(&c.name);
                let span = |lo: i32, hi: i32| lo + (seed % (hi - lo).max(1) as u32) as i32;
                if c.is_camp {
                    return span(120, 500);
                }
                if c.is_village && native {
                    return span(400, 1400);
                }
                if c.is_fort {
                    return span(120, 500);
                }
                let base = if c.is_village {
                    span(500, 2000)
                } else if c.is_capital {
                    span(8_000, 16_000)
                } else {
                    span(2_000, 7_000)
                };
                (base as f32 * year_mult(year)) as i32
            });
            pop = (pop as f32 * rng.gen_range(0.9..1.1)) as i32;
            c.population = pop.max(50);
        }
    }

    fn init_manpower(&mut self) {
        let factions = self.active();
        for f in factions {
            let pop: i32 = self.cities.iter().filter(|c| c.owner == f).map(|c| c.population).sum();
            let rate = if is_native(&f) { 0.05 } else { 0.03 };
            self.manpower.insert(f, (pop as f32 * rate) as i32);
        }
    }

    fn assign_buildings(&mut self) {
        let year = self.year;
        for c in &mut self.cities {
            if c.is_fort || c.is_camp {
                continue;
            }
            let native = is_native(&c.owner);
            let island = ISLANDS.contains(&c.name.as_str());
            if native {
                push_b(&mut c.buildings, "Council Lodge");
            } else {
                push_b(&mut c.buildings, "Town Hall");
            }
            if year <= 1754 && !native && c.is_capital && !island {
                push_b(&mut c.buildings, "Road");
            }
            push_b(&mut c.buildings, "Farm");
            if year >= 1790 && !native {
                if c.is_capital && !island {
                    push_b(&mut c.buildings, "Road");
                }
                if c.is_capital {
                    push_b(&mut c.buildings, "Stables");
                    push_b(&mut c.buildings, "Church");
                }
                if matches!(c.name.as_str(), "New York" | "Boston" | "Havana" | "Quebec" | "Mexico City" | "New Orleans") {
                    push_b(&mut c.buildings, "Market");
                }
            }
            if year >= 1809 && !native {
                if !c.is_village && !island {
                    push_b(&mut c.buildings, "Road");
                }
                if !c.is_village {
                    push_b(&mut c.buildings, "Stables");
                }
                if c.is_capital {
                    push_b(&mut c.buildings, "Barracks");
                }
                if c.material == "Lumber" {
                    push_b(&mut c.buildings, "Lumber Mill");
                }
            }
            if year >= 1835 && !native && !c.is_village {
                push_b(&mut c.buildings, "Church");
                push_b(&mut c.buildings, "Barracks");
                push_b(&mut c.buildings, "Market");
                if matches!(c.name.as_str(), "Charleston" | "Havana" | "Kingston" | "New Orleans" | "Port-au-Prince" | "Nassau") {
                    push_b(&mut c.buildings, "Plantation");
                }
                if matches!(c.name.as_str(), "New York" | "Boston" | "Mexico City" | "Richmond") {
                    push_b(&mut c.buildings, "Foundry");
                }
            }
            if year >= 1858 && !native && !c.is_village {
                push_b(&mut c.buildings, "Warehouse");
                push_b(&mut c.buildings, "Stables");
            }
        }
    }
}

fn push_b(list: &mut Vec<String>, name: &str) {
    if !list.iter().any(|b| b == name) {
        list.push(name.into());
    }
}

/// Moves a demographic distribution one small step toward `target_key`: the
/// target's share grows by `rate` (added if missing), the remainder shrinks
/// proportionally, and the whole thing is renormalized to sum to 1.0. A tiny
/// floor keeps minorities from vanishing entirely, so the original population
/// never fully disappears.
fn drift_toward(mix: &mut Vec<(String, f32)>, target_key: &str, rate: f32) {
    if mix.is_empty() {
        mix.push((target_key.to_string(), 1.0));
        return;
    }
    // Ensure the target exists.
    if !mix.iter().any(|(k, _)| k == target_key) {
        mix.push((target_key.to_string(), 0.0));
    }
    // Grow the target, shrink the rest proportionally.
    let others_total: f32 = mix.iter().filter(|(k, _)| k != target_key).map(|(_, s)| *s).sum();
    let take = rate.min(others_total.max(0.0));
    for (k, s) in mix.iter_mut() {
        if k == target_key {
            *s += take;
        } else if others_total > 0.0 {
            *s -= take * (*s / others_total);
        }
    }
    // Keep a small floor so established groups never fully disappear.
    for (_, s) in mix.iter_mut() {
        if *s < 0.01 {
            *s = 0.0;
        }
    }
    // Renormalize.
    let sum: f32 = mix.iter().map(|(_, s)| *s).sum();
    if sum > 0.0 {
        for (_, s) in mix.iter_mut() {
            *s /= sum;
        }
    }
    // Drop groups that have faded to nothing (but always keep the target).
    mix.retain(|(k, s)| *s > 0.0 || k == target_key);
}

/// Stable FNV-1a hash of a settlement name, used to give each place its own
/// deterministic population so no two share the exact same number.
fn name_seed(name: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

fn year_mult(year: i32) -> f32 {
    let mut m = 1.0;
    for (y, v) in [(1754, 1.0), (1790, 1.4), (1809, 1.7), (1835, 2.2), (1858, 3.0)] {
        if year >= y {
            m = v;
        }
    }
    m
}

fn pop_override(name: &str, year: i32) -> Option<i32> {
    const TABLE: &[(&str, &[(i32, i32)])] = &[
        ("Mexico City", &[(1754, 120_000), (1790, 130_000), (1809, 140_000), (1835, 170_000), (1858, 200_000)]),
        ("Quebec", &[(1754, 8_000), (1790, 14_000), (1809, 18_000), (1835, 27_000), (1858, 42_000)]),
        ("Boston", &[(1754, 16_000), (1790, 18_000), (1809, 33_000), (1835, 78_000), (1858, 140_000)]),
        ("New York", &[(1754, 18_000), (1790, 33_000), (1809, 80_000), (1835, 200_000), (1858, 500_000)]),
        ("Havana", &[(1754, 40_000), (1790, 50_000), (1809, 80_000), (1835, 120_000), (1858, 160_000)]),
        ("New Orleans", &[(1754, 5_000), (1790, 8_000), (1809, 17_000), (1835, 50_000), (1858, 120_000)]),
        ("Charleston", &[(1754, 10_000), (1790, 16_000), (1809, 24_000), (1835, 30_000), (1858, 40_000)]),
        ("Kingston", &[(1754, 8_000), (1790, 12_000), (1809, 25_000), (1835, 30_000), (1858, 35_000)]),
        ("Richmond", &[(1754, 3_000), (1790, 5_000), (1809, 10_000), (1835, 20_000), (1858, 38_000)]),
        ("Port-au-Prince", &[(1754, 6_000), (1790, 10_000), (1809, 20_000), (1835, 25_000), (1858, 30_000)]),
        ("Halifax", &[(1754, 3_000), (1790, 5_000), (1809, 9_000), (1835, 14_000), (1858, 25_000)]),
        ("Saint-Louis", &[(1754, 1_000), (1790, 2_500), (1809, 5_000), (1835, 15_000), (1858, 80_000)]),
        ("San Antonio", &[(1754, 500), (1790, 2_500), (1809, 3_500), (1835, 5_000), (1858, 12_000)]),
        ("Montreal", &[(1754, 5_000), (1790, 9_000), (1809, 16_000), (1835, 27_000), (1858, 90_000)]),
        ("Oaxaca", &[(1754, 18_000), (1790, 20_000), (1809, 24_000), (1835, 25_000), (1858, 28_000)]),
        ("Merida", &[(1754, 10_000), (1790, 14_000), (1809, 18_000), (1835, 24_000), (1858, 30_000)]),
        ("Caracas", &[(1754, 20_000), (1790, 30_000), (1809, 42_000), (1835, 35_000), (1858, 45_000)]),
        ("Guatemala", &[(1754, 20_000), (1790, 25_000), (1809, 30_000), (1835, 35_000), (1858, 40_000)]),
        ("Santo Domingo", &[(1754, 8_000), (1790, 12_000), (1809, 10_000), (1835, 12_000), (1858, 15_000)]),
        ("Albuquerque", &[(1754, 1_000), (1790, 2_000), (1809, 3_000), (1835, 4_000), (1858, 5_000)]),
        ("Monterey", &[(1754, 500), (1790, 1_000), (1809, 1_500), (1835, 2_000), (1858, 3_000)]),
        ("San Diego", &[(1754, 400), (1790, 800), (1809, 1_200), (1835, 1_500), (1858, 2_500)]),
        ("Chihuahua", &[(1754, 4_000), (1790, 6_000), (1809, 7_000), (1835, 8_000), (1858, 12_000)]),
        ("San Agustin", &[(1754, 1_500), (1790, 2_000), (1809, 2_500), (1835, 2_500), (1858, 3_000)]),
        // Spanish South America.
        ("Lima", &[(1754, 54_000), (1790, 52_000), (1809, 64_000), (1835, 55_000), (1858, 94_000)]),
        ("Cusco", &[(1754, 26_000), (1790, 32_000), (1809, 40_000), (1835, 40_000), (1858, 45_000)]),
        ("Bogota", &[(1754, 18_000), (1790, 20_000), (1809, 30_000), (1835, 40_000), (1858, 45_000)]),
        ("Quito", &[(1754, 25_000), (1790, 25_000), (1809, 30_000), (1835, 35_000), (1858, 40_000)]),
        ("Santiago", &[(1754, 25_000), (1790, 30_000), (1809, 40_000), (1835, 70_000), (1858, 110_000)]),
        ("La Paz", &[(1754, 15_000), (1790, 20_000), (1809, 25_000), (1835, 40_000), (1858, 55_000)]),
        ("Guayaquil", &[(1754, 6_000), (1790, 10_000), (1809, 15_000), (1835, 20_000), (1858, 25_000)]),
        ("Medellin", &[(1754, 3_000), (1790, 6_000), (1809, 10_000), (1835, 18_000), (1858, 30_000)]),
        ("Panama", &[(1754, 10_000), (1790, 9_000), (1809, 8_000), (1835, 8_000), (1858, 13_000)]),
        ("Montevideo", &[(1754, 1_000), (1790, 7_000), (1809, 10_000), (1835, 15_000), (1858, 34_000)]),
        ("Buenos Aires", &[(1754, 12_000), (1790, 40_000), (1809, 45_000), (1835, 65_000), (1858, 90_000)]),
        ("Asuncion", &[(1754, 5_000), (1790, 8_000), (1809, 10_000), (1835, 12_000), (1858, 18_000)]),
        // Portuguese Brazil.
        ("Salvador", &[(1754, 40_000), (1790, 45_000), (1809, 55_000), (1835, 65_000), (1858, 85_000)]),
        ("Rio de Janeiro", &[(1754, 25_000), (1790, 43_000), (1809, 90_000), (1835, 150_000), (1858, 270_000)]),
        ("São Paulo", &[(1754, 5_000), (1790, 8_000), (1809, 15_000), (1835, 22_000), (1858, 30_000)]),
        ("Washington", &[(1790, 3_000), (1809, 8_000), (1835, 24_000), (1858, 61_000)]),
        ("Chicago", &[(1835, 4_000), (1858, 90_000)]),
        ("Sitka", &[(1809, 500), (1835, 800), (1858, 1_200)]),
        ("Kodiak", &[(1754, 300), (1790, 400), (1809, 500), (1835, 600), (1858, 700)]),
        ("Reykjavik", &[(1754, 300), (1790, 500), (1809, 700), (1835, 1_000), (1858, 1_500)]),
        ("Godthaab", &[(1754, 150), (1790, 250), (1809, 350), (1835, 450), (1858, 500)]),
        ("York Factory", &[(1754, 150), (1790, 200), (1809, 250), (1835, 300), (1858, 300)]),
        // Native settlements — realistic band/town sizes, roughly stable.
        ("Onondaga", &[(1754, 1_500), (1790, 1_000), (1809, 800)]),
        ("Norridgewock", &[(1754, 600), (1790, 300)]),
        ("Comancheria", &[(1754, 2_000), (1790, 2_200), (1809, 2_000), (1835, 1_500)]),
        ("Mdewakantonwan", &[(1754, 1_200), (1790, 1_300), (1809, 1_200), (1835, 1_000)]),
    ];
    let row = TABLE.iter().find(|(n, _)| *n == name)?.1;
    let mut chosen = row[0].1;
    for (y, pop) in row {
        if year >= *y {
            chosen = *pop;
        }
    }
    Some(chosen)
}

/// The real-world year a settlement was first permanently founded on its site.
/// Covers every place that can exist in the game — scenario-start cities,
/// native towns and camps, and the later settler-founded cities. Dates mark
/// the first lasting European settlement (or, for native places, the
/// traditional/estimated establishment of the town or band on that ground).
/// Used only for display, never for simulation. Falls back to the scenario
/// start year (1754) for any unlisted name so a value is always available.
fn founding_year(name: &str) -> i32 {
    match name {
        // --- New Spain / Spanish North America & Caribbean ---
        "Mexico City" => 1325,   // Tenochtitlan; refounded by Spain 1521
        "Oaxaca" => 1529,
        "Merida" => 1542,
        "Guatemala" => 1524,     // Santiago de los Caballeros de Guatemala
        "San Jose" => 1738,
        "Belize Town" => 1638,
        "Havana" => 1519,
        "Santo Domingo" => 1496,
        "San Agustin" => 1565,   // St. Augustine, oldest city in the US
        "Albuquerque" => 1706,
        "Monterey" => 1770,
        "San Diego" => 1769,
        "Chihuahua" => 1709,
        "San Antonio" => 1718,
        // --- New France / French North America ---
        "Quebec" => 1608,
        "Montreal" => 1642,
        "Tadoussac" => 1600,
        "New Orleans" => 1718,
        "Mobile" => 1702,
        "Saint-Louis" => 1764,   // St. Louis, Missouri
        "Fort Detroit" => 1701,
        "Plaisance" => 1662,     // Placentia, Newfoundland
        "Saint-Pierre" => 1536,  // Saint-Pierre et Miquelon
        "Cayenne" => 1643,
        "Port-au-Prince" => 1749,
        // --- British North America & Caribbean ---
        "Boston" => 1630,
        "New York" => 1624,      // New Amsterdam; English 1664
        "Williamsburg" => 1632,
        "Halifax" => 1749,
        "Charleston" => 1670,
        "Richmond" => 1737,
        "Kingston" => 1692,      // Kingston, Jamaica
        "York Factory" => 1684,
        "Nassau" => 1670,
        "George Town" => 1700,   // George Town, Cayman Islands
        // --- Russian America ---
        "Sitka" => 1799,
        "Kodiak" => 1792,
        // --- Danish North Atlantic ---
        "Godthaab" => 1728,      // Nuuk, Greenland
        "Reykjavik" => 874,      // traditional Norse settlement; town charter 1786
        // --- New Granada / Peru / Spanish South America ---
        "Caracas" => 1567,
        "Barranquilla" => 1629,
        "Panama" => 1519,
        "Bogota" => 1538,
        "Quito" => 1534,
        "Lima" => 1535,
        "Cusco" => 1100,         // Inca capital; refounded by Spain 1534
        "La Paz" => 1548,
        "Guayaquil" => 1538,
        "Medellin" => 1616,
        "Santiago" => 1541,      // Santiago de Chile
        // --- Rio de la Plata ---
        "Montevideo" => 1724,
        "Buenos Aires" => 1536,  // first founding; permanently refounded 1580
        "Asuncion" => 1537,
        // --- Portuguese Brazil ---
        "Salvador" => 1549,
        "Rio de Janeiro" => 1565,
        "São Paulo" => 1554,
        // --- Dutch ---
        "Paramaribo" => 1613,
        // --- Native towns & villages (traditional / estimated) ---
        "Onondaga" => 1450,      // Haudenosaunee capital
        "Norridgewock" => 1600,
        "Chota" => 1740,         // Cherokee "capital town"
        "Moosonee" => 1673,      // near Moose Factory trading post
        "Opaskwayak" => 1690,    // The Pas / Cree settlement
        "Chisasibi" => 1700,
        "Waskahigan" => 1700,
        "Mdewakantonwan" => 1700,
        "Wahpekute" => 1700,
        // --- Native mobile camps (approx. band presence on the plains) ---
        "Comancheria" => 1700,
        "Quahadi Camp" => 1700,
        "Penateka Camp" => 1700,
        "Oglala Camp" => 1720,
        // --- Later settler-founded cities (match their appearance years) ---
        "Washington" => 1790,
        "Austin" => 1839,
        "Victoria" => 1843,
        "Ottawa" => 1826,        // Bytown; renamed Ottawa 1855
        "Chicago" => 1833,
        "York" => 1793,          // York, Upper Canada; renamed Toronto 1834
        // Fallback: the scenario start year.
        _ => 1754,
    }
}

fn max_troops(c: &City) -> i32 {
    let base = if c.is_camp {
        30
    } else if c.is_village {
        50
    } else if c.is_capital {
        200
    } else {
        100
    };
    base + c.tier * 25
}

include!("sim_tick.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_game_loads_the_1754_world() {
        let game = Game::new();
        assert!(game.cities.len() > 30);
        assert_eq!(game.year, 1754);
        assert!(game.factions.iter().any(|faction| faction == "Rupert's Land"));
        assert!(game.materials.contains_key("France"));
        assert!(game.cities.iter().any(|city| city.name == "Quebec" && city.owner == "France" && city.is_capital));
        assert!(game.cities.iter().any(|city| city.name == "Boston" && city.owner == "Great Britain"));
    }

    #[test]
    fn probe_mobile_region() {
        let images = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("images");
        let maps = Maps::load(&images).expect("map images");
        // Mobile is stored unscaled in build_cities, then multiplied by COORD_SCALE,
        // so the live map position is (228, 283) * COORD_SCALE. Confirm it lands in 301.
        let s = crate::config::COORD_SCALE;
        let (mx, my) = (228.0f32, 283.0f32);
        let region = maps.region_at(mx * s, my * s);
        println!("MOBILE raw ({},{}) -> scaled ({:.1},{:.1}) -> region {}", mx, my, mx * s, my * s, region);
        assert_eq!(region, 301, "Mobile should sit in territory 301");
    }

    #[test]
    fn portuguese_cities_sit_in_their_territories() {
        let images = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("images");
        let maps = Maps::load(&images).expect("map images");
        let s = crate::config::COORD_SCALE;
        // Read the live city coords from build_cities so the test guards the
        // actual data rather than a hand-copied duplicate. Expected territories:
        // Salvador #542, São Paulo #584, Rio de Janeiro #586.
        let game = Game::new();
        let want = [("Salvador", 542), ("São Paulo", 584), ("Rio de Janeiro", 586)];
        for (name, want) in want {
            let c = game
                .cities
                .iter()
                .find(|c| c.name == name)
                .unwrap_or_else(|| panic!("{name} missing from world"));
            // c.x / c.y are already scaled by COORD_SCALE in build_cities.
            let region = maps.region_at(c.x, c.y);
            assert_eq!(region, want, "{name} should sit in territory {want}, got {region}");
        }
        let _ = s;
    }

    #[test]
    fn every_scenario_ticks_without_panicking() {
        let images = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("images");
        let maps = Maps::load(&images).expect("map images");
        assert!(maps.region_count > 50, "territories {}", maps.region_count);
        assert_eq!(maps.overlay_w, MAP_W);
        assert_eq!(maps.overlay_h, MAP_H);
        let pixels = maps.overlay(&vec![None; maps.region_count as usize]);
        let borders = pixels.chunks_exact(4).filter(|px| *px == [0, 0, 0, 110]).count();
        assert!(borders > 1000, "border pixels {borders}");
        for year in [1754, 1790, 1809, 1835, 1858] {
            let mut game = Game::new();
            game.start_scenario(year, &maps);
            assert_eq!(game.year, year);
            if year == 1754 {
                assert!(game.cities.iter().any(|city| city.name == "Quebec" && city.region >= 0));
            }
            for _ in 0..DAY_TICKS * 40 {
                game.tick(&maps);
            }
            assert!(!game.cities.is_empty(), "{year}");
        }
    }
}

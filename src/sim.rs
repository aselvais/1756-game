//! Game rules ported from `index.py`: time, economy, armies, trade, diplomacy,
//! disasters, and the historical scenario events.

use crate::config::*;
use crate::world::Maps;
use rand::Rng;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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

#[derive(Clone)]
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
}

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
    pub siege_timer: i32,
    pub siege_phase: u8,
    pub phase_timer: i32,
    pub battle_phase: u8,
    pub skirmish_ticks: i32,
    pub alive: bool,
    pub ship: bool,
}

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

pub struct Settler {
    pub faction: String,
    pub x: f32,
    pub y: f32,
    pub tx: f32,
    pub ty: f32,
    pub native: bool,
    pub alive: bool,
}

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
    wars: HashSet<Pair>,
    war_names: HashMap<Pair, (String, String)>,
    treaties: HashMap<Pair, i32>,
    treaty_cd: HashMap<Pair, i32>,
    pressure: HashMap<Pair, i32>,
    alliances: HashMap<Pair, i32>,
    refused: HashMap<Pair, i32>,
    fatigue: HashMap<Pair, HashMap<String, i32>>,
    puppets: HashMap<String, String>,
    pub news: VecDeque<String>,
    pub paper: Option<(String, String)>,
    pub claims_dirty: bool,
    next_id: u32,
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
            claims_dirty: true,
            next_id: 1,
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
        for c in &mut self.cities {
            c.troops = max_troops(c);
            c.region = maps.region_at(c.x, c.y);
        }
        self.assign_buildings();
        self.init_population();
        self.init_manpower();
        self.claims_dirty = true;
        self.news(format!("Scenario {} begins.", self.year));
    }

    pub fn tick(&mut self, maps: &Maps) {
        if self.speed <= 0 {
            return;
        }
        self.day_timer += self.speed;
        if self.day_timer >= DAY_TICKS {
            self.day_timer = 0;
            self.advance_date(maps);
            self.claims_dirty = true;
        }
        self.spawn_armies(maps);
        self.material_timer += self.speed;
        if self.material_timer >= DAY_TICKS {
            self.material_timer = 0;
            self.gold_tax(maps);
            self.heal_garrisons();
            self.update_floods();
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
                };
            if let Some(year) = founded {
                g.founded.push((year, city));
            } else {
                g.cities.push(city);
            }
        };
        // Scaled colonial cities (coordinates match cities_data.make_city inputs).
        let cities = [
            ("Mexico City", 171.0, 340.0, "Spain", true, false),
            ("Havana", 260.0, 318.0, "Spain", false, false),
            ("Oaxaca", 187.0, 354.0, "Spain", false, false),
            ("Merida", 220.0, 334.0, "Spain", false, false),
            ("Albuquerque", 145.0, 258.0, "Spain", false, false),
            ("San Agustin", 256.0, 284.0, "Spain", false, false),
            ("Monterey", 80.0, 234.0, "Spain", false, false),
            ("San Diego", 95.0, 258.0, "Spain", false, false),
            ("Chihuahua", 145.0, 290.0, "Spain", false, false),
            ("Caracas", 343.0, 370.0, "Spain", false, false),
            ("Barranquilla", 301.0, 371.0, "Spain", false, false),
            ("Santo Domingo", 317.0, 328.0, "Spain", false, false),
            ("Quebec", 280.0, 180.0, "France", true, false),
            ("Montreal", 272.0, 189.0, "France", false, false),
            ("New Orleans", 217.0, 286.0, "France", false, false),
            ("Saint-Louis", 212.0, 242.0, "France", false, false),
            ("Fort Detroit", 241.0, 218.0, "France", false, false),
            ("Port-au-Prince", 306.0, 331.0, "France", false, false),
            ("Plaisance", 332.0, 161.0, "France", false, false),
            ("Boston", 287.0, 208.0, "Great Britain", true, false),
            ("New York", 280.0, 218.0, "Great Britain", false, false),
            ("Halifax", 309.0, 186.0, "Great Britain", false, false),
            ("Charleston", 262.0, 264.0, "Great Britain", false, false),
            ("Richmond", 267.0, 237.0, "Great Britain", false, false),
            ("Kingston", 285.0, 338.0, "Great Britain", false, false),
            ("York Factory", 202.0, 143.0, "Rupert's Land", true, false),
            ("Nassau", 276.0, 302.0, "Great Britain", false, false),
            ("Belize Town", 228.0, 350.0, "Great Britain", false, false),
            ("Guatemala", 216.0, 364.0, "Spain", false, false),
            ("San Jose", 249.0, 382.0, "Spain", false, false),
            ("Sitka", 84.0, 120.0, "Russia", true, false),
            ("Kodiak", 52.0, 91.0, "Russia", false, false),
            ("Godthaab", 284.0, 81.0, "Denmark", true, false),
            ("Reykjavik", 317.0, 46.0, "Denmark", false, false),
            ("San Antonio", 174.0, 293.0, "Spain", false, false),
        ];
        for (n, x, y, o, cap, _v) in cities {
            add(self, n, x, y, o, cap, false, false, false, if cap { 200 } else { 100 }, false, None);
        }
        let villages = [
            ("Tadoussac", 283.0, 171.0, "France"),
            ("Williamsburg", 272.0, 239.0, "Great Britain"),
            ("Onondaga", 265.0, 211.0, "Iroquois"),
            ("Norridgewock", 287.0, 192.0, "Wabanaki"),
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
        for f in ["Spain", "France", "Great Britain", "Russia", "Denmark", "Rupert's Land", "Iroquois", "Wabanaki", "Comanche", "Cree", "Dakota"] {
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
            "France", "Spain", "Great Britain", "Russia", "Denmark", "Iroquois", "Wabanaki",
            "Comanche", "Cree", "Dakota", "Pirates", "Rupert's Land",
        ];
        for n in names {
            self.factions.push(n.into());
            self.touch(n);
        }
        let gb_ir = Pair::new("Great Britain", "Iroquois");
        let fr_wa = Pair::new("France", "Wabanaki");
        self.alliances.insert(gb_ir, DAY_TICKS * 1095);
        self.alliances.insert(fr_wa, DAY_TICKS * 1095);
        self.war_names.insert(gb_ir, ("Great Britain".into(), "Iroquois".into()));
        self.war_names.insert(fr_wa, ("France".into(), "Wabanaki".into()));
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
            ("York Factory", "Rupert's Land", true),
            ("Quebec", "Great Britain", false),
            ("Montreal", "Great Britain", false),
            ("Tadoussac", "Great Britain", false),
            ("Plaisance", "Great Britain", false),
            ("Nassau", "Great Britain", false),
            ("Kingston", "Great Britain", false),
            ("Halifax", "Great Britain", false),
            ("Mexico City", "Spain", true),
            ("Havana", "Spain", false),
            ("Merida", "Spain", false),
            ("Albuquerque", "Spain", false),
            ("San Agustin", "Spain", false),
            ("Monterey", "Spain", false),
            ("Caracas", "Spain", false),
            ("Port-au-Prince", "France", false),
            ("San Antonio", "Spain", false),
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
            ("York Factory", "Rupert's Land", true),
            ("Quebec", "Great Britain", false),
            ("Montreal", "Great Britain", false),
            ("Tadoussac", "Great Britain", false),
            ("Plaisance", "Great Britain", false),
            ("Nassau", "Great Britain", false),
            ("Kingston", "Great Britain", false),
            ("Halifax", "Great Britain", false),
            ("Mexico City", "Spain", true),
            ("Havana", "Spain", false),
            ("Merida", "Spain", false),
            ("Albuquerque", "Spain", false),
            ("San Agustin", "Spain", false),
            ("Monterey", "Spain", false),
            ("San Diego", "Spain", false),
            ("Chihuahua", "Spain", false),
            ("Caracas", "Spain", false),
            ("Port-au-Prince", "Haiti", true),
            ("Saint-Pierre", "France", true),
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
            ("York Factory", "Rupert's Land", true),
            ("Quebec", "Great Britain", false),
            ("Montreal", "Great Britain", false),
            ("Tadoussac", "Great Britain", false),
            ("Plaisance", "Great Britain", false),
            ("Nassau", "Great Britain", false),
            ("Kingston", "Great Britain", false),
            ("Havana", "Spain", true),
            ("Caracas", "Spain", false),
            ("Saint-Pierre", "France", true),
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
            ("York Factory", "Rupert's Land", true),
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
            let mut pop = pop_override(&c.name, year).unwrap_or_else(|| {
                let base = if c.is_camp {
                    150
                } else if c.is_fort {
                    200
                } else if c.is_village {
                    800
                } else if c.is_capital {
                    12000
                } else {
                    5000
                };
                (base as f32 * year_mult(year)) as i32
            });
            pop = (pop as f32 * rng.gen_range(0.85..1.15)) as i32;
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

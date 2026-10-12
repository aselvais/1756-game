impl Game {
    pub fn season(&self) -> &'static str {
        match self.month {
            12 | 1 | 2 => "Winter",
            3 | 4 | 5 => "Spring",
            6 | 7 | 8 => "Summer",
            _ => "Autumn",
        }
    }

    pub fn date_label(&self) -> String {
        format!("{} {} {}", self.day, MONTHS[(self.month - 1) as usize], self.year)
    }

    /// Map color for a faction's territory. A sovereign nation uses its own
    /// palette color; a puppet instead shows a noticeably different SHADE of
    /// its overlord's color, so it reads as part of the overlord's bloc while
    /// staying distinguishable (and distinct from sibling puppets).
    fn display_color(&self, faction: &str) -> [u8; 3] {
        match self.puppets.get(faction) {
            Some(overlord) => {
                let base = faction_color(overlord);
                // Deterministic per-puppet shift: a stable hash picks a
                // brightness factor so each puppet of the same overlord gets a
                // clearly different shade, biased darker so the overlord stays
                // the "brightest" of its bloc.
                let h = name_seed(faction);
                // Factor in roughly 0.45..=0.80 of the overlord's brightness.
                let f = 0.45 + (h % 36) as f32 / 100.0;
                let shade = |c: u8| (c as f32 * f).round().clamp(0.0, 255.0) as u8;
                // Nudge one channel the other way so same-brightness picks still
                // differ in hue, keeping siblings apart.
                let bump = ((h / 7) % 3) as usize;
                let mut out = [shade(base[0]), shade(base[1]), shade(base[2])];
                out[bump] = (out[bump] as u16 + 40).min(255) as u8;
                out
            }
            None => faction_color(faction),
        }
    }

    pub fn region_owners(&self, count: i32) -> Vec<Option<[u8; 4]>> {
        let n = count.max(0) as usize;
        // A region is owned by whichever nation most recently settled it. Each
        // settler-founded city carries a monotonic `founded_order`; the highest
        // order in a region wins, so a later settler takes the region from an
        // earlier one. Scenario-start cities all share order 0, so among those a
        // capital wins the tie and otherwise the first one found stands.
        let mut best: Vec<Option<(u64, bool, String)>> = vec![None; n];
        for c in &self.cities {
            if c.region < 0 || c.region as usize >= n {
                continue;
            }
            // Mobile tribes (Comanche, Cree, Dakota) keep camps and villages but
            // do not claim territory, so their settlements never color a region.
            if !claims_territory(&c.owner) {
                continue;
            }
            let slot = &mut best[c.region as usize];
            let takes = match slot {
                None => true,
                Some((order, cap, _)) => {
                    c.founded_order > *order || (c.founded_order == *order && c.is_capital && !*cap)
                }
            };
            if takes {
                *slot = Some((c.founded_order, c.is_capital, c.owner.clone()));
            }
        }
        // Resolve each region to an owning nation name: a city in the region
        // wins (so conquering a city still recolors its region as you play);
        // otherwise an authored starting claim from the scenario map applies.
        (0..n)
            .map(|r| {
                let name = best[r]
                    .as_ref()
                    .map(|(_, _, name)| name.clone())
                    .or_else(|| self.region_claims.get(&(r as i32)).cloned());
                name.map(|name| {
                    let c = self.display_color(&name);
                    [c[0], c[1], c[2], 130]
                })
            })
            .collect()
    }

    /// The nation owning a region: the claims-territory city with the highest
    /// `founded_order` in that region (capital breaks ties), else an authored
    /// `region_claims` entry. Mirrors `region_owners` but returns the name.
    fn region_owner_name(&self, region: i32) -> Option<String> {
        if region < 0 {
            return None;
        }
        let mut best: Option<(u64, bool, &str)> = None;
        for c in &self.cities {
            if c.region != region || !claims_territory(&c.owner) {
                continue;
            }
            let takes = match best {
                None => true,
                Some((order, cap, _)) => {
                    c.founded_order > order || (c.founded_order == order && c.is_capital && !cap)
                }
            };
            if takes {
                best = Some((c.founded_order, c.is_capital, c.owner.as_str()));
            }
        }
        best.map(|(_, _, name)| name.to_string())
            .or_else(|| self.region_claims.get(&region).cloned())
    }

    /// True if the region containing city `idx` touches a region owned by a
    /// different nation (or by no one) — i.e. it sits on the national frontier.
    fn city_on_border(&self, idx: usize, maps: &Maps) -> bool {
        let region = self.cities[idx].region;
        if region < 0 {
            return false;
        }
        let owner = self.cities[idx].owner.clone();
        self.border_region_from(region, &owner, maps).is_some()
    }

    /// Given one of a faction's regions, return it if it borders foreign/empty
    /// land. Helper so a guard can confirm its anchor is still a frontier.
    fn border_region_from(&self, region: i32, owner: &str, maps: &Maps) -> Option<i32> {
        for &nb in maps.region_neighbors(region) {
            match self.region_owner_name(nb) {
                Some(ref n) if n == owner => continue,
                Some(ref n) if self.same_bloc(owner, n) => continue,
                _ => return Some(region),
            }
        }
        None
    }

    /// A world-space anchor point for a border-guarding unit of `faction`:
    /// the centroid of one of the faction's frontier regions (the one nearest
    /// to the fallback `(fx, fy)`, usually the home city). Falls back to the
    /// home point when the faction has no mapped border regions.
    fn guard_anchor_for(&self, faction: &str, maps: &Maps, fx: f32, fy: f32) -> (i32, (f32, f32)) {
        // Collect the faction's owned regions (from its cities + authored map).
        let mut owned: std::collections::HashSet<i32> = self
            .cities
            .iter()
            .filter(|c| c.owner == faction && c.region >= 0)
            .map(|c| c.region)
            .collect();
        for (region, nation) in &self.region_claims {
            if nation == faction {
                owned.insert(*region);
            }
        }
        let mut best: Option<(i32, (f32, f32), f32)> = None;
        for &region in &owned {
            if self.border_region_from(region, faction, maps).is_none() {
                continue;
            }
            if let Some((cx, cy)) = maps.region_centroid_world(region) {
                let d = dist(cx, cy, fx, fy);
                if best.as_ref().map(|(_, _, bd)| d < *bd).unwrap_or(true) {
                    best = Some((region, (cx, cy), d));
                }
            }
        }
        match best {
            Some((region, pt, _)) => (region, pt),
            None => (-1, (fx, fy)),
        }
    }

    fn remember(&mut self, a: &str, b: &str) -> Pair {
        let pair = Pair::new(a, b);
        self.war_names.entry(pair).or_insert_with(|| {
            if a <= b { (a.into(), b.into()) } else { (b.into(), a.into()) }
        });
        pair
    }

    fn same_bloc(&self, a: &str, b: &str) -> bool {
        if a == b {
            return true;
        }
        if self.puppets.get(a).is_some_and(|o| o == b) || self.puppets.get(b).is_some_and(|o| o == a) {
            return true;
        }
        matches!((self.puppets.get(a), self.puppets.get(b)), (Some(x), Some(y)) if x == y)
    }

    fn at_war(&self, a: &str, b: &str) -> bool {
        a != b && !self.same_bloc(a, b) && self.wars.contains(&Pair::new(a, b))
    }

    fn at_peace(&self, a: &str, b: &str) -> bool {
        !self.at_war(a, b)
    }

    fn war_count(&self, faction: &str) -> usize {
        self.wars.iter().filter(|p| self.involves(p, faction)).count()
    }

    fn involves(&self, pair: &Pair, faction: &str) -> bool {
        self.war_names.get(pair).is_some_and(|(a, b)| a == faction || b == faction)
    }

    fn declare_war(&mut self, a: &str, b: &str, reason: &str) {
        if self.same_bloc(a, b) || self.war_count(a) >= 3 || self.war_count(b) >= 3 {
            return;
        }
        let key = self.remember(a, b);
        if self.alliances.remove(&key).is_some() {
            self.news(format!("Alliance between {a} & {b} broken!"));
            self.cue(Cue::Break);
        }
        self.treaties.remove(&key);
        self.wars.insert(key);
        if !reason.is_empty() {
            self.news(reason);
        }
        self.cue(Cue::War);
    }

    fn end_war(&mut self, a: &str, b: &str) {
        let key = Pair::new(a, b);
        self.wars.remove(&key);
    }

    fn make_treaty(&mut self, a: &str, b: &str, reason: &str) {
        let key = self.remember(a, b);
        if self.treaty_cd.contains_key(&key) {
            return;
        }
        let mut rng = rand::thread_rng();
        self.treaties.insert(key, rng.gen_range(TREATY_MIN..=TREATY_MAX));
        self.pressure.insert(key, 0);
        self.alliances.remove(&key);
        self.end_war(a, b);
        let names = [a.to_string(), b.to_string()];
        for u in &mut self.units {
            if names.contains(&u.faction) {
                if let Some(city) = self.cities.iter().find(|c| c.id == u.target) {
                    if names.contains(&city.owner) && city.owner != u.faction {
                        u.idle = true;
                        u.retreating = false;
                        u.opponent = None;
                    }
                }
            }
        }
        self.resolve_occupied(&names[0], &names[1]);
        self.peace_events(&names[0], &names[1]);
        if !reason.is_empty() {
            self.news(reason);
        }
        self.cue(Cue::Peace);
    }

    fn resolve_occupied(&mut self, a: &str, b: &str) {
        let occupied: Vec<u32> = self
            .cities
            .iter()
            .filter(|c| {
                c.owner != c.sovereign && ((c.owner == a && c.sovereign == b) || (c.owner == b && c.sovereign == a))
            })
            .map(|c| c.id)
            .collect();
        if occupied.is_empty() {
            return;
        }
        let cities_a = self.cities.iter().filter(|c| c.owner == a).count();
        let cities_b = self.cities.iter().filter(|c| c.owner == b).count();
        let (victor, loser) = if cities_a >= cities_b { (a, b) } else { (b, a) };
        let big = self.cities.iter().filter(|c| c.sovereign == loser).count() > 3;
        let roll = rand::thread_rng().gen_range(0..4);
        let annex = !big && roll == 2;
        // A sizable, already-free loser can be made a puppet instead of a plain
        // peace: the occupied cities return but the nation is subordinated.
        let can_puppet = big && !self.puppets.contains_key(loser) && self.overlord_of(victor).is_none();
        let puppet = can_puppet && roll == 0;
        let gold = rand::thread_rng().gen_range(10..80);
        if puppet {
            for c in &mut self.cities {
                if occupied.contains(&c.id) {
                    let sov = c.sovereign.clone();
                    c.owner = sov;
                }
            }
            self.make_puppet(victor, loser);
        } else if annex {
            for c in &mut self.cities {
                if occupied.contains(&c.id) {
                    c.sovereign = c.owner.clone();
                }
            }
            self.news(format!("{victor} annexes occupied territory from {loser}!"));
            self.cue(Cue::Annex);
        } else {
            let keep = if roll == 3 { occupied.len().min(2) } else { 0 };
            for (i, id) in occupied.iter().enumerate() {
                if let Some(c) = self.cities.iter_mut().find(|c| c.id == *id) {
                    if i < keep {
                        c.sovereign = c.owner.clone();
                    } else {
                        let sov = c.sovereign.clone();
                        c.owner = sov;
                    }
                }
            }
            self.transfer(loser, victor, "Gold", gold);
            self.news(format!("{victor} and {loser} settle the peace."));
        }
    }

    fn transfer(&mut self, from: &str, to: &str, mat: &str, amount: i32) {
        let have = self.materials.get(from).map(|s| s.get(mat)).unwrap_or(0);
        let paid = have.min(amount);
        if let Some(s) = self.materials.get_mut(from) {
            s.add(mat, -paid);
        }
        self.touch(to);
        if let Some(s) = self.materials.get_mut(to) {
            s.add(mat, paid);
        }
    }

    fn peace_events(&mut self, a: &str, b: &str) {
        if self.revolution_fired && (a == "United States" || b == "United States") {
            let other = if a == "United States" { b } else { a };
            for c in &mut self.cities {
                if matches!(c.name.as_str(), "Charleston" | "Williamsburg" | "Richmond") && c.owner == other {
                    c.owner = "United States".into();
                    c.sovereign = "United States".into();
                }
            }
        }
        if self.mexico_fired && ((a == "Mexico" && b == "Spain") || (b == "Mexico" && a == "Spain")) {
            for c in &mut self.cities {
                if matches!(c.name.as_str(), "Merida" | "Albuquerque" | "San Diego" | "Monterey" | "San Antonio") && c.owner == "Spain" {
                    c.owner = "Mexico".into();
                    c.sovereign = "Mexico".into();
                }
            }
        }
        if self.confederate_fired && ((a == "Confederate States" && b == "United States") || (b == "Confederate States" && a == "United States")) {
            let us = self.cities.iter().filter(|c| c.owner == "United States").count();
            let cs = self.cities.iter().filter(|c| c.owner == "Confederate States").count();
            if us >= cs {
                for c in &mut self.cities {
                    if c.owner == "Confederate States" {
                        c.owner = "United States".into();
                        c.sovereign = "United States".into();
                    }
                }
                self.eliminated.insert("Confederate States".into());
                self.news("The Union is restored!");
                self.cue(Cue::Fanfare);
            } else {
                self.news("The Confederate States maintain their independence!");
                self.cue(Cue::Fanfare);
            }
        }
    }

    fn advance_date(&mut self, maps: &Maps) {
        self.day += 1;
        if self.day > MONTH_DAYS[(self.month - 1) as usize] {
            self.day = 1;
            self.month += 1;
            if self.month > 12 {
                self.month = 1;
                self.year += 1;
            }
            self.monthly_growth();
            self.monthly_manpower();
        }
        let (d, m, y) = (self.day, self.month, self.year);
        if d == 19 && m == 4 && y == 1775 {
            self.american_revolution();
        }
        self.found_due();
        for c in &mut self.cities {
            c.region = maps.region_at(c.x, c.y);
        }
        if d == 16 && m == 7 && y == 1790 {
            for c in &mut self.cities {
                if c.owner == "United States" {
                    c.is_capital = c.name == "Washington";
                }
            }
            self.news("Washington is established as the capital of the United States!");
        }
        if d == 6 && m == 3 && y == 1834 {
            for c in &mut self.cities {
                if c.name == "York" {
                    c.name = "Toronto".into();
                    self.news("York is renamed Toronto.");
                    break;
                }
            }
        }
        if d == 1 && m == 1 && y == 1785 {
            self.news("Spain adopts a new royal banner!");
        }
        if d == 21 && m == 9 && y == 1792 && !self.eliminated.contains("France") {
            self.news("France abolishes the monarchy — the French Republic is proclaimed!");
            self.paper("French Revolution!", "The French monarchy is abolished. The First French Republic is proclaimed.");
            self.cue(Cue::Fanfare);
        }
        if d == 30 && m == 4 && y == 1803 {
            for c in &mut self.cities {
                if matches!(c.name.as_str(), "New Orleans" | "Saint-Louis")
                    && (c.owner == "France" || c.owner == "Louisiana")
                {
                    c.owner = "United States".into();
                    c.sovereign = "United States".into();
                    c.is_capital = false;
                }
            }
            // The French Louisiana puppet is dissolved into the United States.
            self.puppets.remove("Louisiana");
            self.eliminated.insert("Louisiana".into());
            self.news("Louisiana Purchase! France sells New Orleans and Saint-Louis to the United States.");
            self.paper("Louisiana Purchase!", "France sells the Louisiana Territory to the United States.");
            self.cue(Cue::Fanfare);
        }
        if d == 1 && m == 1 && y == 1804 {
            self.haitian_independence();
        }
        if d == 16 && m == 9 && y == 1810 {
            self.mexican_independence();
        }
        if d == 2 && m == 3 && y == 1836 {
            self.texas_independence();
        }
        if d == 3 && m == 3 && y == 1845 {
            let mut transferred = false;
            for c in &mut self.cities {
                if c.name == "San Agustin" && c.owner != "United States" {
                    c.owner = "United States".into();
                    c.sovereign = "United States".into();
                    transferred = true;
                }
            }
            if transferred {
                self.news("Florida becomes a United States territory!");
            }
        }
        if d == 8 && m == 2 && y == 1861 {
            self.confederate_states();
        }
        if d == 30 && m == 3 && y == 1867 {
            for c in &mut self.cities {
                if matches!(c.name.as_str(), "Sitka" | "Kodiak") && c.owner == "Russia" {
                    c.owner = "United States".into();
                    c.sovereign = "United States".into();
                }
            }
            self.news("Alaska Purchase! The United States buys Alaska from Russia.");
            self.paper("Alaska Purchase!", "The United States purchases Alaska. Sitka and Kodiak are now American.");
            self.cue(Cue::Fanfare);
        }
        if d == 1 && m == 7 && y == 1867 {
            self.canadian_confederation();
        }
    }

    fn monthly_growth(&mut self) {
        for c in &mut self.cities {
            let mut rate = if c.is_capital {
                0.003
            } else if c.is_fort || c.is_camp {
                0.001
            } else if c.is_village {
                0.0015
            } else {
                0.0025
            };
            if c.happiness >= 75 {
                rate *= 1.3;
            } else if c.happiness < 25 {
                rate *= -0.2;
            } else if c.happiness < 50 {
                rate *= 0.5;
            }
            c.pop_acc += c.population as f32 * rate;
            if c.pop_acc.abs() >= 1.0 {
                let add = c.pop_acc as i32;
                c.population = (c.population + add).max(50);
                c.pop_acc -= add as f32;
            }
        }
        // Drift each city's happiness toward a target shaped by its laws,
        // government, demographics, wars, and status — so happiness reflects
        // the whole game state instead of always rising to 80.
        let targets: Vec<(usize, i32)> =
            (0..self.cities.len()).map(|i| (i, self.happiness_target(i))).collect();
        for (i, target) in targets {
            let c = &mut self.cities[i];
            if c.happiness < target {
                c.happiness = (c.happiness + 1).min(target);
            } else if c.happiness > target {
                c.happiness = (c.happiness - 1).max(target);
            }
            c.happiness = c.happiness.clamp(0, 100);
        }
    }

    /// The enfranchised share of a settlement: the fraction of people who get a
    /// political voice. Ethnic groups that are Segregated/Suppressed/Enslaved
    /// (status rank >= 2) are excluded, as are disenfranchised commoners under
    /// non-republican governments. Always leaves a small floor so a town is
    /// never fully voiceless.
    fn electorate_share(&self, i: usize) -> f32 {
        let c = &self.cities[i];
        let nation = if c.sovereign.is_empty() { &c.owner } else { &c.sovereign };
        // Sum the share of ethnic groups whose legal status allows a voice.
        let mut enfranchised: f32 = c
            .ethnicity
            .iter()
            .filter(|(k, _)| ethnic_status_rank(nation, k, self.year) < 2)
            .map(|(_, s)| *s)
            .sum();
        // Under a constitutional monarchy, commoners have a narrower franchise.
        let (gov_key, _) = government_of(nation, self.year);
        if gov_key == "constitutionalmonarchy" {
            // Only aristocracy plus a slice of commoners vote.
            let voting = c.aristocracy + (1.0 - c.aristocracy) * 0.4;
            enfranchised *= voting;
        }
        enfranchised.clamp(0.05, 1.0)
    }

    /// A settlement's party favor as four shares summing to 1.0, scored from
    /// local conditions (taxation, rights, war, trade, tradition) against each
    /// party's stance. Aristocracy tilts toward tradition/order; commoners
    /// toward reform.
    fn settlement_party_favor(&self, i: usize) -> [f32; 4] {
        let c = &self.cities[i];
        let nation = if c.sovereign.is_empty() { &c.owner } else { &c.sovereign };
        let laws = self.laws.get(nation);
        let taxation = laws.and_then(|l| l.get(3).copied()).unwrap_or(2);
        let ethnic_law = laws.and_then(|l| l.get(0).copied()).unwrap_or(0);
        let religion_law = laws.and_then(|l| l.get(2).copied()).unwrap_or(0);

        // Condition signals in roughly [-1, 1].
        // Desire for tax relief rises with heavier taxation.
        let tax_relief = match taxation {
            0 => -0.8,
            1 => -0.3,
            2 => 0.2,
            _ => 1.0,
        };
        // Desire for rights/reform rises where laws are repressive.
        let rights = (ethnic_law.max(religion_law) as f32 / 3.0) * 2.0 - 0.3;
        // War hawk sentiment when the nation is at war.
        let at_war = self.wars.iter().any(|p| self.war_names.get(p).is_some_and(|(a, b)| a == nation || b == nation));
        let war_hawk = if at_war { 0.8 } else { -0.4 };
        // Trade/commercial character from development and markets.
        let trade = (c.tier as f32 / 3.0) + if c.buildings.iter().any(|b| b == "Market") { 0.4 } else { 0.0 } - 0.4;
        // Tradition pull from the aristocracy fraction.
        let tradition = c.aristocracy * 4.0 - 0.4;

        let mut scores = [0.0f32; 4];
        for (p, score) in scores.iter_mut().enumerate() {
            let (w_tax, w_rights, w_war, w_trade, w_trad) = party_stance(p);
            *score = w_tax * tax_relief
                + w_rights * rights
                + w_war * war_hawk
                + w_trade * trade
                + w_trad * tradition;
        }
        // Softmax-ish: shift to positive and normalize.
        let min = scores.iter().cloned().fold(f32::INFINITY, f32::min);
        let mut total = 0.0;
        for s in &mut scores {
            *s = (*s - min + 0.5).max(0.0);
            total += *s;
        }
        if total > 0.0 {
            for s in &mut scores {
                *s /= total;
            }
        } else {
            scores = [0.25; 4];
        }
        scores
    }

    /// Public lookup of a settlement's party favor by city id, for the UI.
    pub fn settlement_party_favor_of(&self, id: u32) -> Option<[f32; 4]> {
        let i = self.cities.iter().position(|c| c.id == id)?;
        Some(self.settlement_party_favor(i))
    }

    /// The national party pie: each settlement's favor weighted by its
    /// enfranchised population. Returns four shares summing to ~1.0.
    pub fn nation_party_pie(&self, nation: &str) -> [f32; 4] {
        let mut acc = [0.0f32; 4];
        let mut weight_total = 0.0f32;
        for i in 0..self.cities.len() {
            let c = &self.cities[i];
            let n = if c.sovereign.is_empty() { &c.owner } else { &c.sovereign };
            if n != nation {
                continue;
            }
            let w = c.population as f32 * self.electorate_share(i);
            if w <= 0.0 {
                continue;
            }
            let favor = self.settlement_party_favor(i);
            for p in 0..4 {
                acc[p] += favor[p] * w;
            }
            weight_total += w;
        }
        if weight_total > 0.0 {
            for a in &mut acc {
                *a /= weight_total;
            }
        }
        acc
    }

    /// Holds elections for every elective nation: the party with the highest
    /// national pie share takes power. Non-elective nations are skipped.
    fn run_elections(&mut self) {
        let nations: Vec<String> = {
            let mut set: Vec<String> = Vec::new();
            for c in &self.cities {
                let n = if c.sovereign.is_empty() { &c.owner } else { &c.sovereign };
                if !n.is_empty() && !set.contains(n) {
                    set.push(n.clone());
                }
            }
            set
        };
        for nation in nations {
            let (gov_key, _) = government_of(&nation, self.year);
            if !is_elective(gov_key) {
                self.ruling_party.remove(&nation);
                continue;
            }
            let pie = self.nation_party_pie(&nation);
            let winner = (0..4).max_by(|a, b| pie[*a].partial_cmp(&pie[*b]).unwrap()).unwrap_or(0);
            let prev = self.ruling_party.insert(nation.clone(), winner);
            if prev != Some(winner) {
                let party = parties_of(&nation)[winner];
                self.news(format!("{nation} election: the {party} party takes power!"));
                self.cue(Cue::Election);
            }
        }
    }

    /// Gold-tax income multiplier from a nation's Taxation law (category 3):
    /// None 0.0, Low 0.5, Standard 1.0, Heavy 1.5.
    fn tax_multiplier(&self, nation: &str) -> f32 {
        match self.laws.get(nation).and_then(|l| l.get(3).copied()).unwrap_or(2) {
            0 => 0.0,
            1 => 0.5,
            3 => 1.5,
            _ => 1.0,
        }
    }

    /// Public lookup of a city's happiness target by id, for the UI.
    pub fn happiness_target_of(&self, id: u32) -> Option<i32> {
        let i = self.cities.iter().position(|c| c.id == id)?;
        Some(self.happiness_target(i))
    }

    /// The happiness level a settlement is drifting toward, from a neutral base
    /// adjusted by: government type, the ethnic/religion laws as they affect the
    /// groups actually present, whether the nation is at war, puppet status,
    /// city development, and the ruling nation matching the local majority.
    fn happiness_target(&self, i: usize) -> i32 {
        let c = &self.cities[i];
        let nation = if c.sovereign.is_empty() { &c.owner } else { &c.sovereign };
        let mut t: f32 = 55.0;

        // Government type baseline.
        let (gov_key, _) = government_of(nation, self.year);
        t += match gov_key {
            "republic" => 10.0,
            "constitutionalmonarchy" => 6.0,
            "confederation" => 4.0,
            "colonialgovernment" => 0.0,
            "tribal" => 6.0,
            "monarchy" => -6.0,
            "militarygovernment" => -10.0,
            _ => 0.0,
        };

        // Laws, weighted by the share of the population they actually affect.
        let laws = self.laws.get(nation);
        let ethnic_law = laws.and_then(|l| l.get(0).copied()).unwrap_or(0);
        let religion_law = laws.and_then(|l| l.get(2).copied()).unwrap_or(0);
        // Ethnic law: harsher regimes anger the share they oppress.
        let per_level_eth = match ethnic_law {
            0 => 10.0,  // Protected rights lift everyone
            1 => -4.0,  // Segregated
            2 => -14.0, // Suppressed
            _ => -26.0, // Enslaved
        };
        // Religion law hits non-state faiths.
        let per_level_rel = match religion_law {
            0 => 8.0,
            1 => -2.0,
            2 => -12.0,
            _ => -22.0,
        };
        if ethnic_law == 0 {
            t += per_level_eth;
        } else {
            // Weight by the minority (non-ruling) share.
            let ruling = owner_ethnicity_key(nation);
            let minority: f32 = c.ethnicity.iter().filter(|(k, _)| k != ruling).map(|(_, s)| *s).sum();
            t += per_level_eth * minority;
        }
        if religion_law == 0 {
            t += per_level_rel;
        } else {
            let state = owner_religion_key(nation);
            let dissent: f32 = c.religion.iter().filter(|(k, _)| k != state).map(|(_, s)| *s).sum();
            t += per_level_rel * dissent;
        }

        // Conscription law weighs on the people.
        let conscription = laws.and_then(|l| l.get(1).copied()).unwrap_or(0);
        t += match conscription {
            0 => 6.0,
            1 => 0.0,
            2 => -5.0,
            _ => -12.0,
        };

        // Taxation law: lighter taxes please the people, heavy taxes anger them.
        let taxation = laws.and_then(|l| l.get(3).copied()).unwrap_or(2);
        t += match taxation {
            0 => 15.0,
            1 => 8.0,
            2 => 0.0,
            _ => -15.0,
        };

        // The ruling ethnicity matching the local majority is reassuring; a
        // foreign ruling class over a different majority breeds resentment.
        if let Some((top_key, top_share)) = c
            .ethnicity
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        {
            if top_key == owner_ethnicity_key(nation) {
                t += 6.0 * top_share;
            } else {
                t -= 8.0 * top_share;
            }
        }

        // At war: war-weariness lowers morale everywhere.
        if self.wars.iter().any(|p| self.war_names.get(p).is_some_and(|(a, b)| a == nation || b == nation)) {
            t -= 8.0;
        }

        // Puppets chafe under a foreign overlord.
        if self.puppets.contains_key(nation) {
            t -= 6.0;
        }

        // Development: bigger, built-up cities are a little happier; occupied
        // settlements (owner != sovereign) are restless.
        t += (c.tier as f32) * 2.0;
        if c.buildings.iter().any(|b| b == "Church") {
            t += 4.0;
        }
        if c.owner != c.sovereign {
            t -= 15.0;
        }

        t.round().clamp(0.0, 100.0) as i32
    }

    fn monthly_manpower(&mut self) {
        let factions = self.active();
        for f in factions {
            let pop: i32 = self.cities.iter().filter(|c| c.owner == f).map(|c| c.population).sum();
            let add = (pop as f32 * 0.015) as i32;
            let cap = (pop as f32 * 0.10) as i32;
            let slot = self.manpower.entry(f).or_insert(0);
            *slot = (*slot + add).min(cap.max(500));
        }
    }

    fn american_revolution(&mut self) {
        if self.revolution_fired {
            return;
        }
        self.revolution_fired = true;
        self.add_faction("United States", 30);
        if let Some(s) = self.materials.get_mut("United States") {
            s.food = 30;
            s.hide = 15;
            s.lumber = 15;
        }
        let mut enemies = Vec::new();
        for c in &mut self.cities {
            if matches!(c.name.as_str(), "Boston" | "New York") && c.owner != "United States" {
                if c.owner != "France" {
                    enemies.push(c.owner.clone());
                }
                c.owner = "United States".into();
                c.sovereign = "United States".into();
            }
        }
        for enemy in enemies {
            let msg = format!("United States declares independence from {enemy}!");
            self.declare_war("United States", &enemy, &msg);
        }
        if !self.eliminated.contains("France") {
            let key = self.remember("United States", "France");
            self.alliances.insert(key, rand::thread_rng().gen_range(DAY_TICKS * 200..DAY_TICKS * 400));
            self.news("France allies with the United States!");
            self.cue(Cue::Alliance);
        }
        self.news("The American Revolution has begun!");
        self.paper("American Revolution!", "The thirteen colonies declare independence from Great Britain.");
        self.cue(Cue::Fanfare);
        if let Some(idx) = self.cities.iter().position(|c| c.name == "Boston" && c.owner == "United States") {
            self.spawn_unit(idx, self.cities[idx].id, "Line", true, false, "Patrol");
            self.spawn_unit(idx, self.cities[idx].id, "Square", true, false, "Patrol");
        }
    }

    fn haitian_independence(&mut self) {
        if self.haiti_fired {
            return;
        }
        self.haiti_fired = true;
        self.add_faction("Haiti", 30);
        let mut old_owner = None;
        for c in &mut self.cities {
            if c.name == "Port-au-Prince" {
                if c.owner != "Haiti" {
                    old_owner = Some(c.owner.clone());
                }
                c.owner = "Haiti".into();
                c.sovereign = "Haiti".into();
                c.is_capital = true;
            }
        }
        if let Some(old) = old_owner {
            let msg = format!("Haiti declares independence from {old}!");
            self.declare_war("Haiti", &old, &msg);
        }
        // The French Saint-Domingue puppet is replaced by independent Haiti.
        self.puppets.remove("Saint-Domingue");
        self.eliminated.insert("Saint-Domingue".into());
        self.news("Haitian Revolution! Haiti declares independence from France!");
        self.paper("Haitian Independence!", "Haiti becomes the first free Black republic.");
        self.cue(Cue::Fanfare);
    }

    fn mexican_independence(&mut self) {
        if self.mexico_fired {
            return;
        }
        self.mexico_fired = true;
        self.add_faction("Mexico", 40);
        let mut wars = Vec::new();
        for c in &mut self.cities {
            if matches!(c.name.as_str(), "Mexico City" | "Chihuahua" | "Oaxaca") && c.owner != "Mexico" {
                wars.push(c.owner.clone());
                c.owner = "Mexico".into();
                c.sovereign = "Mexico".into();
                c.is_capital = c.name == "Mexico City";
            }
        }
        if !self.cities.iter().any(|c| c.owner == "Spain" && c.is_capital) {
            if let Some(c) = self.cities.iter_mut().find(|c| c.name == "Havana" && c.owner == "Spain") {
                c.is_capital = true;
            }
        }
        for target in wars {
            let msg = format!("Mexico declares independence from {target}!");
            self.declare_war("Mexico", &target, &msg);
        }
        self.news("Mexican War of Independence!");
        self.paper("Mexican Independence!", "Mexico declares independence from Spain.");
        self.cue(Cue::Fanfare);
    }

    fn texas_independence(&mut self) {
        if self.texas_fired {
            return;
        }
        self.texas_fired = true;
        self.add_faction("Texas", 30);
        let mut old_owner = None;
        for c in &mut self.cities {
            if c.name == "San Antonio" || c.name == "Austin" {
                if c.name == "San Antonio" && c.owner != "Texas" {
                    old_owner = Some(c.owner.clone());
                }
                c.owner = "Texas".into();
                c.sovereign = "Texas".into();
                c.is_capital = c.name == "Austin";
            }
        }
        if let Some(old) = old_owner {
            let msg = format!("Texas declares independence from {old}!");
            self.declare_war("Texas", &old, &msg);
        }
        self.news("Texas Revolution! The Republic of Texas declares independence!");
        self.paper("Texas Revolution!", "Texas declares independence.");
        self.cue(Cue::Fanfare);
    }

    fn confederate_states(&mut self) {
        if self.confederate_fired {
            return;
        }
        self.confederate_fired = true;
        self.add_faction("Confederate States", 40);
        let mut wars = Vec::new();
        for c in &mut self.cities {
            if matches!(c.name.as_str(), "Richmond" | "Charleston" | "San Antonio" | "Austin" | "San Agustin" | "New Orleans" | "Williamsburg") {
                if c.owner != "Confederate States" {
                    wars.push(c.owner.clone());
                }
                c.owner = "Confederate States".into();
                c.sovereign = "Confederate States".into();
                c.is_capital = c.name == "Richmond";
            }
        }
        for target in wars {
            let msg = format!("Confederate States secedes from {target}!");
            self.declare_war("Confederate States", &target, &msg);
        }
        self.news("The Confederate States of America is formed!");
        self.paper("Confederate Secession!", "The American Civil War has begun.");
        self.cue(Cue::Fanfare);
    }

    fn canadian_confederation(&mut self) {
        if self.canada_fired {
            return;
        }
        self.canada_fired = true;
        // Canada forms as its OWN new nation out of British North America — it
        // does not replace Rupert's Land. Rupert's Land stays on the map as a
        // distinct nation (a British puppet, see `init_puppets`) and simply
        // transfers from Britain to the new Dominion at confederation, the way
        // the HBC lands passed to Canada historically.
        if !self.factions.iter().any(|f| f == "Canada") {
            self.factions.push("Canada".into());
        }
        self.touch("Canada");
        if let Some(s) = self.materials.get_mut("Canada") {
            s.gold += 60;
            s.lumber += 40;
        }
        for c in &mut self.cities {
            if matches!(c.name.as_str(), "Quebec" | "Montreal" | "Halifax" | "Ottawa" | "Tadoussac" | "Plaisance" | "Victoria" | "Toronto" | "York")
                && c.owner == "Great Britain"
            {
                c.owner = "Canada".into();
                c.sovereign = "Canada".into();
                c.builder = "Canada".into();
                c.is_capital = false;
            }
            if c.owner == "Canada" {
                c.is_capital = c.name == "Ottawa";
            }
        }
        self.puppets.insert("Canada".into(), "Great Britain".into());
        self.focus.insert("Canada".into(), "Economic".into());
        // Rupert's Land passes from the British Crown to the Dominion of Canada,
        // but only if it is still around and still a British puppet.
        if !self.eliminated.contains("Rupert's Land")
            && self.puppets.get("Rupert's Land").map(|o| o.as_str()) == Some("Great Britain")
        {
            self.puppets.insert("Rupert's Land".into(), "Canada".into());
            self.claims_dirty = true;
        }
        self.news("Canadian Confederation! The Dominion of Canada is formed.");
        self.paper("Canadian Confederation!", "British North America unites as the Dominion of Canada. Ottawa is the capital, and Rupert's Land passes to the new dominion.");
        self.cue(Cue::Fanfare);
    }

    /// Reserved helper: wholesale rename/merge one faction into another. No
    /// event currently uses it (Canadian Confederation now spawns Canada as a
    /// separate nation rather than renaming Rupert's Land), but it is kept for
    /// any future faction-transformation event.
    #[allow(dead_code)]
    fn rename_faction(&mut self, old: &str, new: &str) {
        if let Some(slot) = self.factions.iter_mut().find(|f| f.as_str() == old) {
            *slot = new.into();
        } else if !self.factions.iter().any(|f| f == new) {
            self.factions.push(new.into());
        }
        if let Some(stock) = self.materials.remove(old) {
            self.materials.insert(new.into(), stock);
        }
        if let Some(v) = self.manpower.remove(old) {
            self.manpower.insert(new.into(), v);
        }
        if let Some(v) = self.focus.remove(old) {
            self.focus.insert(new.into(), v);
        }
        if self.eliminated.remove(old) {
            self.eliminated.insert(new.into());
        }
        let puppets = std::mem::take(&mut self.puppets);
        self.puppets = puppets
            .into_iter()
            .map(|(p, o)| {
                let p = if p == old { new.into() } else { p };
                let o = if o == old { new.into() } else { o };
                (p, o)
            })
            .collect();
        for c in &mut self.cities {
            for field in [&mut c.owner, &mut c.sovereign] {
                if field == old {
                    *field = new.into();
                }
            }
        }
        for u in &mut self.units {
            if u.faction == old {
                u.faction = new.into();
            }
        }
        for m in &mut self.merchants {
            if m.faction == old {
                m.faction = new.into();
            }
        }
        for s in &mut self.settlers {
            if s.faction == old {
                s.faction = new.into();
            }
        }
        self.rekey_pairs(old, new);
    }

    #[allow(dead_code)]
    fn rekey_pairs(&mut self, old: &str, new: &str) {
        let rename = |name: String| if name == old { new.to_string() } else { name };
        let named = std::mem::take(&mut self.war_names);
        let mut remap = HashMap::new();
        let mut names = HashMap::new();
        for (key, (a, b)) in named {
            let a = rename(a);
            let b = rename(b);
            let next = Pair::new(&a, &b);
            remap.insert(key, next);
            names.insert(next, (a, b));
        }
        self.war_names = names;
        let map_key = |key: Pair| remap.get(&key).copied().unwrap_or(key);
        let wars = std::mem::take(&mut self.wars);
        self.wars = wars.into_iter().map(map_key).collect();
        self.treaties = rekey_map(std::mem::take(&mut self.treaties), &remap);
        self.alliances = rekey_map(std::mem::take(&mut self.alliances), &remap);
        self.pressure = rekey_map(std::mem::take(&mut self.pressure), &remap);
        self.treaty_cd = rekey_map(std::mem::take(&mut self.treaty_cd), &remap);
        self.refused = rekey_map(std::mem::take(&mut self.refused), &remap);
        self.fatigue = rekey_map(std::mem::take(&mut self.fatigue), &remap);
    }

    fn spawn_armies(&mut self, maps: &Maps) {
        self.spawn_timer += self.speed;
        if self.spawn_timer <= DAY_TICKS * 5 {
            return;
        }
        self.spawn_timer = 0;
        let plans: Vec<(usize, String, bool)> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(_, c)| !self.eliminated.contains(&c.owner) && c.spawn_cd <= 0)
            .map(|(i, c)| (i, c.owner.clone(), c.is_fort))
            .collect();
        for (idx, faction, is_fort) in plans {
            if self.cities[idx].spawn_cd > 0 {
                continue;
            }
            let ucount = self.units.iter().filter(|u| u.alive && u.faction == faction).count() as i32;
            let ucap = self.cities.iter().filter(|c| c.owner == faction && !c.is_fort && !c.is_camp).count() as i32;
            let at_war_now = self.active().iter().any(|f| f != &faction && self.at_war(&faction, f));
            let focus = self.focus.get(&faction).cloned().unwrap_or_else(|| "Neutral".into());
            let fill = ucount as f32 / ucap.max(1) as f32;
            let (w_mil, w_mer) = match focus.as_str() {
                "Aggressive" if at_war_now => (if fill < 0.8 { 0.75 } else { 0.4 }, 0.1),
                "Aggressive" => (if fill < 0.7 { 0.5 } else { 0.2 }, 0.2),
                "Economic" if at_war_now => (if fill < 0.5 { 0.3 } else { 0.1 }, 0.5),
                "Economic" => (if fill < 0.3 { 0.1 } else { 0.02 }, 0.6),
                _ if at_war_now => (if fill < 0.7 { 0.6 } else { 0.2 }, 0.25),
                _ => (if fill < 0.5 { 0.2 } else { 0.05 }, 0.4),
            };
            let roll: f32 = rand::random();
            if roll < w_mil {
                self.try_spawn_unit(idx, is_fort, ucount, ucap, maps);
            } else if roll < w_mil + w_mer {
                self.try_spawn_merchant(idx);
            }
        }
        for c in &mut self.cities {
            if c.spawn_cd > 0 {
                c.spawn_cd -= self.speed;
            }
        }
    }

    fn try_spawn_unit(&mut self, idx: usize, is_fort: bool, ucount: i32, ucap: i32, maps: &Maps) {
        let faction = self.cities[idx].owner.clone();
        let (x, y) = (self.cities[idx].x, self.cities[idx].y);
        let id = self.cities[idx].id;
        let range = if is_native(&faction) { NATIVE_MAX_RANGE } else { UNIT_MAX_RANGE };
        if is_fort {
            let near = self.cities.iter().any(|c| c.owner != faction && !self.at_peace(&faction, &c.owner) && dist(x, y, c.x, c.y) <= 150.0)
                || self.units.iter().any(|u| u.alive && u.faction != faction && !self.at_peace(&faction, &u.faction) && dist(x, y, u.x, u.y) <= 150.0);
            if !near {
                return;
            }
        } else if ucount >= ucap {
            return;
        }
        let enemies: Vec<u32> = self
            .cities
            .iter()
            .filter(|c| c.owner != faction && !self.at_peace(&faction, &c.owner) && dist(x, y, c.x, c.y) <= range)
            .map(|c| c.id)
            .collect();
        let formation = self.pick_formation(idx);
        let cost = formation_cost(&faction, &formation);
        let stock = self.materials.get(&faction).cloned().unwrap_or_else(Stock::zero);
        let men = *self.manpower.get(&faction).unwrap_or(&0);
        if !stock.afford(&cost) || stock.food < 20 || men < 150 {
            return;
        }
        if !is_native(&faction) && stock.gold < 30 {
            return;
        }
        if let Some(s) = self.materials.get_mut(&faction) {
            s.pay(&cost);
        }
        let (target, idle, patrol, behavior) = if let Some(t) = enemies.choose_rand() {
            // An enemy is in reach: march on it.
            (*t, false, false, "Attack")
        } else if ucount < ucap {
            // No enemy nearby: give the garrison a peacetime standing order.
            // Border cities tend to raise guards that hold the frontier; the
            // rest patrol between friendly cities. Mobile tribes don't guard
            // territory, so they always patrol.
            let mut rng = rand::thread_rng();
            let on_border = !is_native(&faction)
                && self.city_on_border(idx, maps)
                && rng.gen_bool(0.5);
            if on_border {
                // Guard: anchor on this city's region; destination set per tick.
                (id, false, false, "Guard")
            } else {
                let friend = self
                    .cities
                    .iter()
                    .find(|c| c.owner == faction && c.id != id && dist(x, y, c.x, c.y) < 160.0)
                    .map(|c| c.id);
                match friend {
                    Some(fid) => (fid, false, true, "Patrol"),
                    None => (id, true, false, "Patrol"),
                }
            }
        } else {
            return;
        };
        self.spawn_unit(idx, target, &formation, idle, patrol, behavior);
        let cd = if self.cities[idx].is_fort {
            DAY_TICKS * 90
        } else if self.cities[idx].is_village {
            DAY_TICKS * 60
        } else if self.cities[idx].is_capital {
            DAY_TICKS * 30
        } else {
            DAY_TICKS * 45
        };
        let cd = if self.cities[idx].buildings.iter().any(|b| b == "Barracks") { cd * 7 / 10 } else { cd };
        self.cities[idx].spawn_cd = cd;
    }

    fn pick_formation(&self, idx: usize) -> String {
        let faction = &self.cities[idx].owner;
        let mut rng = rand::thread_rng();
        if faction == "Comanche" {
            return "Square".into();
        }
        if faction == "Dakota" {
            return if rng.gen_bool(0.75) { "Line" } else { "Square" }.into();
        }
        if faction == "Cree" || is_native(faction) {
            return "Line".into();
        }
        if self.cities[idx].buildings.iter().any(|b| b == "Stables") && rng.gen_bool(0.33) {
            "Square".into()
        } else if rng.gen_bool(0.5) {
            "Line".into()
        } else {
            "Column".into()
        }
    }

    fn spawn_unit(&mut self, city_idx: usize, target: u32, formation: &str, idle: bool, patrol: bool, behavior: &str) {
        let faction = self.cities[city_idx].owner.clone();
        let (x, y) = (self.cities[city_idx].x, self.cities[city_idx].y);
        let (speed, mut power) = match formation {
            "Column" => (0.54, 3),
            "Square" => (0.75, 8),
            _ => (0.36, 5),
        };
        if is_native(&faction) {
            power = (power / 2).max(1);
            if formation == "Square" {
                power = (power / 2).max(1);
            }
        } else {
            power *= 2;
        }
        let mut rng = rand::thread_rng();
        let available = *self.manpower.get(&faction).unwrap_or(&0);
        let ideal = if is_native(&faction) { rng.gen_range(100..=300) } else { rng.gen_range(400..=1000) };
        let men = if is_native(&faction) { available.min(ideal) } else { available.min(ideal.max(200)) };
        if men <= 0 {
            return;
        }
        *self.manpower.entry(faction.clone()).or_insert(0) -= men;
        let uid = self.next_uid;
        self.next_uid += 1;
        self.units.push(Unit {
            uid,
            faction: faction.clone(),
            target,
            x,
            y,
            formation: formation.into(),
            speed,
            power,
            men,
            max_men: men,
            tier: 0,
            retreating: false,
            retreat: None,
            idle,
            patrolling: patrol,
            opponent: None,
            name: regiment_name(&faction),
            behavior: behavior.into(),
            path: Vec::new(),
            path_goal: -1,
            path_from: -1,
            guard_region: -1,
            guard_anchor: None,
            siege_timer: 0,
            siege_phase: 1,
            phase_timer: 0,
            battle_phase: 0,
            skirmish_ticks: 0,
            alive: true,
            ship: false,
        });
    }

    /// Picks a trade destination for a merchant leaving `from_id` at (`x`, `y`).
    /// Instead of always taking the first in-range city (which made every
    /// merchant converge on the same nearby town), this gathers every valid,
    /// at-peace city within range and chooses one at random, weighted to favor
    /// farther and less-crowded cities so trade spreads across the whole map.
    fn pick_trade_dest(&self, faction: &str, from_id: u32, x: f32, y: f32) -> Option<u32> {
        let mut candidates: Vec<(u32, f32)> = Vec::new();
        for c in &self.cities {
            if c.id == from_id || self.at_war(faction, &c.owner) {
                continue;
            }
            let d = dist(x, y, c.x, c.y);
            if d > MERCHANT_MAX_TRAVEL || d < 1.0 {
                continue;
            }
            // How many merchants are already heading to this city.
            let crowd = self
                .merchants
                .iter()
                .filter(|m| m.alive && m.dest == c.id)
                .count() as f32;
            // Favor distance (reach far-off markets) and penalize crowding so
            // merchants fan out rather than stacking on one route.
            let weight = (d / MERCHANT_MAX_TRAVEL) * (1.0 / (1.0 + crowd));
            candidates.push((c.id, weight.max(0.01)));
        }
        if candidates.is_empty() {
            return None;
        }
        let total: f32 = candidates.iter().map(|(_, w)| w).sum();
        let mut roll = rand::random::<f32>() * total;
        for (id, w) in &candidates {
            roll -= w;
            if roll <= 0.0 {
                return Some(*id);
            }
        }
        Some(candidates[candidates.len() - 1].0)
    }

    fn try_spawn_merchant(&mut self, idx: usize) {
        let faction = self.cities[idx].owner.clone();
        let cap = if is_native(&faction) { NATIVE_MERCHANT_CAP } else { MERCHANT_CAP };
        let have = self.merchants.iter().filter(|m| m.alive && m.faction == faction).count() as i32;
        if have >= cap {
            return;
        }
        let stock = self.materials.get(&faction).cloned().unwrap_or_else(Stock::zero);
        let native = is_native(&faction);
        if native {
            if stock.food < 10 || stock.hide < 5 {
                return;
            }
        } else if stock.gold < 40 || stock.food < 20 {
            return;
        }
        let (x, y, id, material) = {
            let c = &self.cities[idx];
            (c.x, c.y, c.id, c.material.clone())
        };
        let dest = self.pick_trade_dest(&faction, id, x, y);
        let Some(dest) = dest else { return };
        if let Some(s) = self.materials.get_mut(&faction) {
            if native {
                s.food -= 10;
                s.hide -= 5;
            } else {
                s.gold -= 40;
                s.food -= 20;
            }
        }
        self.merchants.push(Merchant {
            faction,
            x,
            y,
            source: id,
            dest,
            material,
            status: 0,
            timer: 0,
            ship: false,
            hp: 30,
            alive: true,
        });
    }

    fn produce(&mut self, maps: &Maps) {
        self.food_timer += 1;
        self.lumber_timer += 1;
        self.hide_timer += 1;
        self.iron_timer += 1;
        if self.food_timer >= 24 {
            self.food_timer = 0;
            let gains: Vec<(String, i32)> = self
                .cities
                .iter()
                .filter(|c| !self.flooded.contains_key(&c.id))
                .map(|c| {
                    let mut base = if is_native(&c.owner) { 8 } else { 10 };
                    let mult = biome_mods(maps.biome_at(c.x, c.y)).1;
                    if c.buildings.iter().any(|b| b == "Farm") {
                        base *= 2;
                    }
                    (c.owner.clone(), (base as f32 * mult) as i32)
                })
                .collect();
            for (owner, n) in gains {
                self.touch(&owner);
                if let Some(s) = self.materials.get_mut(&owner) {
                    s.food += n;
                }
            }
        }
        if self.lumber_timer >= 180 {
            self.lumber_timer = 0;
            self.produce_material(maps, "Lumber", 3, 2);
        }
        if self.hide_timer >= 240 {
            self.hide_timer = 0;
            self.produce_material(maps, "Hide", 1, 3);
        }
        if self.iron_timer >= 360 {
            self.iron_timer = 0;
            let gains: Vec<(String, i32, i32)> = self
                .cities
                .iter()
                .filter(|c| c.material == "Iron" && !self.flooded.contains_key(&c.id))
                .map(|c| {
                    let foundry = c.buildings.iter().any(|b| b == "Foundry");
                    (c.owner.clone(), if foundry { 2 } else { 1 }, if foundry { 2 } else { 1 })
                })
                .collect();
            for (owner, iron, coal) in gains {
                self.touch(&owner);
                if let Some(s) = self.materials.get_mut(&owner) {
                    s.iron += iron;
                    s.coal += coal;
                }
            }
        }
    }

    fn produce_material(&mut self, maps: &Maps, material: &str, base: i32, mod_idx: usize) {
        let gains: Vec<(String, i32)> = self
            .cities
            .iter()
            .filter(|c| c.material == material && !self.flooded.contains_key(&c.id))
            .map(|c| {
                let mods = biome_mods(maps.biome_at(c.x, c.y));
                let mult = match mod_idx {
                    2 => mods.2,
                    3 => mods.3,
                    _ => 1.0,
                };
                let extra = if material == "Lumber" && c.buildings.iter().any(|b| b == "Lumber Mill") { 2 } else { 1 };
                (c.owner.clone(), (base * extra) as f32 * mult)
            })
            .filter(|(_, n)| *n > 0.0)
            .map(|(o, n)| (o, n as i32))
            .collect();
        for (owner, n) in gains {
            self.touch(&owner);
            if let Some(s) = self.materials.get_mut(&owner) {
                s.add(material, n);
            }
        }
    }

    fn gold_tax(&mut self, maps: &Maps) {
        let gains: Vec<(String, i32)> = self
            .cities
            .iter()
            .filter(|c| !self.flooded.contains_key(&c.id))
            .map(|c| {
                let mut mult = biome_mods(maps.biome_at(c.x, c.y)).4 * 0.75;
                if c.buildings.iter().any(|b| b == "Town Hall" || b == "Council Lodge") {
                    mult *= 1.25;
                }
                // Taxation law scales income: None 0x, Low 0.5x, Standard 1x, Heavy 1.5x.
                let nation = if c.sovereign.is_empty() { &c.owner } else { &c.sovereign };
                mult *= self.tax_multiplier(nation);
                let base = if c.is_village || c.is_fort || c.is_camp { 2 } else { 5 };
                (c.owner.clone(), (base as f32 * mult) as i32)
            })
            .collect();
        for (owner, n) in gains {
            self.touch(&owner);
            if let Some(s) = self.materials.get_mut(&owner) {
                s.gold += n;
            }
        }
    }

    fn heal_garrisons(&mut self) {
        let plans: Vec<(usize, String, i32)> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(_, c)| c.troops < max_troops(c))
            .map(|(i, c)| (i, c.owner.clone(), max_troops(c)))
            .collect();
        for (i, owner, max) in plans {
            let native = is_native(&owner);
            let ok = self.materials.get(&owner).is_some_and(|s| {
                if native { s.food >= 10 && s.hide >= 5 } else { s.gold >= 50 && s.lumber >= 10 }
            });
            if !ok {
                continue;
            }
            if let Some(s) = self.materials.get_mut(&owner) {
                if native {
                    s.food -= 10;
                    s.hide -= 5;
                } else {
                    s.gold -= 50;
                    s.lumber -= 10;
                }
            }
            self.cities[i].troops = max;
        }
    }

    fn passive_regen(&mut self) {
        self.regen_timer += 1;
        if self.regen_timer < 60 {
            return;
        }
        self.regen_timer = 0;
        for c in &mut self.cities {
            if c.regen_cooldown > 0 {
                c.regen_cooldown -= 60;
                continue;
            }
            let max = max_troops(c);
            if c.troops < max {
                c.troops = (c.troops + 2).min(max);
            }
        }
    }

    fn upkeep(&mut self) {
        let factions = self.active();
        for f in factions {
            let native = is_native(&f);
            let costs: Vec<(u32, i32, i32)> = self
                .units
                .iter()
                .filter(|u| u.alive && u.faction == f)
                .map(|u| {
                    if native {
                        (u.uid, 0, 1)
                    } else if u.formation == "Square" {
                        (u.uid, 4, 5)
                    } else {
                        (u.uid, 2, 3)
                    }
                })
                .collect();
            for (uid, g, food) in costs {
                let ok = self.materials.get(&f).is_some_and(|s| s.gold >= g && s.food >= food);
                if ok {
                    if let Some(s) = self.materials.get_mut(&f) {
                        s.gold -= g;
                        s.food -= food;
                    }
                } else if let Some(u) = self.units.iter_mut().find(|u| u.uid == uid) {
                    u.men = (u.men - 2).max(1);
                    u.retreating = true;
                }
            }
            let merchants: Vec<usize> = self.merchants.iter().enumerate().filter(|(_, m)| m.alive && m.faction == f).map(|(i, _)| i).collect();
            for i in merchants {
                let (g, food) = if native { (0, 1) } else { (1, 1) };
                let ok = self.materials.get(&f).is_some_and(|s| s.gold >= g && s.food >= food);
                if ok {
                    if let Some(s) = self.materials.get_mut(&f) {
                        s.gold -= g;
                        s.food -= food;
                    }
                } else {
                    self.merchants[i].alive = false;
                }
            }
        }
    }

    fn upgrade_units(&mut self) {
        let factions = self.active();
        for f in factions {
            let candidate = self.units.iter().find(|u| u.alive && u.faction == f && u.tier < 3 && !u.retreating && u.opponent.is_none()).map(|u| u.uid);
            let Some(uid) = candidate else { continue };
            let tier = self.units.iter().find(|u| u.uid == uid).map(|u| u.tier).unwrap_or(0) + 1;
            let iron = tier;
            let coal = (tier - 1).max(0);
            let ok = self.materials.get(&f).is_some_and(|s| s.iron >= iron && s.coal >= coal);
            if !ok {
                continue;
            }
            if let Some(s) = self.materials.get_mut(&f) {
                s.iron -= iron;
                s.coal -= coal;
            }
            if let Some(u) = self.units.iter_mut().find(|u| u.uid == uid) {
                u.tier = tier;
                u.power += 2;
                u.max_men += 10;
                u.men = (u.men + 10).min(u.max_men);
            }
        }
    }

    fn upgrade_cities(&mut self) {
        let costs = [(1, 2, 10, 50), (2, 5, 25, 120), (3, 8, 40, 200)];
        let plans: Vec<(usize, String)> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(_, c)| c.tier < 3 && c.buildings.iter().any(|b| b == "Town Hall" || b == "Council Lodge"))
            .map(|(i, c)| (i, c.owner.clone()))
            .collect();
        for (i, owner) in plans {
            let nt = self.cities[i].tier + 1;
            let Some((_, iron, lumber, gold)) = costs.iter().find(|(t, ..)| *t == nt) else { continue };
            let ok = self.materials.get(&owner).is_some_and(|s| s.iron >= *iron && s.lumber >= *lumber && s.gold >= *gold);
            if !ok {
                continue;
            }
            if let Some(s) = self.materials.get_mut(&owner) {
                s.iron -= iron;
                s.lumber -= lumber;
                s.gold -= gold;
            }
            self.cities[i].tier = nt;
            let max = max_troops(&self.cities[i]);
            self.cities[i].troops = (self.cities[i].troops + 25).min(max);
            self.cue(Cue::Built);
        }
    }

    fn finish_construction(&mut self) {
        let mut finished = false;
        for c in &mut self.cities {
            if let Some((name, ticks)) = c.construction.clone() {
                let left = ticks - self.speed;
                if left <= 0 {
                    push_b(&mut c.buildings, &name);
                    c.construction = None;
                    finished = true;
                } else {
                    c.construction = Some((name, left));
                }
            }
        }
        if finished {
            self.cue(Cue::Built);
        }
    }

    fn ai_build(&mut self, maps: &Maps) {
        let idxs: Vec<usize> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(_, c)| c.construction.is_none() && !c.is_fort && !c.is_camp && !self.eliminated.contains(&c.owner))
            .map(|(i, _)| i)
            .collect();
        for i in idxs {
            let want = {
                let c = &self.cities[i];
                let native = is_native(&c.owner);
                if native {
                    if !has(&c.buildings, "Council Lodge") {
                        Some("Council Lodge")
                    } else if c.tier >= 1 && c.material == "Lumber" && !has(&c.buildings, "Lumber Mill") {
                        Some("Lumber Mill")
                    } else if c.tier >= 1 && !has(&c.buildings, "Market") {
                        Some("Market")
                    } else {
                        None
                    }
                } else if !has(&c.buildings, "Town Hall") {
                    Some("Town Hall")
                } else if !has(&c.buildings, "Farm") {
                    Some("Farm")
                } else if !has(&c.buildings, "Road") && !ISLANDS.contains(&c.name.as_str()) {
                    Some("Road")
                } else if c.tier >= 1 && !has(&c.buildings, "Market") {
                    Some("Market")
                } else if c.tier >= 1 && c.material == "Lumber" && !has(&c.buildings, "Lumber Mill") {
                    Some("Lumber Mill")
                } else if c.tier >= 1 && !has(&c.buildings, "Barracks") {
                    Some("Barracks")
                } else {
                    None
                }
            };
            if let Some(name) = want {
                let biome = maps.biome_at(self.cities[i].x, self.cities[i].y);
                if name == "Plantation" && !matches!(biome, 4 | 6 | 8) {
                    continue;
                }
                self.start_build(i, name);
            }
        }
    }

    fn start_build(&mut self, idx: usize, name: &str) {
        let Some(spec) = building(name) else { return };
        let owner = self.cities[idx].owner.clone();
        let native = is_native(&owner);
        if spec.colonial_only && native || spec.native_only && !native {
            return;
        }
        if self.cities[idx].tier < spec.tier_req || has(&self.cities[idx].buildings, name) {
            return;
        }
        let cost = [
            ("Gold", spec.gold),
            ("Lumber", spec.lumber),
            ("Hide", spec.hide),
            ("Iron", spec.iron),
            ("Coal", spec.coal),
            ("Food", spec.food),
        ];
        let cost: Vec<(&str, i32)> = cost.into_iter().filter(|(_, n)| *n > 0).collect();
        let ok = self.materials.get(&owner).is_some_and(|s| s.afford(&cost));
        if !ok {
            return;
        }
        if let Some(s) = self.materials.get_mut(&owner) {
            s.pay(&cost);
        }
        self.cities[idx].construction = Some((name.into(), spec.days * DAY_TICKS));
    }

    fn maybe_extra_merchants(&mut self) {
        self.merchant_timer += self.speed;
        if self.merchant_timer < DAY_TICKS * 10 {
            return;
        }
        self.merchant_timer = 0;
        let idxs: Vec<usize> = (0..self.cities.len()).filter(|_| rand::random::<f32>() < 0.05).collect();
        for i in idxs {
            self.try_spawn_merchant(i);
        }
    }

    /// Settlers are the expansion mechanic: a nation that sends a settler to a
    /// region comes to own that region (see `region_owners`). The old caps on
    /// forts/camps are gone — the only limits are a cooldown, a modest resource
    /// cost so expansion is paced, and the requirement that the target be a
    /// region the faction does not already own.
    fn maybe_settlers(&mut self, maps: &Maps) {
        self.settler_timer += self.speed;
        if self.settler_timer < DAY_TICKS * 60 {
            return;
        }
        self.settler_timer = 0;
        let idxs: Vec<usize> = (0..self.cities.len()).filter(|_| rand::random::<f32>() < 0.05).collect();
        for i in idxs {
            let owner = self.cities[i].owner.clone();
            let native = is_native(&owner);
            let (x, y) = (self.cities[i].x, self.cities[i].y);
            // Modest cost so expansion is paced but never hard-capped.
            if native {
                let cost = [("Food", 30), ("Hide", 15), ("Lumber", 10)];
                if !self.materials.get(&owner).is_some_and(|s| s.afford(&cost)) {
                    continue;
                }
                if let Some(s) = self.materials.get_mut(&owner) {
                    s.pay(&cost);
                }
            } else {
                let cost = [("Gold", 25), ("Food", 40), ("Lumber", 30)];
                if !self.materials.get(&owner).is_some_and(|s| s.afford(&cost)) {
                    continue;
                }
                if let Some(s) = self.materials.get_mut(&owner) {
                    s.pay(&cost);
                }
            }
            // Regions the settler may NOT target. This is the union of:
            //  - regions this faction already owns (its cities' regions plus any
            //    authored starting claims), so a settler expands the nation
            //    rather than crowding home turf or overwriting its own borders;
            //  - regions already claimed by ANY OTHER nation, so a nation can't
            //    plant a fort/camp inside territory a rival already holds.
            // The settler aims for a nearby region in neither bucket, i.e.
            // genuinely unclaimed land.
            let mut owned: std::collections::HashSet<i32> = self
                .cities
                .iter()
                .filter(|c| c.owner == owner && c.region >= 0)
                .map(|c| c.region)
                .collect();
            for (region, nation) in &self.region_claims {
                if *nation == owner {
                    owned.insert(*region);
                }
            }
            // Block every region currently owned by a different nation.
            for region in 0..maps.region_count {
                if owned.contains(&region) {
                    continue;
                }
                if let Some(holder) = self.region_owner_name(region) {
                    if holder != owner {
                        owned.insert(region);
                    }
                }
            }
            let Some((tx, ty)) = settle_spot(maps, x, y, &self.cities, &owned) else {
                continue;
            };
            self.settlers.push(Settler { faction: owner, x, y, tx, ty, native, ship: false, alive: true });
        }
    }

    fn stalemates(&mut self) {
        for key in self.wars.clone() {
            *self.pressure.entry(key).or_insert(0) += self.speed.max(1) * 5;
        }
        let due: Vec<(String, String)> = self
            .pressure
            .iter()
            .filter(|(k, v)| self.wars.contains(k) && **v >= STALEMATE_THRESHOLD)
            .filter_map(|(k, _)| self.war_names.get(k).cloned())
            .collect();
        for (a, b) in due {
            let msg = format!("Stalemate! {a} & {b} sign a peace treaty.");
            self.make_treaty(&a, &b, &msg);
        }
    }

    fn forced_peace(&mut self) {
        let total = self.cities.len().max(1) as f32;
        let af = self.active();
        let mut orders = Vec::new();
        for i in 0..af.len() {
            for b in af.iter().skip(i + 1) {
                let a = &af[i];
                if self.treaties.contains_key(&Pair::new(a, b)) {
                    continue;
                }
                let ca = self.cities.iter().filter(|c| c.owner == *a).count() as f32 / total;
                let cb = self.cities.iter().filter(|c| c.owner == *b).count() as f32 / total;
                if ca >= 0.70 && cb <= 0.15 {
                    orders.push((a.clone(), b.clone(), format!("{a} imposes peace on {b}.")));
                } else if cb >= 0.70 && ca <= 0.15 {
                    orders.push((a.clone(), b.clone(), format!("{b} imposes peace on {a}.")));
                }
            }
        }
        for (a, b, msg) in orders {
            self.make_treaty(&a, &b, &msg);
        }
    }

    fn alliance_chances(&mut self) {
        let af = self.active();
        let mut formed = Vec::new();
        for i in 0..af.len() {
            for b in af.iter().skip(i + 1) {
                let a = &af[i];
                let key = Pair::new(a, b);
                if self.alliances.contains_key(&key) || self.treaties.contains_key(&key) || self.wars.contains(&key) {
                    continue;
                }
                let share = self.active().iter().any(|e| e != a && e != b && self.at_war(a, e) && self.at_war(b, e));
                if share && rand::random::<f32>() < 0.02 {
                    formed.push((a.clone(), b.clone()));
                }
            }
        }
        for (a, b) in formed {
            let key = self.remember(&a, &b);
            self.alliances.insert(key, rand::thread_rng().gen_range(DAY_TICKS * 50..DAY_TICKS * 1825));
            self.news(format!("{a} & {b} form an alliance!"));
            self.cue(Cue::Alliance);
        }
    }

    fn war_fatigue(&mut self) {
        let wars: Vec<(String, String)> = self.wars.iter().filter_map(|k| self.war_names.get(k).cloned()).collect();
        for (a, b) in wars {
            let key = Pair::new(&a, &b);
            let fa = self.fatigue.get(&key).and_then(|m| m.get(&a)).copied().unwrap_or(0);
            let fb = self.fatigue.get(&key).and_then(|m| m.get(&b)).copied().unwrap_or(0);
            let ex = |f: &str, losses: i32| {
                let stock = self.materials.get(f);
                let mut score = losses * 5;
                if stock.map(|s| s.gold < 5).unwrap_or(true) {
                    score += 20;
                }
                if stock.map(|s| s.food < 5).unwrap_or(true) {
                    score += 20;
                }
                if self.cities.iter().filter(|c| c.owner == f).count() <= 1 {
                    score += 30;
                }
                if !self.cities.iter().any(|c| c.owner == f && c.is_capital) {
                    score += 40;
                }
                score
            };
            let ea = ex(&a, fa);
            let eb = ex(&b, fb);
            if ea > 80 && ea > eb * 3 / 2 {
                let msg = format!("{b} dictates terms to exhausted {a}.");
                self.make_treaty(&b, &a, &msg);
            } else if eb > 80 && eb > ea * 3 / 2 {
                let msg = format!("{a} dictates terms to exhausted {b}.");
                self.make_treaty(&a, &b, &msg);
            } else if ea > 40 && eb > 40 && rand::random::<f32>() < 0.03 {
                let msg = format!("White peace between {a} & {b}.");
                self.make_treaty(&a, &b, &msg);
            }
        }
    }

    fn territorial_demands(&mut self) {
        let demanders = self.active();
        for demander in demanders {
            if demander == "Pirates" {
                continue;
            }
            let claim = self.cities.iter().find(|c| c.sovereign == demander && c.owner != demander && c.owner != "Pirates" && !self.at_war(&demander, &c.owner)).map(|c| (c.id, c.owner.clone(), c.name.clone()));
            let Some((id, holder, name)) = claim else { continue };
            if rand::random::<f32>() > 0.005 || self.eliminated.contains(&holder) {
                continue;
            }
            let key = self.remember(&demander, &holder);
            let hu = self.units.iter().filter(|u| u.faction == holder).count();
            let du = self.units.iter().filter(|u| u.faction == demander).count();
            let mut chance = 0.3;
            if du > hu * 3 / 2 {
                chance += 0.3;
            }
            if rand::random::<f32>() < chance {
                if let Some(c) = self.cities.iter_mut().find(|c| c.id == id) {
                    c.owner = demander.clone();
                    c.sovereign = demander.clone();
                }
                self.news(format!("{holder} returns {name} to {demander}."));
                self.cue(Cue::Occupy);
            } else {
                *self.refused.entry(key).or_insert(0) += 1;
                self.news(format!("{holder} refuses to return {name} to {demander}!"));
                self.cue(Cue::Break);
            }
        }
    }

    fn war_declarations(&mut self) {
        let af = self.active();
        let mut orders = Vec::new();
        for i in 0..af.len() {
            for b in af.iter().skip(i + 1) {
                let a = &af[i];
                let key = Pair::new(a, b);
                if self.wars.contains(&key) || self.treaties.contains_key(&key) || self.alliances.contains_key(&key) {
                    continue;
                }
                let ca: Vec<(f32, f32)> = self.cities.iter().filter(|c| c.owner == *a).map(|c| (c.x, c.y)).collect();
                let cb: Vec<(f32, f32)> = self.cities.iter().filter(|c| c.owner == *b).map(|c| (c.x, c.y)).collect();
                if ca.is_empty() || cb.is_empty() {
                    continue;
                }
                let min_d = ca.iter().flat_map(|p| cb.iter().map(move |q| dist(p.0, p.1, q.0, q.1))).fold(f32::MAX, f32::min);
                let war_range = if is_native(a) || is_native(b) { 100.0 } else { 600.0 };
                if min_d > war_range {
                    continue;
                }
                for (atk, def) in [(a.clone(), b.clone()), (b.clone(), a.clone())] {
                    let u_atk = self.units.iter().filter(|u| u.faction == atk).count();
                    let u_def = self.units.iter().filter(|u| u.faction == def).count();
                    if u_atk == 0 {
                        continue;
                    }
                    let mut score = 0;
                    if self.cities.iter().any(|c| c.sovereign == atk && c.owner == def) {
                        score += 35;
                    }
                    score += self.refused.get(&Pair::new(&atk, &def)).copied().unwrap_or(0) * 25;
                    if self.active().iter().any(|x| x != &atk && x != &def && self.at_war(&def, x)) {
                        score += 30;
                    }
                    if u_atk > u_def {
                        score += 20;
                    } else if u_def >= u_atk * 2 {
                        score -= 40;
                    } else if u_def > u_atk {
                        score -= 20;
                    }
                    let poor = self.materials.get(&atk).is_some_and(|s| s.gold < 5 || s.food < 5);
                    if poor {
                        score -= 15;
                    }
                    let near = self.cities.iter().any(|c| c.owner == atk)
                        && self.cities.iter().any(|e| e.owner == def && self.cities.iter().filter(|c| c.owner == atk).any(|c| dist(c.x, c.y, e.x, e.y) < 150.0));
                    if near {
                        score += 30;
                    }
                    if score > WAR_THRESHOLD && rand::random::<f32>() < WAR_DECLARE_CHANCE {
                        orders.push((atk, def, score));
                        break;
                    }
                }
            }
        }
        for (atk, def, score) in orders {
            let msg = format!("{atk} declares war on {def} (score:{score})");
            self.declare_war(&atk, &def, &msg);
        }
    }

    fn disasters(&mut self) {
        let ids: Vec<u32> = self.cities.iter().map(|c| c.id).collect();
        for id in ids {
            if rand::random::<f32>() < DISASTER_CHANCE {
                self.strike(id);
            }
            let month = self.month;
            if matches!(month, 3 | 4 | 5) && rand::random::<f32>() < 0.003 {
                if let Some(c) = self.cities.iter_mut().find(|c| c.id == id) {
                    c.troops = (c.troops - rand::thread_rng().gen_range(10..30)).max(1);
                    c.happiness = (c.happiness - 8).max(0);
                    let name = c.name.clone();
                    self.flooded.insert(id, DAY_TICKS * 30);
                    self.news(format!("Flood hit {name}! Production halted for a month."));
                    self.cue(Cue::Flood);
                }
            }
        }
    }

    fn strike(&mut self, id: u32) {
        const KINDS: &[(&str, i32, f32)] = &[
            ("Hurricane", 60, 40.0),
            ("Tornado", 40, 25.0),
            ("Storm", 25, 35.0),
            ("Earthquake", 50, 30.0),
            ("Blizzard", 20, 45.0),
        ];
        let (kind, dmg, radius) = KINDS[rand::thread_rng().gen_range(0..KINDS.len())];
        let Some(c) = self.cities.iter_mut().find(|c| c.id == id) else { return };
        let loss = rand::thread_rng().gen_range((dmg / 2).max(1)..=dmg);
        c.troops = (c.troops - loss).max(1);
        c.population = (c.population as f32 * rand::thread_rng().gen_range(0.92..0.98)) as i32;
        c.happiness = (c.happiness - rand::thread_rng().gen_range(5..16)).max(0);
        let (x, y, name) = (c.x, c.y, c.name.clone());
        let mut killed = 0;
        for u in &mut self.units {
            if u.alive && dist(u.x, u.y, x, y) < radius {
                u.alive = false;
                killed += 1;
            }
        }
        self.news(format!("{kind} strikes {name}! -{loss} troops, {killed} units lost"));
        self.cue(Cue::Disaster);
    }

    fn update_floods(&mut self) {
        self.flooded.retain(|_, t| {
            *t -= DAY_TICKS;
            *t > 0
        });
    }

    fn pirate_spawn(&mut self) {
        if self.year >= 1830 {
            return;
        }
        if self.pirate_cd > 0 {
            self.pirate_cd -= 1;
            return;
        }
        if self.pirates_spawned && self.cities.iter().any(|c| c.owner == "Pirates") {
            return;
        }
        if rand::random::<f32>() > rand::thread_rng().gen_range(0.01..0.05) {
            return;
        }
        let mut names = PIRATE_TARGETS.to_vec();
        names.sort_by_key(|_| rand::random::<u32>());
        let Some(idx) = names.iter().find_map(|n| self.cities.iter().position(|c| c.name == *n && c.owner != "Pirates")) else { return };
        let old = self.cities[idx].owner.clone();
        let name = self.cities[idx].name.clone();
        self.cities[idx].owner = "Pirates".into();
        self.cities[idx].is_capital = true;
        self.add_faction("Pirates", 15);
        if let Some(s) = self.materials.get_mut("Pirates") {
            s.food = 15;
            s.hide = 10;
        }
        self.eliminated.remove("Pirates");
        let id = self.cities[idx].id;
        self.spawn_unit(idx, id, "Line", true, false, "Attack");
        self.spawn_unit(idx, id, "Line", true, false, "Attack");
        self.declare_war("Pirates", &old, "");
        self.news(format!("Pirates seize {name} from {old}!"));
        self.cue(Cue::Pirates);
        self.pirates_spawned = true;
        self.pirate_cd = DAY_TICKS * rand::thread_rng().gen_range(100..600);
    }

    fn tick_relations(&mut self) {
        let speed = self.speed;
        let expired: Vec<(Pair, String, String)> = self
            .treaties
            .iter()
            .filter(|(_, ticks)| **ticks - speed <= 0)
            .filter_map(|(key, _)| self.war_names.get(key).map(|(a, b)| (*key, a.clone(), b.clone())))
            .collect();
        self.treaties.retain(|_, ticks| {
            *ticks -= speed;
            *ticks > 0
        });
        for (key, a, b) in expired {
            self.treaty_cd.insert(key, TREATY_COOLDOWN);
            self.news(format!("Treaty expired: {a} vs {b} — now neutral."));
            self.cue(Cue::Break);
        }
        self.treaty_cd.retain(|_, t| {
            *t -= speed;
            *t > 0
        });
        let ended: Vec<(String, String)> = self
            .alliances
            .iter()
            .filter(|(_, t)| **t - speed <= 0)
            .filter_map(|(k, _)| self.war_names.get(k).cloned())
            .collect();
        self.alliances.retain(|_, t| {
            *t -= speed;
            *t > 0
        });
        for (a, b) in ended {
            self.news(format!("Alliance between {a} & {b} has ended."));
            self.cue(Cue::Break);
        }
    }

    fn eliminations(&mut self) {
        let factions = self.factions.clone();
        for f in factions {
            if self.eliminated.contains(&f) {
                continue;
            }
            if f == "Pirates" && !self.pirates_spawned {
                continue;
            }
            if self.cities.iter().any(|c| c.owner == f) {
                self.surrendered.remove(&f);
                continue;
            }
            if !self.surrendered.contains(&f) {
                let victors: Vec<String> = self.active().into_iter().filter(|v| v != &f && self.at_war(&f, v)).collect();
                self.surrendered.insert(f.clone());
                if !victors.is_empty() {
                    self.news(format!("{f} has lost all cities — forced to surrender!"));
                    self.cue(Cue::Surrender);
                    for v in &victors {
                        let msg = format!("{v} dictates peace terms to defeated {f}.");
                        self.make_treaty(v, &f, &msg);
                    }
                }
                for u in &mut self.units {
                    if u.faction == f {
                        u.alive = false;
                    }
                }
                for m in &mut self.merchants {
                    if m.faction == f {
                        m.alive = false;
                    }
                }
            }
            if !self.cities.iter().any(|c| c.sovereign == f && c.owner != f) {
                self.eliminated.insert(f.clone());
                self.news(format!("{f} has been eliminated from the game!"));
                self.paper(&format!("{f} Eliminated!"), &format!("The nation of {f} has been destroyed."));
                self.cue(Cue::Eliminated);
            }
        }
    }

    fn one_capital(&mut self) {
        let factions = self.active();
        for f in factions {
            let mut caps: Vec<usize> = self.cities.iter().enumerate().filter(|(_, c)| c.owner == f && c.is_capital).map(|(i, _)| i).collect();
            if caps.len() > 1 {
                for i in caps.drain(1..) {
                    self.cities[i].is_capital = false;
                }
            } else if caps.is_empty() {
                if let Some(i) = self.cities.iter().position(|c| c.owner == f) {
                    self.cities[i].is_capital = true;
                }
            }
        }
    }

    fn move_units(&mut self, maps: &Maps) {
        let speed = self.speed as f32;
        let n = self.units.len();
        for i in 0..n {
            if self.units[i].alive && self.units[i].opponent.is_none() {
                self.step_unit(i, maps, speed);
            }
        }
    }

    fn step_unit(&mut self, i: usize, maps: &Maps, game_speed: f32) {
        let faction = self.units[i].faction.clone();
        let target = self.units[i].target;
        let x = self.units[i].x;
        let y = self.units[i].y;
        self.units[i].ship = maps.is_water(x, y) && !self.cities.iter().any(|c| dist(c.x, c.y, x, y) < 10.0);
        if self.units[i].men * 10 < self.units[i].max_men * 3 && !self.units[i].retreating {
            self.units[i].retreating = true;
        }
        if self.units[i].retreating {
            if self.units[i].retreat.is_none() {
                self.units[i].retreat = self.nearest_city(&faction, x, y);
            }
            let dest = self.units[i].retreat;
            if let Some((tx, ty)) = dest {
                // Flee home plot-to-plot, a little faster than normal.
                let spd = self.unit_speed(i, maps) * game_speed * 1.3;
                let d = self.advance_along_path(i, maps, tx, ty, spd);
                if d <= 8.0 {
                    self.units[i].men = self.units[i].max_men;
                    self.units[i].retreating = false;
                    self.units[i].retreat = None;
                    self.units[i].idle = true;
                    self.units[i].path.clear();
                    self.units[i].path_goal = -1;
                }
            } else {
                self.units[i].alive = false;
            }
            return;
        }

        // Peacetime standing orders decide where a unit without an enemy goes.
        match self.units[i].behavior.as_str() {
            "Guard" => {
                self.step_guard(i, maps, game_speed);
                return;
            }
            "Patrol" => {
                self.step_patrol(i, maps, game_speed);
                return;
            }
            _ => {}
        }

        // Default "Attack" behavior: march on the target city.
        let Some((tx, ty, city_owner)) = self
            .cities
            .iter()
            .find(|c| c.id == target)
            .map(|c| (c.x, c.y, c.owner.clone()))
        else {
            self.units[i].idle = true;
            return;
        };
        if city_owner != faction && self.at_peace(&faction, &city_owner) {
            self.units[i].idle = true;
            return;
        }
        let spd = self.unit_speed(i, maps) * game_speed;
        let d = self.advance_along_path(i, maps, tx, ty, spd);
        if d > 5.0 {
            return;
        }
        if city_owner == faction {
            self.units[i].idle = !self.units[i].patrolling;
            return;
        }
        // phase_timer stays 0 until the first siege step, and later phases
        // leave it 0 only after siege_phase has already advanced.
        let announce = self.units[i].siege_phase <= 1 && self.units[i].phase_timer == 0;
        self.siege(i, target);
        if announce {
            self.cue(Cue::Siege);
        }
    }

    /// Moves unit `i` one step toward world point `(tx, ty)` by following a
    /// region-to-region path instead of a straight line. Returns the remaining
    /// straight-line distance to the final destination so callers can detect
    /// arrival. Ships (on water) ignore pathing and sail straight, since the
    /// region graph only covers land.
    fn advance_along_path(&mut self, i: usize, maps: &Maps, tx: f32, ty: f32, spd: f32) -> f32 {
        let x = self.units[i].x;
        let y = self.units[i].y;
        let final_d = dist(x, y, tx, ty);
        // Sailing or very close: head straight for the destination.
        if self.units[i].ship || final_d <= spd * 1.5 {
            let (nx, ny, _) = step_toward(x, y, tx, ty, spd);
            self.units[i].x = nx;
            self.units[i].y = ny;
            return dist(nx, ny, tx, ty);
        }
        let here = maps.region_at(x, y);
        let goal = maps.region_at(tx, ty);
        // Rebuild the plot path when the destination region changes, or when
        // the unit has walked into a new region with no waypoints left. An
        // empty path for the region we already searched (overseas target, or
        // already standing in the goal) is kept: searching again every tick
        // walked the whole territory graph for every such regiment.
        let goal_changed = self.units[i].path_goal != goal;
        let entered = self.units[i].path.is_empty() && here != goal && self.units[i].path_from != here;
        if goal >= 0 && (goal_changed || entered) {
            self.units[i].path = maps.region_path(here, goal);
            self.units[i].path_goal = goal;
            self.units[i].path_from = here;
        }
        // Drop any leading waypoints we've already reached (including the one
        // we're currently standing in) so we always aim at the NEXT plot.
        while let Some(&next) = self.units[i].path.first() {
            if next == here {
                self.units[i].path.remove(0);
            } else {
                break;
            }
        }
        // Aim at the next region's centroid, or the final destination if the
        // path is empty (same region, adjacent, or no overland route).
        let (wx, wy) = match self.units[i].path.first() {
            Some(&next) => maps.region_centroid_world(next).unwrap_or((tx, ty)),
            None => (tx, ty),
        };
        let (nx, ny, _) = step_toward(x, y, wx, wy, spd);
        self.units[i].x = nx;
        self.units[i].y = ny;
        dist(nx, ny, tx, ty)
    }

    /// Patrol order: march to the target city, then on arrival pick a different
    /// friendly city to march to, cycling the garrison through the nation's
    /// settlements. Falls back to idling at home if the nation has one city.
    fn step_patrol(&mut self, i: usize, maps: &Maps, game_speed: f32) {
        let faction = self.units[i].faction.clone();
        let target = self.units[i].target;
        let x = self.units[i].x;
        let y = self.units[i].y;
        // If an enemy settlement is now in reach, switch to attacking it.
        let range = if is_native(&faction) { NATIVE_MAX_RANGE } else { UNIT_MAX_RANGE };
        if let Some(enemy) = self
            .cities
            .iter()
            .filter(|c| c.owner != faction && !self.at_peace(&faction, &c.owner) && dist(x, y, c.x, c.y) <= range)
            .min_by(|a, b| dist(a.x, a.y, x, y).total_cmp(&dist(b.x, b.y, x, y)))
            .map(|c| c.id)
        {
            self.units[i].behavior = "Attack".into();
            self.units[i].target = enemy;
            self.units[i].path.clear();
            self.units[i].path_goal = -1;
            self.units[i].idle = false;
            return;
        }
        let Some((tx, ty)) = self.cities.iter().find(|c| c.id == target && c.owner == faction).map(|c| (c.x, c.y)) else {
            // Target lost or no longer ours: pick a new friendly city.
            self.repoint_patrol(i, x, y);
            return;
        };
        let spd = self.unit_speed(i, maps) * game_speed;
        let d = self.advance_along_path(i, maps, tx, ty, spd);
        self.units[i].idle = false;
        if d <= 6.0 {
            // Reached this city; head to the next one.
            self.repoint_patrol(i, x, y);
        }
    }

    /// Chooses the next patrol destination: a friendly city other than the
    /// current one, preferring nearer cities so patrols sweep local territory.
    fn repoint_patrol(&mut self, i: usize, x: f32, y: f32) {
        let faction = self.units[i].faction.clone();
        let cur = self.units[i].target;
        let mut options: Vec<(u32, f32)> = self
            .cities
            .iter()
            .filter(|c| c.owner == faction && c.id != cur)
            .map(|c| (c.id, dist(x, y, c.x, c.y)))
            .collect();
        if options.is_empty() {
            // Only one city: sit tight there.
            self.units[i].idle = true;
            return;
        }
        options.sort_by(|a, b| a.1.total_cmp(&b.1));
        // Pick randomly among the nearest few so patrols vary their routes.
        let pick = &options[rand::thread_rng().gen_range(0..options.len().min(3))];
        self.units[i].target = pick.0;
        self.units[i].path.clear();
        self.units[i].path_goal = -1;
        self.units[i].idle = false;
    }

    /// Border-guard order: hold a frontier region. The unit anchors on a border
    /// region's centroid and stays within `GUARD_RADIUS` of it; if an enemy
    /// unit or settlement enters that circle it moves to intercept, then
    /// returns to its post. If the anchored region stops being a frontier, it
    /// re-anchors on another border region.
    fn step_guard(&mut self, i: usize, maps: &Maps, game_speed: f32) {
        const GUARD_RADIUS: f32 = 36.0 * COORD_SCALE;
        let faction = self.units[i].faction.clone();
        let x = self.units[i].x;
        let y = self.units[i].y;

        // Ensure we have a valid frontier anchor.
        let need_anchor = match (self.units[i].guard_region, self.units[i].guard_anchor) {
            (r, Some(_)) if r >= 0 => self.border_region_from(r, &faction, maps).is_none(),
            _ => true,
        };
        if need_anchor {
            let home = self.nearest_city(&faction, x, y).unwrap_or((x, y));
            let (region, pt) = self.guard_anchor_for(&faction, maps, home.0, home.1);
            self.units[i].guard_region = region;
            self.units[i].guard_anchor = Some(pt);
        }
        let (ax, ay) = self.units[i].guard_anchor.unwrap_or((x, y));

        // Intercept any hostile unit or settlement within the guarded circle.
        let intercept = self.nearest_threat(&faction, ax, ay, GUARD_RADIUS);
        let (tx, ty) = intercept.unwrap_or((ax, ay));
        let spd = self.unit_speed(i, maps) * game_speed;
        let d = self.advance_along_path(i, maps, tx, ty, spd);
        // Standing at the post counts as idle (plays the idle sprite); moving to
        // intercept or returning does not.
        self.units[i].idle = intercept.is_none() && d <= 8.0;
    }

    /// Nearest hostile unit or enemy settlement within `radius` of `(cx, cy)`,
    /// as a world point to move toward. Used by border guards to intercept.
    fn nearest_threat(&self, faction: &str, cx: f32, cy: f32, radius: f32) -> Option<(f32, f32)> {
        let mut best: Option<((f32, f32), f32)> = None;
        let mut consider = |px: f32, py: f32| {
            let d = dist(cx, cy, px, py);
            if d <= radius && best.as_ref().map(|(_, bd)| d < *bd).unwrap_or(true) {
                best = Some(((px, py), d));
            }
        };
        for u in &self.units {
            if u.alive && u.faction != faction && !self.at_peace(faction, &u.faction) {
                consider(u.x, u.y);
            }
        }
        for c in &self.cities {
            if c.owner != faction && !self.at_peace(faction, &c.owner) {
                consider(c.x, c.y);
            }
        }
        best.map(|(pt, _)| pt)
    }

    fn unit_speed(&self, i: usize, maps: &Maps) -> f32 {
        let u = &self.units[i];
        let mut spd = u.speed;
        if u.ship {
            spd *= 2.0;
        } else {
            spd *= move_mult(maps, u.x, u.y, u.formation == "Square");
        }
        if u.faction == "Comanche" {
            spd *= 1.4;
        }
        spd
    }

    fn nearest_city(&self, faction: &str, x: f32, y: f32) -> Option<(f32, f32)> {
        self.cities.iter().filter(|c| c.owner == faction).min_by(|a, b| dist(a.x, a.y, x, y).total_cmp(&dist(b.x, b.y, x, y))).map(|c| (c.x, c.y))
    }

    fn siege(&mut self, i: usize, city_id: u32) {
        self.units[i].phase_timer += 1;
        self.units[i].siege_timer += 1;
        let phase = self.units[i].siege_phase;
        let power = self.units[i].power;
        if self.units[i].siege_timer >= DAY_TICKS && phase > 1 {
            self.units[i].siege_timer = 0;
            let dmg = match phase {
                2 => (power / 3).max(1),
                3 => power.max(2),
                _ => (power * 2).max(3),
            };
            if let Some(c) = self.cities.iter_mut().find(|c| c.id == city_id) {
                c.troops -= dmg;
                c.regen_cooldown = 600;
                c.happiness = (c.happiness - 1).max(0);
            }
            if phase >= 3 {
                let loss = rand::thread_rng().gen_range(1..6);
                self.units[i].men = (self.units[i].men - loss).max(1);
            }
        }
        if self.units[i].phase_timer >= DAY_TICKS * 5 && self.units[i].siege_phase < 4 {
            self.units[i].siege_phase += 1;
            self.units[i].phase_timer = 0;
        }
        if self.units[i].men * 4 <= self.units[i].max_men {
            self.units[i].retreating = true;
            return;
        }
        let fallen = self.cities.iter().find(|c| c.id == city_id).is_some_and(|c| c.troops <= 0);
        if fallen {
            self.capture(i, city_id);
        }
    }

    fn capture(&mut self, i: usize, city_id: u32) {
        let faction = self.units[i].faction.clone();
        let Some(ci) = self.cities.iter().position(|c| c.id == city_id) else { return };
        let name = self.cities[ci].name.clone();
        if self.cities[ci].is_fort && !self.cities[ci].is_camp {
            self.cities.remove(ci);
            self.news(format!("{name} destroyed by {faction}!"));
            self.cue(Cue::Occupy);
            self.units[i].idle = true;
            self.claims_dirty = true;
            return;
        }
        let old = self.cities[ci].owner.clone();
        let was_capital = self.cities[ci].is_capital;
        self.cities[ci].owner = faction.clone();
        self.cities[ci].troops = max_troops(&self.cities[ci]);
        self.cities[ci].happiness = (self.cities[ci].happiness - 30).max(0);
        self.cities[ci].population = (self.cities[ci].population as f32 * 0.85).max(50.0) as i32;
        if was_capital {
            self.cities[ci].is_capital = false;
        }
        if name == "Moosonee" || name == "Moose Factory" {
            self.cities[ci].name = if faction == "Great Britain" { "Moose Factory".into() } else { "Moosonee".into() };
        }
        self.news(format!("{faction} occupies {name}!"));
        self.cue(Cue::Occupy);
        let mats = ["Gold", "Food", "Lumber", "Hide", "Iron", "Coal"];
        for mat in mats.iter().take(3) {
            self.transfer(&old, &faction, mat, rand::thread_rng().gen_range(5..25));
        }
        let key = self.remember(&faction, &old);
        self.pressure.insert(key, 0);
        if was_capital {
            if let Some(ni) = self.cities.iter().position(|c| c.owner == old) {
                self.cities[ni].is_capital = true;
                let new_name = self.cities[ni].name.clone();
                self.news(format!("{old} moves capital to {new_name}!"));
            }
            if rand::random::<f32>() < 0.5 {
                for c in &mut self.cities {
                    if c.owner == old {
                        c.owner = faction.clone();
                    }
                }
                for u in &mut self.units {
                    if u.faction == old {
                        u.alive = false;
                    }
                }
                self.news(format!("{old} surrenders to {faction}!"));
                self.cue(Cue::Surrender);
                let msg = format!("{faction} dictates peace terms to {old}.");
                self.make_treaty(&faction, &old, &msg);
            }
        }
        self.units[i].idle = true;
        self.units[i].siege_phase = 1;
        self.claims_dirty = true;
    }

    fn battles(&mut self, maps: &Maps) {
        let n = self.units.len();
        for i in 0..n {
            if !self.units[i].alive || self.units[i].opponent.is_some() || self.units[i].retreating {
                continue;
            }
            if self.units[i].men * 10 < self.units[i].max_men * 3 {
                continue;
            }
            let (x, y, faction, uid) = {
                let u = &self.units[i];
                (u.x, u.y, u.faction.clone(), u.uid)
            };
            for j in 0..n {
                if i == j || !self.units[j].alive || self.units[j].opponent.is_some() {
                    continue;
                }
                if self.units[j].faction == faction || dist(x, y, self.units[j].x, self.units[j].y) >= 15.0 {
                    continue;
                }
                if self.units[j].retreating {
                    let enemy = self.units[j].faction.clone();
                    self.fatigue_add(&faction, &enemy);
                    self.units[j].alive = false;
                    self.cue(Cue::Shot);
                    break;
                }
                if self.at_peace(&faction, &self.units[j].faction) {
                    continue;
                }
                let other = self.units[j].uid;
                self.units[i].opponent = Some(other);
                self.units[j].opponent = Some(uid);
                self.units[i].battle_phase = 1;
                self.units[j].battle_phase = 1;
                self.cue(Cue::Battle);
                break;
            }
        }
        let fighters: Vec<u32> = self.units.iter().filter(|u| u.alive && u.opponent.is_some()).map(|u| u.uid).collect();
        for uid in fighters {
            self.resolve_battle(uid, maps);
        }
    }

    fn resolve_battle(&mut self, uid: u32, maps: &Maps) {
        let Some(i) = self.units.iter().position(|u| u.uid == uid && u.alive) else { return };
        let Some(oid) = self.units[i].opponent else { return };
        let Some(j) = self.units.iter().position(|u| u.uid == oid && u.alive) else {
            if let Some(u) = self.units.iter_mut().find(|u| u.uid == uid) {
                u.opponent = None;
                u.battle_phase = 0;
            }
            return;
        };
        if uid > oid {
            return;
        }
        self.units[i].phase_timer += 1;
        let phase = self.units[i].battle_phase;
        let timer = self.units[i].phase_timer;
        if phase == 1 && timer >= DAY_TICKS * 7 {
            self.units[i].battle_phase = 2;
            self.units[j].battle_phase = 2;
            self.units[i].phase_timer = 0;
            self.units[j].phase_timer = 0;
            return;
        }
        if (phase == 2 || phase == 3 || phase == 4) && timer >= DAY_TICKS * 2 {
            let (ax, ay) = (self.units[i].x, self.units[i].y);
            let (bx, by) = (self.units[j].x, self.units[j].y);
            let ap = self.units[i].power.max(1) as f32 * self.units[i].men as f32 / self.units[i].max_men.max(1) as f32;
            let bp = self.units[j].power.max(1) as f32 * self.units[j].men as f32 / self.units[j].max_men.max(1) as f32;
            let scale = match phase {
                2 => 0.2,
                3 => 0.5,
                _ => 1.0,
            };
            let def_a = biome_mods(maps.biome_at(ax, ay)).5.max(0.25);
            let def_b = biome_mods(maps.biome_at(bx, by)).5.max(0.25);
            let loss_b = ((ap * scale) / def_b).max(1.0) as i32;
            let loss_a = ((bp * scale) / def_a).max(1.0) as i32;
            self.units[j].men = (self.units[j].men - loss_b).max(1);
            self.units[i].men = (self.units[i].men - loss_a).max(1);
            self.units[i].phase_timer = 0;
            self.units[i].skirmish_ticks += 1;
            if phase == 2 && self.units[i].skirmish_ticks >= 3 {
                self.units[i].battle_phase = 3;
                self.units[j].battle_phase = 3;
                self.units[i].skirmish_ticks = 0;
            } else if phase == 3 && self.units[i].skirmish_ticks >= 3 {
                self.units[i].battle_phase = 4;
                self.units[j].battle_phase = 4;
                self.units[i].skirmish_ticks = 0;
            }
        }
        for idx in [i, j] {
            if self.units[idx].men * 4 <= self.units[idx].max_men {
                let other = if idx == i { j } else { i };
                let loser = self.units[idx].faction.clone();
                let winner = self.units[other].faction.clone();
                self.fatigue_add(&loser, &winner);
                let survivors = self.units[idx].men;
                *self.manpower.entry(loser).or_insert(0) += survivors / 2;
                self.units[idx].opponent = None;
                self.units[other].opponent = None;
                self.units[idx].battle_phase = 0;
                self.units[other].battle_phase = 0;
                self.units[idx].retreating = true;
                break;
            }
        }
    }

    fn fatigue_add(&mut self, faction: &str, enemy: &str) {
        let key = self.remember(faction, enemy);
        let slot = self.fatigue.entry(key).or_default();
        *slot.entry(faction.into()).or_insert(0) += 1;
    }

    fn move_merchants(&mut self, maps: &Maps) {
        let speed = self.speed;
        let n = self.merchants.len();
        for i in 0..n {
            if !self.merchants[i].alive {
                continue;
            }
            let dest = self.merchants[i].dest;
            let Some(city) = self.cities.iter().find(|c| c.id == dest) else {
                self.merchants[i].alive = false;
                continue;
            };
            let (tx, ty, dest_owner) = (city.x, city.y, city.owner.clone());
            let (x, y) = (self.merchants[i].x, self.merchants[i].y);
            if self.merchants[i].status == 0 {
                self.merchants[i].ship = maps.is_water(x, y);
                self.merchants[i].timer += speed;
                if self.merchants[i].timer >= DAY_TICKS {
                    self.merchants[i].timer = 0;
                    let faction = self.merchants[i].faction.clone();
                    if !is_native(&faction) {
                        if let Some(s) = self.materials.get_mut(&faction) {
                            s.gold = (s.gold - 3).max(0);
                        }
                    }
                }
                let mut spd = 0.18 * speed as f32;
                if self.merchants[i].ship {
                    spd *= 2.0;
                } else {
                    spd *= move_mult(maps, x, y, false);
                }
                let (nx, ny, d) = step_toward(x, y, tx, ty, spd);
                if d > 5.0 {
                    self.merchants[i].x = nx;
                    self.merchants[i].y = ny;
                } else {
                    self.merchants[i].status = 1;
                    self.merchants[i].timer = 0;
                    self.merchants[i].x = tx;
                    self.merchants[i].y = ty;
                }
            } else if self.merchants[i].status == 1 {
                self.merchants[i].timer += speed;
                if self.merchants[i].timer >= DAY_TICKS * 2 {
                    let faction = self.merchants[i].faction.clone();
                    let material = self.merchants[i].material.clone();
                    let market = self.cities.iter().find(|c| c.id == self.merchants[i].source).is_some_and(|c| has(&c.buildings, "Market"));
                    let mult = if market { 1.25 } else { 1.0 };
                    self.touch(&faction);
                    self.touch(&dest_owner);
                    if dest_owner == faction {
                        if let Some(s) = self.materials.get_mut(&faction) {
                            s.gold += (5.0 * mult) as i32;
                        }
                    } else {
                        if let Some(s) = self.materials.get_mut(&faction) {
                            s.gold += (8.0 * mult) as i32;
                        }
                        let qty = if matches!(material.as_str(), "Coal" | "Iron") { 3 } else { 5 };
                        self.transfer(&faction, &dest_owner, &material, qty);
                    }
                    self.merchants[i].status = 2;
                    self.merchants[i].timer = 0;
                }
            } else {
                self.merchants[i].timer += speed;
                if self.merchants[i].timer >= DAY_TICKS * 5 {
                    let faction = self.merchants[i].faction.clone();
                    let here = self.merchants[i].dest;
                    let (x, y) = (self.merchants[i].x, self.merchants[i].y);
                    if let Some(next) = self.pick_trade_dest(&faction, here, x, y) {
                        self.merchants[i].source = here;
                        self.merchants[i].dest = next;
                        self.merchants[i].status = 0;
                        self.merchants[i].timer = 0;
                    }
                }
            }
        }
    }

    fn move_settlers(&mut self, maps: &Maps) {
        let speed = self.speed as f32;
        let n = self.settlers.len();
        for i in 0..n {
            if !self.settlers[i].alive {
                continue;
            }
            let (x, y, tx, ty) = (self.settlers[i].x, self.settlers[i].y, self.settlers[i].tx, self.settlers[i].ty);
            // Settlers can cross water: on sea they become a boat and travel at
            // sea speed; on land they march at biome-modified speed.
            let on_water = maps.is_water(x, y);
            let spd = if on_water { 0.16 * speed * 2.0 } else { 0.16 * speed * move_mult(maps, x, y, false) };
            let (nx, ny, d) = step_toward(x, y, tx, ty, spd);
            if d > 6.0 {
                // Move freely over both land and water now.
                self.settlers[i].x = nx;
                self.settlers[i].y = ny;
                // Draw/report as a boat whenever on water and not atop a city.
                self.settlers[i].ship =
                    maps.is_water(nx, ny) && !self.cities.iter().any(|c| dist(c.x, c.y, nx, ny) < 10.0);
                continue;
            }
            let faction = self.settlers[i].faction.clone();
            let native = self.settlers[i].native;
            let id = self.next_id;
            self.next_id += 1;
            // Stamp a fresh settle order so this settlement outranks any earlier
            // claim in the same region (most recent settler owns the region).
            self.settle_order += 1;
            let order = self.settle_order;
            let region = maps.region_at(tx, ty);
            self.cities.push(City {
                id,
                name: if native { format!("{faction} Camp") } else { format!("{faction} Fort") },
                x: tx,
                y: ty,
                owner: faction.clone(),
                sovereign: faction.clone(),
                troops: if native { 30 } else { 40 },
                tier: 0,
                is_capital: false,
                is_village: false,
                is_fort: true,
                is_camp: native,
                material: "Lumber".into(),
                buildings: Vec::new(),
                construction: None,
                spawn_cd: DAY_TICKS * 30,
                happiness: 80,
                regen_cooldown: 0,
                population: if native { 150 } else { 200 },
                pop_acc: 0.0,
                region,
                ethnicity: Vec::new(),
                religion: Vec::new(),
                aristocracy: if native { 0.0 } else { 0.06 },
                founded_order: order,
                founded_year: self.year,
                builder: faction.clone(),
            });
            // Seed the new settlement with its founder's own people and faith.
            if let Some(c) = self.cities.last_mut() {
                let owner = c.owner.clone();
                if is_native(&owner) {
                    c.ethnicity = vec![("indigenous".to_string(), 1.0)];
                    c.religion = vec![("animism".to_string(), 1.0)];
                } else {
                    c.ethnicity = vec![
                        (owner_ethnicity_key(&owner).to_string(), 0.9),
                        ("indigenous".to_string(), 0.1),
                    ];
                    c.religion = vec![(owner_religion_key(&owner).to_string(), 1.0)];
                }
            }
            self.settlers[i].alive = false;
            self.claims_dirty = true;
            self.news(format!("{faction} settles territory #{region}!"));
            self.cue(Cue::Founded);
        }
    }
}

#[allow(dead_code)]
fn rekey_map<V>(map: HashMap<Pair, V>, remap: &HashMap<Pair, Pair>) -> HashMap<Pair, V> {
    map.into_iter()
        .map(|(key, value)| (remap.get(&key).copied().unwrap_or(key), value))
        .collect()
}

fn has(list: &[String], name: &str) -> bool {
    list.iter().any(|b| b == name)
}

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = ax - bx;
    let dy = ay - by;
    (dx * dx + dy * dy).sqrt()
}

fn step_toward(x: f32, y: f32, tx: f32, ty: f32, spd: f32) -> (f32, f32, f32) {
    let dx = tx - x;
    let dy = ty - y;
    let d = (dx * dx + dy * dy).sqrt();
    if d <= spd || d < 0.001 {
        (tx, ty, d)
    } else {
        (x + dx / d * spd, y + dy / d * spd, d)
    }
}

fn move_mult(maps: &Maps, x: f32, y: f32, cavalry: bool) -> f32 {
    let b = maps.biome_at(x, y);
    let mut m = biome_mods(b).0;
    if cavalry {
        m = match b {
            5 => 0.25,
            2 => m * 0.5,
            0 | 1 => m * 0.75,
            _ => m,
        };
    }
    m
}

fn formation_cost(faction: &str, formation: &str) -> Vec<(&'static str, i32)> {
    if is_native(faction) {
        vec![("Food", 5)]
    } else {
        match formation {
            "Column" => vec![("Gold", 10)],
            "Square" => vec![("Gold", 25), ("Hide", 5)],
            _ => vec![("Gold", 15)],
        }
    }
}

fn regiment_name(faction: &str) -> String {
    let names: &[&str] = match faction {
        "France" | "Louisiana" | "Saint-Domingue" => &["Picardie", "Navarre", "La Marine", "Béarn", "Soissonnais", "Auvergne"],
        "Great Britain" => &["Coldstream", "Royal Scots", "The Buffs", "King's Own", "East Essex"],
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => &["León", "Granada", "Sevilla", "Aragón", "Galicia"],
        "Portugal" => &["Lagos", "Lippe", "Cascais", "Peniche", "Chaves"],
        "Netherlands" => &["Oranje", "Holland", "Zeeland", "Gelderland", "Friesland"],
        "Russia" => &["Siberian", "Moscow", "Azov", "Kiev", "Ingermanland"],
        "United States" => &["Continental", "Maryland", "New York", "Light Infantry"],
        "Mexico" => &["Supremos", "Allende", "Hidalgo", "Morelos"],
        "Confederate States" => &["Virginia", "1st Texas", "Army of Northern Virginia"],
        _ => &["War Party", "Company", "Regiment"],
    };
    let mut rng = rand::thread_rng();
    format!("{} {}", rng.gen_range(1..80), names[rng.gen_range(0..names.len())])
}

/// Pick a target for a settler: a land spot in a nearby region the faction
/// does NOT already own, so settling claims new territory. Returns None if no
/// suitable unowned region is found nearby (the settler is then skipped).
fn settle_spot(
    maps: &Maps,
    x: f32,
    y: f32,
    cities: &[City],
    owned: &std::collections::HashSet<i32>,
) -> Option<(f32, f32)> {
    let mut rng = rand::thread_rng();
    let mut fallback: Option<(f32, f32)> = None;
    for _ in 0..48 {
        let ang = rng.gen_range(0.0..std::f32::consts::TAU);
        let rad = rng.gen_range(70.0..260.0);
        let tx = x + ang.cos() * rad;
        let ty = y + ang.sin() * rad;
        if maps.is_water(tx, ty) {
            continue;
        }
        let region = maps.region_at(tx, ty);
        if region < 0 || owned.contains(&region) {
            continue;
        }
        // Don't drop a new settlement right on top of an existing one.
        if cities.iter().any(|c| dist(c.x, c.y, tx, ty) < 36.0) {
            // Still a valid unowned region — remember it as a loose fallback.
            if fallback.is_none() {
                fallback = Some((tx, ty));
            }
            continue;
        }
        return Some((tx, ty));
    }
    fallback
}

trait ChooseRand<T> {
    fn choose_rand(&self) -> Option<&T>;
}

impl<T> ChooseRand<T> for [T] {
    fn choose_rand(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(&self[rand::thread_rng().gen_range(0..self.len())])
        }
    }
}

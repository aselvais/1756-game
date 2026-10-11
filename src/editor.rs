//! Scenario map editor.
//!
//! A standalone mode for painting territory ownership onto the blank regions
//! map. It intentionally knows nothing about cities, units, or the running
//! simulation — the only thing it edits is "which country owns each region".
//! The result is saved to disk as a [`MapDef`] (region id -> country name) and
//! is meant to become the seed for official scenarios: you lay out who holds
//! what, save it, and a future scenario loader can read these files to place
//! each nation's starting territory.

use crate::config::faction_color;
use serde::{Deserialize, Serialize};

/// Every country that can be painted onto the map, in palette order. Mirrors
/// `art::codex_nations` but kept here so the editor owns its own list and can
/// grow independently if more scenario-only factions are added later.
pub const EDITOR_NATIONS: &[&str] = &[
    "Great Britain",
    "France",
    "Louisiana",
    "Saint-Domingue",
    "Spain",
    "Portugal",
    "Netherlands",
    "New Granada",
    "Peru",
    "Rio de la Plata",
    "New Spain",
    "Russia",
    "Denmark",
    "United States",
    "Confederate States",
    "Mexico",
    "Haiti",
    "Texas",
    "Canada",
    "Rupert's Land",
    "Iroquois",
    "Wabanaki",
    "Cherokee",
    "Comanche",
    "Cree",
    "Dakota",
];

/// A saved scenario map: for every owned region, the region id and the name of
/// the country that holds it. Regions not listed are unowned. Storing the
/// country *name* (not a palette index) keeps saves stable even if the palette
/// order changes later, and makes the JSON human-readable/editable.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MapDef {
    /// Optional human label for the map (e.g. "1754 Seven Years War").
    #[serde(default)]
    pub name: String,
    /// (region id, country name) for each owned region.
    pub regions: Vec<(i32, String)>,
}

/// Live editor state: one ownership slot per region plus the selected country
/// and palette scroll. `owners[r]` is `None` when region `r` is unowned, else
/// an index into [`EDITOR_NATIONS`].
pub struct Editor {
    pub owners: Vec<Option<usize>>,
    pub selected: usize,
    pub palette_scroll: f32,
    /// Transient status line (message, time shown in seconds).
    pub notice: Option<(String, f64)>,
    /// Set true whenever ownership changes so the caller re-renders the overlay.
    pub dirty: bool,
    /// When true the country palette collapses to a thin edge so the territory
    /// map underneath it can be reached and clicked.
    pub panel_hidden: bool,
}

impl Editor {
    /// Creates an empty editor sized for `region_count` regions.
    pub fn new(region_count: i32) -> Self {
        Self {
            owners: vec![None; region_count.max(0) as usize],
            selected: 0,
            palette_scroll: 0.0,
            notice: None,
            dirty: true,
            panel_hidden: false,
        }
    }

    /// Paints the selected country onto `region`. Clicking a region that the
    /// selected country already owns clears it instead (toggle), so the same
    /// click both assigns and erases.
    pub fn paint(&mut self, region: i32) {
        if region < 0 || region as usize >= self.owners.len() {
            return;
        }
        let slot = &mut self.owners[region as usize];
        *slot = if *slot == Some(self.selected) {
            None
        } else {
            Some(self.selected)
        };
        self.dirty = true;
    }

    /// Removes all ownership.
    pub fn clear(&mut self) {
        for slot in &mut self.owners {
            *slot = None;
        }
        self.dirty = true;
    }

    /// Per-region RGBA tint for `Maps::overlay`. Owned regions take their
    /// country color (semi-opaque to match the in-game overlay); unowned
    /// regions get `None` and render as neutral gray.
    pub fn overlay_colors(&self) -> Vec<Option<[u8; 4]>> {
        self.owners
            .iter()
            .map(|slot| {
                slot.map(|idx| {
                    let c = faction_color(EDITOR_NATIONS[idx]);
                    [c[0], c[1], c[2], 150]
                })
            })
            .collect()
    }

    /// Builds a [`MapDef`] snapshot of the current assignments for saving.
    pub fn to_mapdef(&self, name: &str) -> MapDef {
        let regions = self
            .owners
            .iter()
            .enumerate()
            .filter_map(|(r, slot)| slot.map(|idx| (r as i32, EDITOR_NATIONS[idx].to_string())))
            .collect();
        MapDef { name: name.to_string(), regions }
    }

    /// Replaces the current assignments with those from a loaded [`MapDef`].
    /// Region ids outside the current map are ignored; unknown country names
    /// are skipped. Returns how many regions were applied.
    pub fn apply_mapdef(&mut self, def: &MapDef) -> usize {
        for slot in &mut self.owners {
            *slot = None;
        }
        let mut applied = 0;
        for (region, nation) in &def.regions {
            if *region < 0 || *region as usize >= self.owners.len() {
                continue;
            }
            if let Some(idx) = EDITOR_NATIONS.iter().position(|n| n == nation) {
                self.owners[*region as usize] = Some(idx);
                applied += 1;
            }
        }
        self.dirty = true;
        applied
    }
}

impl MapDef {
    /// Writes the map to `path` as pretty JSON, creating parent dirs.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Loads a map previously written by [`MapDef::save`].
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

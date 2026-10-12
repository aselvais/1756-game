//! Sprites, flags, buildings, and paintings from `images/`, laid out the same
//! way `sprites.py` and `index.py` slice them.

use crate::config::scaled_font;
use macroquad::prelude::*;
use ::rand::seq::SliceRandom;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone)]
pub struct Anim {
    pub tex: Texture2D,
    clips: HashMap<&'static str, Vec<Rect>>,
}

impl Anim {
    pub fn blit(&self, clip: &str, tick: i32, speed: i32, cx: f32, cy: f32, width: f32) {
        let Some(frames) = self.clips.get(clip) else { return };
        if frames.is_empty() || width < 1.0 {
            return;
        }
        let src = frames[((tick.max(0) / speed.max(1)) as usize) % frames.len()];
        let height = width * src.h / src.w.max(1.0);
        draw_texture_ex(
            &self.tex,
            cx - width * 0.5,
            cy - height * 0.5,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(width, height)),
                source: Some(src),
                ..Default::default()
            },
        );
    }

    pub fn blit_size(&self, clip: &str, tick: i32, speed: i32, x: f32, y: f32, w: f32, h: f32) {
        let Some(frames) = self.clips.get(clip) else { return };
        if frames.is_empty() {
            return;
        }
        let src = frames[((tick.max(0) / speed.max(1)) as usize) % frames.len()];
        draw_texture_ex(
            &self.tex,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(w, h)),
                source: Some(src),
                ..Default::default()
            },
        );
    }
}

pub struct Marcher {
    pub x: f32,
    pub cav: bool,
    pub tex: Texture2D,
    pub frames: Vec<Rect>,
    pub flag: Option<Texture2D>,
    pub dead: bool,
}

pub struct Boom {
    pub x: f32,
    pub y: f32,
    pub tick: i32,
}

pub struct CodexVariant {
    pub label: &'static str,
    pub dates: &'static str,
    pub infantry: Option<&'static str>,
    pub cavalry: Option<&'static str>,
}

pub struct Art {
    pub frame: Texture2D,
    pub news: Texture2D,
    pub scenarios: Vec<Texture2D>,
    pub flags: HashMap<String, Texture2D>,
    pub buildings: HashMap<String, Texture2D>,
    pub farms: HashMap<String, Vec<Texture2D>>,
    pub biomes: HashMap<String, Vec<Texture2D>>,
    pub capitals: HashMap<String, Texture2D>,
    pub sheets: HashMap<String, Anim>,
    pub explosion: Vec<Texture2D>,
    pub smoke: Vec<Texture2D>,
    pub mountain: Option<Texture2D>,
    pub plantation: Option<Texture2D>,
    pub faces: HashMap<String, Texture2D>,
    pub ethnicities: HashMap<String, Texture2D>,
    pub rulers: HashMap<String, Texture2D>,
    pub religions: HashMap<String, Texture2D>,
    pub governments: HashMap<String, Texture2D>,
    pub portrait_bg: Option<Texture2D>,
}

impl Art {
    pub fn load(images: &Path) -> Self {
        let sheets = load_sheets(images);
        let mut flags = HashMap::new();
        for (name, file) in FLAG_FILES {
            // Flags are drawings, not pixel art. Linear filtering plus mipmaps
            // keeps a 5px map flag from sparkling as the camera moves.
            if let Some(tex) = load_tex(&images.join(file), FilterMode::Linear) {
                flags.insert((*name).to_string(), tex);
            }
        }
        let mut buildings = HashMap::new();
        for (name, file) in BUILDING_FILES {
            if let Some(tex) = load_tex(&images.join(file), FilterMode::Nearest) {
                buildings.insert((*name).to_string(), tex);
            }
        }
        let mut farms = HashMap::new();
        for (prefix, owners) in FARM_GROUPS {
            let mut imgs = Vec::new();
            for file in [
                format!("buildings/{prefix}farm.png"),
                format!("buildings/{prefix}farm1.png"),
                format!("buildings/{prefix}farm2.png"),
                format!("buildings/{prefix}farm3.png"),
            ] {
                let path = images.join(file);
                if path.exists() {
                    if let Some(tex) = load_tex(&path, FilterMode::Nearest) {
                        imgs.push(tex);
                    }
                }
            }
            for owner in *owners {
                if !imgs.is_empty() {
                    farms.insert((*owner).to_string(), imgs.clone());
                }
            }
        }
        let mut biomes = HashMap::new();
        for (name, files) in BIOME_FILES {
            let imgs: Vec<_> = files.iter().filter_map(|file| load_tex(&images.join(file), FilterMode::Linear)).collect();
            if !imgs.is_empty() {
                biomes.insert((*name).to_string(), imgs);
            }
        }
        let mut capitals = HashMap::new();
        for (name, file) in CAPITAL_FILES {
            if let Some(tex) = load_tex(&images.join(file), FilterMode::Linear) {
                capitals.insert((*name).to_string(), tex);
            }
        }
        let scenarios = SCENARIO_FILES.iter().filter_map(|file| load_tex(&images.join(file), FilterMode::Linear)).collect();
        let frame = load_tex(&images.join("frame.webp"), FilterMode::Linear).unwrap_or_else(|| blank(4, 4));
        let news = load_tex(&images.join("news.png"), FilterMode::Linear).unwrap_or_else(|| blank(8, 8));
        let explosion = load_gif(&images.join("explosion.gif"));
        let smoke = load_gif(&images.join("smoke.gif"));
        if let Some(tex) = load_tex(&images.join("continent.webp"), FilterMode::Linear) {
            flags.insert("Continental".to_string(), tex);
        }
        let mut faces = HashMap::new();
        for (name, file) in [("happy", "happyface.png"), ("med", "medface.png"), ("sad", "sadface.png")] {
            if let Some(tex) = load_tex(&images.join(file), FilterMode::Nearest) {
                faces.insert(name.to_string(), tex);
            }
        }
        let mut ethnicities = HashMap::new();
        for name in [
            "africans", "americans", "british", "danes", "french", "indigenous", "latinamericans", "russians",
            "spanish", "portugese", "dutch",
        ] {
            let file = format!("ethnicities/common{name}.png");
            if let Some(tex) = load_tex(&images.join(&file), FilterMode::Linear) {
                ethnicities.insert(name.to_string(), tex);
            }
        }
        // Leader portraits, keyed by file stem (e.g. "georgeiii"). Loaded from
        // images/rulers/. Not included in the bundled build.
        let mut rulers = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(images.join("rulers")) {
            for entry in entries.flatten() {
                let path = entry.path();
                let stem = path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_lowercase());
                let Some(stem) = stem else { continue };
                if let Some(tex) = load_tex(&path, FilterMode::Linear) {
                    rulers.insert(stem, tex);
                }
            }
        }
        let mut religions = HashMap::new();
        for name in ["animism", "catholic", "mesoamerican", "orthodox", "protestant", "voodoo"] {
            let file = format!("religions/{name}.png");
            if let Some(tex) = load_tex(&images.join(&file), FilterMode::Linear) {
                religions.insert(name.to_string(), tex);
            }
        }
        let mut governments = HashMap::new();
        for name in [
            "colonialgovernment", "confederation", "constitutionalmonarchy", "militarygovernment", "monarchy",
            "republic", "tribal",
        ] {
            let file = format!("governments/{name}.png");
            if let Some(tex) = load_tex(&images.join(&file), FilterMode::Linear) {
                governments.insert(name.to_string(), tex);
            }
        }
        let mountain = load_tex(&images.join("mountain.webp"), FilterMode::Linear);
        let plantation = load_tex(&images.join("buildings/plantation.png"), FilterMode::Nearest);
        let portrait_bg = load_tex(&images.join("portraitbackground.png"), FilterMode::Linear);
        Self { frame, news, scenarios, flags, buildings, farms, biomes, capitals, sheets, explosion, smoke, mountain, plantation, faces, ethnicities, rulers, religions, governments, portrait_bg }
    }

    pub fn marchers(&self) -> Vec<Marcher> {
        let mut groups = MARCH_GROUPS.to_vec();
        groups.shuffle(&mut ::rand::thread_rng());
        let mut column = Vec::new();
        let mut x = -60.0;
        for (inf, inf1800, cav, cav1800, flag, flag1800) in groups {
            let flag_tex = self.flags.get(flag).cloned();
            let flag1800_tex = self.flags.get(flag1800).cloned();
            x = push_group(&mut column, self.sheets.get(inf), false, flag_tex, x);
            x -= 200.0;
            x = push_group(&mut column, self.sheets.get(cav), true, None, x);
            x -= 200.0;
            x = push_group(&mut column, self.sheets.get(inf1800), false, flag1800_tex, x);
            x -= 200.0;
            x = push_group(&mut column, self.sheets.get(cav1800), true, None, x);
            x -= 400.0;
        }
        column
    }

    pub fn flag<'a>(&'a self, faction: &str, year: i32) -> Option<&'a Texture2D> {
        let key = match faction {
            // Spanish colonial puppets fly the Spanish flag.
            "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" if year >= 1785 => "Spain Kingdom",
            "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => "Spain",
            "France" | "Louisiana" if year >= 1792 => "France Republic",
            "Louisiana" => "France",
            // Saint-Domingue flies its own colonial flag.
            "Saint-Domingue" => "Saint-Domingue",
            other => other,
        };
        self.flags.get(key).or_else(|| self.flags.get(faction))
    }

    pub fn unit_sheet<'a>(&'a self, faction: &str, formation: &str, year: i32) -> Option<&'a Anim> {
        let cav = formation == "Square";
        let key = if cav { cavalry_key(faction, year) } else { infantry_key(faction, year) };
        self.sheets.get(key).or_else(|| {
            let other = if cav { infantry_key(faction, year) } else { cavalry_key(faction, year) };
            self.sheets.get(other)
        })
    }

    pub fn sheet<'a>(&'a self, key: &str) -> Option<&'a Anim> {
        self.sheets.get(key)
    }

    pub fn ship(&self) -> Option<&Anim> {
        self.sheets.get("ship")
    }

    pub fn canoe(&self) -> Option<&Anim> {
        self.sheets.get("canoe")
    }

    pub fn merchant<'a>(&'a self, faction: &str) -> Option<&'a Anim> {
        let key = if crate::config::is_native(faction) {
            "natmerchant"
        } else if faction == "Haiti" {
            "blackmerchant"
        } else {
            "merchant"
        };
        self.sheets.get(key)
    }

    /// Map icon. Matches `draw_city` in `index.py`. The architecture follows
    /// the settlement's original `builder` nation, not its current owner, so a
    /// conquered city keeps its founder's look (Quebec stays French, etc.).
    pub fn map_building<'a>(&'a self, builder: &str, camp: bool, fort: bool, village: bool, tier: i32) -> Option<&'a Texture2D> {
        let look = builder;
        let native = crate::config::is_native(look);
        let key = if camp || (native && village) || native {
            "camp"
        } else if fort {
            "fort"
        } else if matches!(look, "Great Britain" | "United States" | "Texas" | "Confederate States" | "Canada" | "Rupert's Land" | "Netherlands") {
            match tier {
                3.. => "brit3",
                2 => "brit2",
                _ => "brit",
            }
        } else if matches!(look, "France" | "Haiti" | "Louisiana" | "Saint-Domingue") {
            match tier {
                3.. => "fren3",
                2 => "fren2",
                _ => "fren",
            }
        } else if matches!(look, "Spain" | "Portugal" | "Mexico" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain") {
            match tier {
                3.. => "span3",
                2 => "span2",
                _ => "span",
            }
        } else if look == "Russia" {
            "rus"
        } else if look == "Denmark" {
            "dan"
        } else {
            "brit"
        };
        self.buildings.get(key)
    }

    pub fn button(&self, rect: Rect, label: &str, font: f32) {
        draw_texture_ex(
            &self.frame,
            rect.x,
            rect.y,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(rect.w, rect.h)), ..Default::default() },
        );
        // Scale the label but shrink it to fit the button if needed.
        let mut font = scaled_font(font);
        let mut width = measure_text(label, None, font as u16, 1.0).width;
        let max_w = rect.w - 12.0;
        if width > max_w && width > 0.0 {
            font *= max_w / width;
            width = measure_text(label, None, font as u16, 1.0).width;
        }
        draw_text(label, rect.x + (rect.w - width) * 0.5, rect.y + rect.h * 0.5 + font * 0.32, font, WHITE);
    }
}

pub fn gold_panel(x: f32, y: f32, w: f32, h: f32, alpha: u8) {
    draw_rectangle(x, y, w, h, Color::from_rgba(0, 0, 0, alpha));
    draw_rectangle_lines(x, y, w, h, 2.0, Color::from_rgba(120, 100, 50, 255));
    draw_rectangle_lines(x + 1.0, y + 1.0, w - 2.0, h - 2.0, 1.0, Color::from_rgba(180, 150, 60, 255));
}

pub fn outlined(text: &str, x: f32, top: f32, size: f32, color: Color) {
    let size = scaled_font(size);
    let dims = measure_text(text, None, size as u16, 1.0);
    let y = top + dims.offset_y;
    let dark = color.r + color.g + color.b < 100.0 / 255.0;
    let shade = if dark { WHITE } else { BLACK };
    for (ox, oy) in [(-1.0, -1.0), (-1.0, 0.0), (-1.0, 1.0), (0.0, -1.0), (0.0, 1.0), (1.0, -1.0), (1.0, 0.0), (1.0, 1.0)] {
        draw_text(text, x + ox, y + oy, size, shade);
    }
    draw_text(text, x, y, size, color);
}

/// `cy` is the vertical center of the text, matching pygame's center anchor.
pub fn outlined_center(text: &str, cx: f32, cy: f32, size: f32, color: Color) {
    // Measure at the scaled size so centering matches the scaled draw; pass the
    // base size to `outlined`, which applies the scale once when drawing.
    let dims = measure_text(text, None, scaled_font(size) as u16, 1.0);
    outlined(text, cx - dims.width * 0.5, cy - dims.height * 0.5, size, color);
}

pub fn land_clip(dx: f32, dy: f32) -> (&'static str, i32) {
    if dx.abs() < 5.0 && dy.abs() < 5.0 {
        return ("idle", 1);
    }
    if dx.abs() > dy.abs() {
        if dx > 0.0 {
            if dy > 20.0 { ("walk_br", 8) } else if dy < -20.0 { ("walk_tr", 8) } else { ("walk_right", 8) }
        } else if dy > 20.0 {
            ("walk_bl", 8)
        } else if dy < -20.0 {
            ("walk_tl", 8)
        } else {
            ("walk_left", 8)
        }
    } else if dy > 0.0 {
        ("walk_down", 8)
    } else {
        ("walk_up", 8)
    }
}

pub fn water_clip(dx: f32, dy: f32) -> &'static str {
    if dx.abs() > dy.abs() {
        if dx > 0.0 {
            if dy > 10.0 { "br" } else if dy < -10.0 { "tr" } else { "right" }
        } else if dy > 10.0 {
            "bl"
        } else if dy < -10.0 {
            "tl"
        } else {
            "left"
        }
    } else if dy > 0.0 {
        "down"
    } else {
        "up"
    }
}

pub fn codex_nations() -> &'static [&'static str] {
    &[
        "Great Britain", "France", "Louisiana", "Saint-Domingue", "Spain", "Portugal", "Netherlands", "New Granada", "Peru", "Rio de la Plata", "New Spain", "Russia", "Denmark", "United States", "Confederate States",
        "Mexico", "Haiti", "Texas", "Canada", "Iroquois", "Wabanaki", "Cherokee", "Comanche", "Cree", "Dakota",
    ]
}

pub fn codex_flag(nation: &str) -> &'static str {
    match nation {
        "Great Britain" => "Great Britain",
        "France" | "Louisiana" => "France",
        "Saint-Domingue" => "Saint-Domingue",
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => "Spain",
        "Portugal" => "Portugal",
        "Netherlands" => "Netherlands",
        "Russia" => "Russia",
        "Denmark" => "Denmark",
        "United States" => "United States",
        "Confederate States" => "Confederate States",
        "Mexico" => "Mexico",
        "Haiti" => "Haiti",
        "Texas" => "Texas",
        "Canada" => "Canada",
        "Iroquois" => "Iroquois",
        "Wabanaki" => "Wabanaki",
        "Cherokee" => "Cherokee",
        "Comanche" => "Comanche",
        "Cree" => "Cree",
        "Dakota" => "Dakota",
        _ => "Great Britain",
    }
}

pub fn codex_variants(nation: &str) -> &'static [CodexVariant] {
    match nation {
        // Rupert's Land, the Hudson's Bay Company charter, fields British
        // regulars, so it shares Britain's codex entry.
        "Great Britain" | "Rupert's Land" => &[
            CodexVariant { label: "Standard", dates: "1754-1800", infantry: Some("britinf"), cavalry: Some("britishcav") },
            CodexVariant { label: "1800s", dates: "1800+", infantry: Some("britinf1800"), cavalry: Some("britcav1800") },
        ],
        "France" | "Louisiana" | "Saint-Domingue" => &[
            CodexVariant { label: "Monarchy", dates: "1754-1792", infantry: Some("frenchinf"), cavalry: Some("frenchcav") },
            CodexVariant { label: "Republic", dates: "1792-1800", infantry: Some("frenchinfrv"), cavalry: Some("frenchcavrv") },
            CodexVariant { label: "1800s", dates: "1800+", infantry: Some("frenchinf1800"), cavalry: Some("frenchcav1800") },
        ],
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => &[
            CodexVariant { label: "Standard", dates: "1754-1800", infantry: Some("spaininf"), cavalry: Some("spanishcav") },
            CodexVariant { label: "1800s", dates: "1800+", infantry: Some("spaininf1800"), cavalry: Some("spaincav1800") },
        ],
        "Portugal" => &[
            CodexVariant { label: "Standard", dates: "1754-1800", infantry: Some("portinf"), cavalry: Some("portcav") },
            CodexVariant { label: "1800s", dates: "1800+", infantry: Some("portinf1800"), cavalry: Some("portcav1800") },
        ],
        "Netherlands" => &[
            CodexVariant { label: "Standard", dates: "1754-1800", infantry: Some("dutchinf"), cavalry: Some("dutchcav") },
            CodexVariant { label: "1800s", dates: "1800+", infantry: Some("dutchinf1800"), cavalry: Some("dutchcav1800") },
        ],
        "Russia" => &[
            CodexVariant { label: "Standard", dates: "1754-1800", infantry: Some("rusinf"), cavalry: Some("russiancav") },
            CodexVariant { label: "1800s", dates: "1800+", infantry: Some("rusinf1800"), cavalry: Some("ruscav1800") },
        ],
        "Denmark" => &[
            CodexVariant { label: "Standard", dates: "1754-1800", infantry: Some("daneinf"), cavalry: Some("danecav") },
            CodexVariant { label: "1800s", dates: "1800+", infantry: Some("daneinf1800"), cavalry: Some("danecav1800") },
        ],
        "United States" => &[
            CodexVariant { label: "Standard", dates: "1775-1800", infantry: Some("americaninf"), cavalry: Some("americancav") },
            CodexVariant { label: "1800s", dates: "1800-1850", infantry: Some("americaninf1800"), cavalry: Some("americancav1800") },
            CodexVariant { label: "Union", dates: "1850+", infantry: Some("unioninf"), cavalry: Some("unioncav") },
        ],
        "Mexico" => &[CodexVariant { label: "Standard", dates: "1810+", infantry: Some("mexinf"), cavalry: Some("mexcav") }],
        "Haiti" => &[CodexVariant { label: "Standard", dates: "1804+", infantry: Some("haitinf"), cavalry: Some("haiticav") }],
        "Texas" => &[CodexVariant { label: "Standard", dates: "1836+", infantry: Some("texinf"), cavalry: Some("texcav") }],
        "Confederate States" => &[CodexVariant { label: "Standard", dates: "1861+", infantry: Some("confedinf"), cavalry: Some("confedcav") }],
        "Iroquois" | "Wabanaki" | "Cherokee" => &[CodexVariant { label: "Warrior", dates: "All eras", infantry: Some("gunbearer"), cavalry: None }],
        "Comanche" => &[CodexVariant { label: "Warrior", dates: "All eras", infantry: Some("gunbearer"), cavalry: Some("horseman") }],
        "Cree" => &[CodexVariant { label: "Bowman", dates: "All eras", infantry: Some("bowman"), cavalry: None }],
        "Dakota" => &[CodexVariant { label: "Bowman", dates: "All eras", infantry: Some("bowman"), cavalry: Some("horseman") }],
        "Canada" => &[CodexVariant { label: "Standard", dates: "1867+", infantry: Some("caninf"), cavalry: Some("cancav") }],
        _ => &[CodexVariant { label: "Standard", dates: "", infantry: None, cavalry: None }],
    }
}

pub fn alt_flags(nation: &str) -> &'static [(&'static str, &'static str)] {
    match nation {
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => &[("Spain", "Bourbon (pre-1785)"), ("Spain Kingdom", "Kingdom (1785+)")],
        "France" => &[("France", "Monarchy"), ("France Republic", "Republic (1792+)")],
        "United States" => &[("Continental", "Continental"), ("United States", "Stars & Stripes")],
        _ => &[],
    }
}

fn infantry_key(faction: &str, year: i32) -> &'static str {
    match faction {
        "United States" if year >= 1850 => "unioninf",
        "United States" if year >= 1800 => "americaninf1800",
        "United States" => "americaninf",
        "France" | "Louisiana" | "Saint-Domingue" if (1792..1800).contains(&year) => "frenchinfrv",
        "France" | "Louisiana" | "Saint-Domingue" if year >= 1800 => "frenchinf1800",
        "France" | "Louisiana" | "Saint-Domingue" => "frenchinf",
        // Rupert's Land is a British chartered territory: it fields British
        // troops right up to Canadian Confederation (when its men become
        // Canada's). Canada proper keeps its own post-1867 look.
        "Great Britain" | "Rupert's Land" if year >= 1800 => "britinf1800",
        "Great Britain" | "Rupert's Land" => "britinf",
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" if year >= 1800 => "spaininf1800",
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => "spaininf",
        "Portugal" if year >= 1800 => "portinf1800",
        "Portugal" => "portinf",
        "Netherlands" if year >= 1800 => "dutchinf1800",
        "Netherlands" => "dutchinf",
        "Russia" if year >= 1800 => "rusinf1800",
        "Russia" => "rusinf",
        "Denmark" if year >= 1800 => "daneinf1800",
        "Denmark" => "daneinf",
        "Pirates" => "pirateinf",
        "Iroquois" | "Wabanaki" | "Cherokee" | "Comanche" => "gunbearer",
        "Cree" | "Dakota" => "bowman",
        "Canada" => "caninf",
        "Mexico" => "mexinf",
        "Haiti" => "haitinf",
        "Texas" => "texinf",
        "Confederate States" => "confedinf",
        _ => "britinf",
    }
}

fn cavalry_key(faction: &str, year: i32) -> &'static str {
    match faction {
        "United States" if year >= 1850 => "unioncav",
        "United States" if year >= 1800 => "americancav1800",
        "United States" => "americancav",
        "France" | "Louisiana" | "Saint-Domingue" if (1792..1800).contains(&year) => "frenchcavrv",
        "France" | "Louisiana" | "Saint-Domingue" if year >= 1800 => "frenchcav1800",
        "France" | "Louisiana" | "Saint-Domingue" => "frenchcav",
        "Great Britain" | "Rupert's Land" if year >= 1800 => "britcav1800",
        "Great Britain" | "Rupert's Land" => "britishcav",
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" if year >= 1800 => "spaincav1800",
        "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => "spanishcav",
        "Portugal" if year >= 1800 => "portcav1800",
        "Portugal" => "portcav",
        "Netherlands" if year >= 1800 => "dutchcav1800",
        "Netherlands" => "dutchcav",
        "Russia" if year >= 1800 => "ruscav1800",
        "Russia" => "russiancav",
        "Denmark" if year >= 1800 => "danecav1800",
        "Denmark" => "danecav",
        "Comanche" | "Dakota" => "horseman",
        "Canada" => "cancav",
        "Mexico" => "mexcav",
        "Haiti" => "haiticav",
        "Texas" => "texcav",
        "Confederate States" => "confedcav",
        _ => "britishcav",
    }
}

fn push_group(column: &mut Vec<Marcher>, anim: Option<&Anim>, cav: bool, flag: Option<Texture2D>, mut x: f32) -> f32 {
    let Some(anim) = anim else { return x };
    let Some(frames) = anim.clips.get("walk_right").cloned() else { return x };
    let count = if cav { 2 } else { 3 };
    let step = if cav { 290.0 } else { 150.0 };
    for i in 0..count {
        column.push(Marcher {
            x,
            cav,
            tex: anim.tex.clone(),
            frames: frames.clone(),
            flag: if i == 0 { flag.clone() } else { None },
            dead: false,
        });
        x -= step;
    }
    x
}

fn load_sheets(images: &Path) -> HashMap<String, Anim> {
    let mut sheets = HashMap::new();
    let inf = [
        "britinf", "britinf1800", "frenchinf", "frenchinfrv", "frenchinf1800", "rusinf", "rusinf1800",
        "spaininf", "spaininf1800", "americaninf", "americaninf1800", "unioninf", "pirateinf", "daneinf",
        "daneinf1800", "gunbearer", "bowman", "caninf", "mexinf", "haitinf", "texinf", "confedinf",
        "portinf", "portinf1800", "dutchinf", "dutchinf1800",
    ];
    for name in inf {
        let path = images.join(format!("sprites/{name}.png"));
        if let Some(anim) = load_infantry(&path) {
            sheets.insert(name.to_string(), anim);
        }
    }
    let cav = [
        "britishcav", "britcav1800", "frenchcav", "frenchcavrv", "frenchcav1800", "russiancav", "ruscav1800",
        "spanishcav", "spaincav1800", "americancav", "americancav1800", "unioncav", "danecav", "danecav1800",
        "horseman", "cancav", "mexcav", "haiticav", "texcav", "confedcav",
        "portcav", "portcav1800", "dutchcav", "dutchcav1800",
    ];
    for name in cav {
        let path = images.join(format!("sprites/{name}.png"));
        if let Some(anim) = load_cavalry(&path) {
            sheets.insert(name.to_string(), anim);
        }
    }
    if let Some(anim) = load_ship(&images.join("sprites/ship.png")) {
        sheets.insert("ship".into(), anim);
    }
    if let Some(anim) = load_canoe(&images.join("sprites/canoe.png")) {
        sheets.insert("canoe".into(), anim);
    }
    for name in ["merchant", "natmerchant", "blackmerchant"] {
        if let Some(anim) = load_merchant(&images.join(format!("sprites/{name}.png"))) {
            sheets.insert(name.to_string(), anim);
        }
    }
    // Governor and chief portrait sprites (idle/walk character layout).
    for name in ["govenor1", "govenor2", "govenor3", "govenor4", "blackgovenor", "natchief1", "natchief2"] {
        if let Some(anim) = load_infantry(&images.join(format!("sprites/{name}.png"))) {
            sheets.insert(name.to_string(), anim);
        }
    }
    sheets
}

fn load_infantry(path: &Path) -> Option<Anim> {
    let (tex, w, h) = open_sheet(path)?;
    let mut clips = HashMap::new();
    let names = ["idle", "walk_right", "walk_left", "walk_up", "walk_down", "walk_br", "walk_bl", "walk_tl", "walk_tr"];
    for (row, name) in names.iter().enumerate() {
        let count = if row == 0 { 1 } else { 2 };
        clips.insert(*name, row_frames(row as f32 * 40.0, 20.0, 40.0, count, w, h));
    }
    clips.insert("attack", row_frames(9.0 * 40.0, 30.0, 40.0, 4, w, h));
    Some(Anim { tex, clips })
}

fn load_cavalry(path: &Path) -> Option<Anim> {
    let (tex, w, h) = open_sheet(path)?;
    let mut y = 0.0;
    let mut clips = HashMap::new();
    let rows: &[(&str, i32, f32, f32)] = &[
        ("idle", 1, 20.0, 40.0),
        ("walk_right", 2, 40.0, 40.0),
        ("walk_left", 2, 40.0, 40.0),
        ("walk_down", 2, 20.0, 40.0),
        ("walk_up", 2, 20.0, 40.0),
        ("walk_br", 2, 30.0, 40.0),
        ("walk_bl", 2, 30.0, 40.0),
        ("walk_tl", 2, 30.0, 40.0),
        ("walk_tr", 2, 30.0, 40.0),
        ("attack", 6, 40.0, 40.0),
    ];
    for (name, count, fw, fh) in rows {
        clips.insert(*name, row_frames(y, *fw, *fh, *count, w, h));
        y += fh;
    }
    Some(Anim { tex, clips })
}

fn load_ship(path: &Path) -> Option<Anim> {
    let (tex, w, h) = open_sheet(path)?;
    let mut clips = HashMap::new();
    let rows: &[(&str, i32, f32)] = &[
        ("right", 2, 40.0),
        ("left", 2, 40.0),
        ("down", 2, 30.0),
        ("up", 2, 30.0),
        ("tl", 2, 40.0),
        ("tr", 2, 40.0),
        ("bl", 2, 40.0),
        ("br", 2, 40.0),
        ("attack", 4, 30.0),
    ];
    for (row, (name, count, fw)) in rows.iter().enumerate() {
        clips.insert(*name, row_frames(row as f32 * 40.0, *fw, 40.0, *count, w, h));
    }
    Some(Anim { tex, clips })
}

fn load_canoe(path: &Path) -> Option<Anim> {
    let (tex, w, h) = open_sheet(path)?;
    let mut y = 0.0;
    let mut clips = HashMap::new();
    let rows: &[(&str, f32, f32)] = &[
        ("right", 40.0, 30.0),
        ("left", 40.0, 30.0),
        ("down", 20.0, 40.0),
        ("up", 20.0, 40.0),
        ("br", 40.0, 30.0),
        ("bl", 40.0, 30.0),
        ("tl", 40.0, 30.0),
        ("tr", 40.0, 30.0),
    ];
    for (name, fw, fh) in rows {
        clips.insert(*name, row_frames(y, *fw, *fh, 2, w, h));
        y += fh;
    }
    Some(Anim { tex, clips })
}

fn load_merchant(path: &Path) -> Option<Anim> {
    let (tex, w, h) = open_sheet(path)?;
    let mut clips = HashMap::new();
    let names = ["walk_right", "walk_left", "walk_up", "walk_down", "walk_br", "walk_bl", "walk_tl", "walk_tr"];
    for (row, name) in names.iter().enumerate() {
        clips.insert(*name, row_frames(row as f32 * 40.0, 20.0, 40.0, 2, w, h));
    }
    let y8 = 8.0 * 40.0;
    clips.insert(
        "trading",
        vec![
            Rect::new(0.0, y8, 20.0, 40.0),
            Rect::new(20.0, y8, 30.0, 40.0),
            Rect::new(50.0, y8, 30.0, 40.0),
            Rect::new(80.0, y8, 20.0, 40.0),
        ],
    );
    let y9 = 9.0 * 40.0;
    clips.insert("resting", vec![Rect::new(0.0, y9, 30.0, 40.0), Rect::new(30.0, y9, 30.0, 40.0)]);
    let _ = (w, h);
    Some(Anim { tex, clips })
}

fn row_frames(y: f32, fw: f32, fh: f32, count: i32, sheet_w: f32, sheet_h: f32) -> Vec<Rect> {
    let mut frames = Vec::new();
    for i in 0..count {
        let x = i as f32 * fw;
        if x + fw <= sheet_w + 0.5 && y + fh <= sheet_h + 0.5 {
            frames.push(Rect::new(x, y, fw, fh));
        }
    }
    if frames.is_empty() {
        frames.push(Rect::new(0.0, 0.0, fw.min(sheet_w).max(1.0), fh.min(sheet_h).max(1.0)));
    }
    frames
}

fn open_sheet(path: &Path) -> Option<(Texture2D, f32, f32)> {
    let img = image::open(path).ok()?.to_rgba8();
    let w = img.width();
    let h = img.height();
    if w > u16::MAX as u32 || h > u16::MAX as u32 {
        return None;
    }
    let tex = Texture2D::from_rgba8(w as u16, h as u16, &img.into_raw());
    tex.set_filter(FilterMode::Nearest);
    Some((tex, w as f32, h as f32))
}

/// Longest side uploaded for a panel, flag, or portrait.
///
/// The pictures on disk are already near this size. The cap is a backstop so a
/// multi-megapixel photo cannot become a GPU texture and flash while it is
/// drawn into a small panel.
const UI_TEX_MAX: u32 = 1024;

fn fit_ui_image(img: image::DynamicImage, max_side: u32) -> image::RgbaImage {
    let w = img.width();
    let h = img.height();
    let side = w.max(h);
    if side <= max_side || w == 0 || h == 0 {
        return img.to_rgba8();
    }
    let scale = max_side as f32 / side as f32;
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    let rgba = img.to_rgba8();
    image::imageops::resize(&rgba, nw, nh, image::imageops::FilterType::Triangle)
}

fn load_tex(path: &Path, filter: FilterMode) -> Option<Texture2D> {
    let img = match image::open(path) {
        Ok(img) => img,
        Err(err) => {
            eprintln!("image skipped ({}): {err}", path.display());
            return None;
        }
    };
    let img = fit_ui_image(img, UI_TEX_MAX);
    let w = img.width();
    let h = img.height();
    if w > u16::MAX as u32 || h > u16::MAX as u32 || w == 0 || h == 0 {
        return None;
    }
    let raw = img.into_raw();
    if filter == FilterMode::Linear {
        // Mipmaps: drawing a photo smaller than its texture without them picks
        // one texel per screen pixel, and that choice flips between frames.
        Some(upload_linear(&raw, w, h))
    } else {
        let tex = Texture2D::from_rgba8(w as u16, h as u16, &raw);
        tex.set_filter(filter);
        Some(tex)
    }
}

/// Photo texture with a mip chain. `Texture2D::set_filter` turns mipmaps off,
/// so the sampler is set on the miniquad texture, same as the satellite map.
fn upload_linear(pixels: &[u8], w: u32, h: u32) -> Texture2D {
    use miniquad::{FilterMode, MipmapFilterMode, TextureAccess, TextureFormat, TextureKind, TextureParams, TextureSource, TextureWrap};
    let params = TextureParams {
        kind: TextureKind::Texture2D,
        format: TextureFormat::RGBA8,
        wrap: TextureWrap::Clamp,
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: MipmapFilterMode::Linear,
        width: w,
        height: h,
        allocate_mipmaps: true,
        sample_count: 1,
    };
    unsafe {
        let gl = macroquad::window::get_internal_gl();
        let id = gl.quad_context.new_texture(TextureAccess::Static, TextureSource::Bytes(pixels), params);
        gl.quad_context.texture_generate_mipmaps(id);
        gl.quad_context.texture_set_filter(id, FilterMode::Linear, MipmapFilterMode::Linear);
        Texture2D::from_miniquad_texture(id)
    }
}

fn load_gif(path: &Path) -> Vec<Texture2D> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    let reader = std::io::BufReader::new(file);
    let Ok(decoder) = image::codecs::gif::GifDecoder::new(reader) else { return Vec::new() };
    let frames = image::AnimationDecoder::into_frames(decoder);
    let mut out = Vec::new();
    for frame in frames {
        let Ok(frame) = frame else { break };
        let buf = frame.into_buffer();
        if buf.width() > u16::MAX as u32 || buf.height() > u16::MAX as u32 {
            continue;
        }
        let tex = Texture2D::from_rgba8(buf.width() as u16, buf.height() as u16, &buf.into_raw());
        tex.set_filter(FilterMode::Nearest);
        out.push(tex);
    }
    out
}

fn blank(w: u16, h: u16) -> Texture2D {
    Texture2D::from_rgba8(w, h, &vec![0; w as usize * h as usize * 4])
}

const FLAG_FILES: &[(&str, &str)] = &[
    ("France", "flags/france.png"),
    ("France Republic", "flags/francerp.webp"),
    ("Spain", "flags/spain.png"),
    ("Spain Kingdom", "flags/kingdomofspain.webp"),
    ("Portugal", "flags/portugal.webp"),
    ("Netherlands", "flags/netherlands.webp"),
    ("Great Britain", "flags/britian.png"),
    ("Russia", "flags/russia.webp"),
    ("Denmark", "flags/denmark.webp"),
    ("Iroquois", "flags/iroquois.png"),
    ("Wabanaki", "flags/wabanaki.png"),
    ("Cherokee", "flags/cherokee.png"),
    ("Comanche", "flags/comanche.png"),
    ("Cree", "flags/cree.webp"),
    ("Dakota", "flags/dakota.png"),
    ("Rupert's Land", "flags/rupertsland.webp"),
    ("Pirates", "flags/pirates.webp"),
    ("United States", "flags/usa.webp"),
    ("Mexico", "flags/mexico.png"),
    ("Haiti", "flags/haiti.png"),
    ("Saint-Domingue", "flags/saintdomingue.webp"),
    ("Texas", "flags/texas.webp"),
    ("Confederate States", "flags/confederatestates.png"),
    ("Canada", "flags/canada.png"),
];

const BUILDING_FILES: &[(&str, &str)] = &[
    ("fort", "buildings/woodfort.png"),
    ("camp", "buildings/nativecamp.png"),
    ("brit", "buildings/britcity.png"),
    ("brit2", "buildings/britcity2.png"),
    ("brit3", "buildings/britcity3.png"),
    ("span", "buildings/spancity.png"),
    ("span2", "buildings/spancity2.png"),
    ("span3", "buildings/spancity3.png"),
    ("fren", "buildings/frencity.png"),
    ("fren2", "buildings/frencity2.png"),
    ("fren3", "buildings/frencity3.png"),
    ("rus", "buildings/ruscity.png"),
    ("dan", "buildings/dancity.png"),
];

const FARM_GROUPS: &[(&str, &[&str])] = &[
    ("brit", &["Great Britain", "United States", "Texas", "Confederate States", "Netherlands"]),
    ("fren", &["France", "Haiti", "Louisiana", "Saint-Domingue"]),
    ("span", &["Spain", "Portugal", "Mexico", "New Granada", "Peru", "Rio de la Plata", "New Spain"]),
    ("rus", &["Russia"]),
    ("dan", &["Denmark"]),
];

const BIOME_FILES: &[(&str, &[&str])] = &[
    ("desert", &["biomes/desert.png", "biomes/desert2.png", "biomes/desert3.png"]),
    ("grassland", &["biomes/grassland.jpg", "biomes/grassland2.jpeg", "biomes/grassland3.webp"]),
    ("swamp", &["biomes/swamp.jpg", "biomes/swamp2.jpg", "biomes/swamp3.webp"]),
    ("taiga", &["biomes/taiga.png", "biomes/taiga2.jpg", "biomes/taiga3.jpg"]),
    ("temperate", &["biomes/temperate.png", "biomes/temperate2.jpg", "biomes/temperate3.webp"]),
    ("tropical", &["biomes/tropics.jpg", "biomes/tropics2.jpg", "biomes/tropics3.jpg"]),
    ("tundra", &["biomes/tundra.jpg", "biomes/tundra2.png", "biomes/tundra3.jpg"]),
    ("mountains", &["biomes/mountains.png", "biomes/mountains2.png", "biomes/mountains3.png"]),
    ("rainforest", &["biomes/rainforest.png", "biomes/rainforest2.png", "biomes/rainforest3.png"]),
];

const CAPITAL_FILES: &[(&str, &str)] = &[
    ("Mexico City", "biomes/mexicocity.jpg"),
    ("Quebec", "biomes/quebec.png"),
    ("Boston", "biomes/boston.png"),
];

const SCENARIO_FILES: &[&str] = &[
    "7yearswar.jpg",
    "continentalwars.png",
    "warof1812.png",
    "mexicanamericanwar.png",
    "americancivilwar.png",
];

const MARCH_GROUPS: &[(&str, &str, &str, &str, &str, &str)] = &[
    ("britinf", "britinf1800", "britishcav", "britcav1800", "Great Britain", "Great Britain"),
    ("frenchinf", "frenchinf1800", "frenchcav", "frenchcav1800", "France", "France Republic"),
    ("spaininf", "spaininf1800", "spanishcav", "spaincav1800", "Spain", "Spain"),
    ("portinf", "portinf1800", "portcav", "portcav1800", "Portugal", "Portugal"),
    ("dutchinf", "dutchinf1800", "dutchcav", "dutchcav1800", "Netherlands", "Netherlands"),
    ("rusinf", "rusinf1800", "russiancav", "ruscav1800", "Russia", "Russia"),
    ("daneinf", "daneinf1800", "danecav", "danecav1800", "Denmark", "Denmark"),
    ("americaninf", "americaninf1800", "americancav", "americancav1800", "United States", "United States"),
    ("mexinf", "mexinf", "mexcav", "mexcav", "Mexico", "Mexico"),
    ("haitinf", "haitinf", "haiticav", "haiticav", "Haiti", "Haiti"),
    ("texinf", "texinf", "texcav", "texcav", "Texas", "Texas"),
    ("confedinf", "confedinf", "confedcav", "confedcav", "Confederate States", "Confederate States"),
];


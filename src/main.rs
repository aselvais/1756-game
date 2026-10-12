//! Colonial Simulator — Rust port.
//!
//! Run from the repository with `cargo run --release`. Images are loaded from
//! the `images/` directory next to this crate. A Windows build with
//! `--features bundle` carries those images inside the executable.

mod art;
mod audio;
mod config;
mod cues;
mod editor;
mod sim;
mod world;

use art::{
    alt_flags, codex_flag, codex_nations, codex_variants, gold_panel, land_clip, outlined, outlined_center,
    water_clip, Art, Boom, Marcher,
};
use audio::Bank;
use config::{
    biome_mods, biome_name, building, faction_color, font_scale, government_of, is_elective, is_native,
    law_categories, owner_ethnicity_key, owner_religion_key, parties_of, scaled_font, set_font_scale, set_sound_volume,
    sound_volume, BUILDINGS, COORD_SCALE, DAY_TICKS, FONT_SCALE_MAX, FONT_SCALE_MIN, ISLANDS, MAP_H, MAP_W, PAN_SPEED,
    SOUND_VOLUME_MAX, SOUND_VOLUME_MIN,
};
use cues::Cue;
use editor::{Editor, MapDef, EDITOR_NATIONS};
use macroquad::prelude::*;
use ::rand::Rng;
use sim::{City, Game, Merchant, Unit};
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::path::PathBuf;
use world::Maps;

fn window_conf() -> Conf {
    Conf {
        window_title: "Colonial Simulator".to_owned(),
        fullscreen: false,
        window_width: 1600,
        window_height: 900,
        // Disable multisampling. This UI switches textures constantly, which
        // breaks draw batches; with MSAA on, macroquad does a multisample blit
        // at the end of every broken batch, and on many GPUs that resolve
        // produces intermittent dim/flicker frames. One sample avoids it.
        sample_count: 1,
        // Render at the monitor's real resolution. With this off, Windows
        // upscales a smaller framebuffer while the map moves, and that resample
        // reads as the background blinking during pan and zoom. Logical
        // coordinates stay the same: macroquad divides by the DPI scale.
        high_dpi: true,
        // Force vsync (swap interval 1). Without a locked swap interval the
        // window can present partially-composited frames, which shows up as
        // flicker on Windows.
        platform: miniquad::conf::Platform {
            swap_interval: Some(1),
            ..Default::default()
        },
        ..Default::default()
    }
}

enum Mode {
    Menu,
    Scenarios { scroll: f32 },
    Codex { nation: Option<String>, variant: i32 },
    Play,
    Paused,
    Editor,
}

struct App {
    maps: Maps,
    map_tex: Texture2D,
    art: Art,
    overlay: Option<Texture2D>,
    game: Game,
    mode: Mode,
    cam_x: f32,
    cam_y: f32,
    zoom: f32,
    selected: Option<usize>,
    selected_region: i32,
    panning: bool,
    pan_last: Vec2,
    news_scroll: i32,
    news_open: bool,
    fullscreen: bool,
    anim: i32,
    anim_accum: i32,
    sim_accum: i32,
    laws_open: bool,
    laws_category: usize,
    politics_open: bool,
    settings_open: bool,
    marchers: Vec<Marcher>,
    booms: Vec<Boom>,
    farms: HashMap<u32, Option<(f32, f32, usize)>>,
    plantations: HashMap<u32, Option<(f32, f32)>>,
    mountains: Vec<(f32, f32)>,
    /// Transient on-screen message after a save, with the time it was shown
    /// (seconds). Cleared automatically after a few seconds.
    save_notice: Option<(String, f64)>,
    /// Scenario map editor state, present only while in `Mode::Editor`.
    editor: Option<Editor>,
    /// Overlay texture for the editor's territory painting. Separate from the
    /// in-game `overlay` so the two never clobber each other.
    editor_overlay: Option<Texture2D>,
    /// Cached big faded nation-name labels (one per territorial nation),
    /// recomputed only when ownership changes (alongside `refresh_overlay`) so
    /// the per-frame draw stays cheap. See `rebuild_nation_labels`.
    nation_labels: Vec<NationLabel>,
    /// Synthesized effects. Empty if the audio device could not be opened.
    sfx: Bank,
}

/// One rendered character of a nation title, placed in world space. The glyph
/// path bends to follow the nation's land, so each glyph carries its own
/// position and rotation rather than the label being one straight run.
struct LabelGlyph {
    ch: char,
    /// Baseline-left anchor of the glyph, in world (native-map) coordinates.
    x: f32,
    y: f32,
    /// Glyph rotation in radians, following the local direction of the path.
    rot: f32,
}

/// A grand-strategy style country title laid out across a nation's land as a
/// sequence of individually placed, rotated glyphs. All glyph positions are in
/// native-map (world) space; the renderer converts to screen each frame so the
/// title tracks pan/zoom. The font size is stored in world units and scaled by
/// `zoom` at draw time.
struct NationLabel {
    glyphs: Vec<LabelGlyph>,
    /// Font size in WORLD units (multiplied by `zoom` when drawn).
    world_font: f32,
    color: [u8; 3],
    /// Bounding box of the glyph anchors in world space, for off-screen culling.
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

/// Real frames that pass between simulation steps at normal speed.
/// The simulation `tick` used to run every frame (~60/s), which made the
/// calendar, unit movement, and combat race far ahead of the Python game.
/// Stepping it once every `SIM_INTERVAL` frames slows the whole simulation
/// uniformly (date, movement, economy, war) while leaving every in-game
/// ratio untouched. Larger values are slower.
const SIM_INTERVAL: i32 = 4;

/// Draws a translucent seasonal color over the whole screen each frame.
/// Set to `false` to rule out the seasonal tint as a cause of screen flicker.
const SEASON_TINT: bool = true;

/// Real frames per sprite-animation step. Higher = slower animations.
const ANIM_SLOWDOWN: i32 = 10;

/// Below this zoom level, NPC sprites are drawn as simple colored shapes
/// instead of animated spritesheets. This is a level-of-detail optimization:
/// when the whole map is in view there can be hundreds of units, and the
/// cheap shapes keep the frame rate up. Zoom runs from ~1.0 (fit to width)
/// up to 8.0 (closest).
const LOD_ZOOM: f32 = 2.2;

/// Outlined city text is hidden while at least this much of the map width is
/// on screen. The fit-to-width opening view is 1.0, and that view has every
/// settlement visible. The outline draws each string nine times.
const NAME_SPAN: f32 = 0.55;

/// Garrison counts are drawn for every settlement, so they wait until the
/// camera is a little closer than capital names.
const TROOP_SPAN: f32 = 0.40;

#[macroquad::main(window_conf)]
async fn main() {
    #[cfg(windows)]
    install_panic_dialog();
    draw_text("Loading Colonial Simulator...", 48.0, 80.0, 36.0, WHITE);
    #[cfg(feature = "bundle")]
    draw_text("Preparing game files...", 48.0, 130.0, 24.0, WHITE);
    next_frame().await;
    let images = images_dir();
    let maps = Maps::load(&images).unwrap_or_else(|err| {
        eprintln!("{err}");
        panic!("could not load game images from {}", images.display());
    });
    let map_tex = satellite_texture(&maps.display, maps.w, maps.h);
    let art = Art::load(&images);
    let sfx = Bank::load().await;
    let marchers = art.marchers();
    let mountains = mountain_spots(&maps);
    let zoom = screen_width() / MAP_W as f32;
    let mut app = App {
        maps,
        map_tex,
        art,
        overlay: None,
        game: Game::new(),
        mode: Mode::Menu,
        cam_x: 0.0,
        cam_y: 0.0,
        zoom,
        selected: None,
        selected_region: -1,
        panning: false,
        pan_last: Vec2::ZERO,
        news_scroll: 0,
        news_open: true,
        fullscreen: false,
        anim: 0,
        anim_accum: 0,
        sim_accum: 0,
        laws_open: false,
        laws_category: 0,
        politics_open: false,
        settings_open: false,
        marchers,
        booms: Vec::new(),
        farms: HashMap::new(),
        plantations: HashMap::new(),
        mountains,
        save_notice: None,
        editor: None,
        editor_overlay: None,
        nation_labels: Vec::new(),
        sfx,
    };
    loop {
        app.frame();
        next_frame().await;
    }
}

impl App {
    fn frame(&mut self) {
        if is_key_pressed(KeyCode::F11) {
            self.fullscreen = !self.fullscreen;
            set_fullscreen(self.fullscreen);
        }
        let freeze = matches!(self.mode, Mode::Paused) || (matches!(self.mode, Mode::Play) && self.game.paper.is_some());
        if !freeze {
            // Advance sprite animations only every ANIM_SLOWDOWN frames so all
            // animations (units, ships, smoke, explosions, marchers) play back
            // much more slowly than the 60fps frame rate.
            self.anim_accum = self.anim_accum.wrapping_add(1);
            if self.anim_accum >= ANIM_SLOWDOWN {
                self.anim_accum = 0;
                self.anim = self.anim.wrapping_add(1);
            }
        }
        // Clear to a fully opaque color at the very start of every frame, on
        // every screen. macroquad does not clear automatically; without this,
        // stale framebuffer contents and a varying window alpha let the desktop
        // compositor bleed through, which shows up as a random dim flicker.
        clear_background(Color::from_rgba(10, 10, 30, 255));
        match self.mode {
            Mode::Menu => self.menu(),
            Mode::Scenarios { .. } => self.scenarios(),
            Mode::Codex { .. } => self.codex(),
            Mode::Play => self.play(false),
            Mode::Paused => self.play(true),
            Mode::Editor => self.editor_screen(),
        }
        self.play_cues();
    }

    fn play_cues(&mut self) {
        let cues = self.game.take_cues();
        self.sfx.play(&cues, sound_volume());
    }

    /// A button click. Plays the UI tick when the rectangle is pressed.
    fn press(&mut self, rect: Rect) -> bool {
        if left_click(rect) {
            self.game.cue(Cue::Click);
            true
        } else {
            false
        }
    }

    fn menu(&mut self) {
        self.backdrop();
        self.title_block();
        let killed = self.kill_marcher();
        let w = screen_width();
        let h = screen_height();
        let bx = w / 2.0 - 100.0;
        let by = h / 2.0 - 40.0;
        let step = 68.0;
        let play = Rect::new(bx, by, 200.0, 56.0);
        let codex = Rect::new(bx, by + step, 200.0, 56.0);
        let editor = Rect::new(bx, by + step * 2.0, 200.0, 56.0);
        let settings = Rect::new(bx, by + step * 3.0, 200.0, 56.0);
        let exit = Rect::new(bx, by + step * 4.0, 200.0, 56.0);
        self.art.button(play, "Play", 34.0);
        self.art.button(codex, "Codex", 34.0);
        self.art.button(editor, "Map Editor", 30.0);
        self.art.button(settings, "Settings", 34.0);
        self.art.button(exit, "Exit", 34.0);
        if self.settings_open {
            self.draw_settings_overlay();
            self.draw_march();
            return;
        }
        if !killed {
            if self.press(play) {
                self.mode = Mode::Scenarios { scroll: 0.0 };
            } else if self.press(codex) {
                self.mode = Mode::Codex { nation: None, variant: 0 };
            } else if self.press(editor) {
                self.open_editor();
            } else if self.press(settings) {
                self.settings_open = true;
            } else if self.press(exit) {
                std::process::exit(0);
            }
        }
        self.draw_march();
    }

    fn scenarios(&mut self) {
        self.backdrop();
        self.title_block();
        let killed = self.kill_marcher();
        let w = screen_width();
        let h = screen_height();
        label_center("Select Scenario", w * 0.5, h / 4.0 - 30.0, 36.0, WHITE);
        let mut scroll = match self.mode {
            Mode::Scenarios { scroll } => scroll,
            _ => 0.0,
        };
        let card_w = 320.0;
        let card_h = 240.0;
        let gap = 40.0;
        let count = 5.0;
        let max_scroll = (count * (card_w + gap) - gap - w + 100.0).max(0.0);
        scroll = (scroll - mouse_wheel().1 * 80.0).clamp(0.0, max_scroll);
        let start_x = 50.0 - scroll;
        let cy = h / 2.0 - card_h / 2.0;
        let cards = [
            (1754, "1754", "7 Years War"),
            (1790, "1790", "Continental Wars"),
            (1809, "1809", "War of 1812"),
            (1835, "1835", "Mexican American War"),
            (1858, "1858", "American Civil War"),
        ];
        for (i, (year, date, title)) in cards.iter().enumerate() {
            let cx = start_x + i as f32 * (card_w + gap);
            let rect = Rect::new(cx, cy, card_w, card_h);
            if let Some(tex) = self.art.scenarios.get(i) {
                stretch(tex, rect);
            } else {
                draw_rectangle(cx, cy, card_w, card_h, Color::from_rgba(20, 24, 32, 255));
            }
            draw_rectangle_lines(cx, cy, card_w, card_h, 2.0, Color::from_rgba(200, 180, 80, 255));
            shadow_center(date, cx + card_w / 2.0, cy + 8.0, 28.0, WHITE);
            shadow_center(title, cx + card_w / 2.0, cy + card_h - 36.0, 24.0, gold());
            if !killed && left_click(rect) {
                self.begin(*year);
                self.game.cue(Cue::Fanfare);
                return;
            }
        }
        if scroll > 0.0 {
            draw_triangle(vec2(20.0, h / 2.0), vec2(40.0, h / 2.0 - 20.0), vec2(40.0, h / 2.0 + 20.0), gold());
        }
        if scroll < max_scroll {
            draw_triangle(vec2(w - 20.0, h / 2.0), vec2(w - 40.0, h / 2.0 - 20.0), vec2(w - 40.0, h / 2.0 + 20.0), gold());
        }
        let back = Rect::new(w / 2.0 - 60.0, cy + card_h + 20.0, 120.0, 40.0);
        self.art.button(back, "Back", 20.0);
        label_center("Scroll to see more", w * 0.5, cy + card_h + 68.0, 16.0, Color::from_rgba(150, 150, 150, 255));
        if !killed && (self.press(back) || is_key_pressed(KeyCode::Escape)) {
            self.mode = Mode::Menu;
        } else if !killed {
            self.mode = Mode::Scenarios { scroll };
        }
        self.draw_march();
    }

    fn codex(&mut self) {
        self.backdrop();
        let (mut nation, mut variant) = match &self.mode {
            Mode::Codex { nation, variant } => (nation.clone(), *variant),
            _ => (None, 0),
        };
        let w = screen_width();
        let h = screen_height();
        let mut leave = false;
        if nation.is_none() {
            label_center("Codex — Nations", w * 0.5, 30.0, 36.0, gold());
            let nations = codex_nations();
            let cols = 5;
            let flag_sz = 80.0;
            let cell_w = 180.0;
            let cell_h = 130.0;
            let rows = (nations.len() + cols - 1) / cols;
            let grid_w = cols as f32 * cell_w;
            let grid_h = rows as f32 * cell_h;
            let gx = w / 2.0 - grid_w / 2.0;
            let gy = h / 2.0 - grid_h / 2.0;
            for (i, name) in nations.iter().enumerate() {
                let col = i % cols;
                let row = i / cols;
                let cell = Rect::new(gx + col as f32 * cell_w, gy + row as f32 * cell_h, cell_w, cell_h);
                let cx = cell.x + cell_w / 2.0;
                if let Some(tex) = self.art.flags.get(codex_flag(name)) {
                    stretch(tex, Rect::new(cx - flag_sz / 2.0, cell.y, flag_sz, flag_sz));
                }
                label_center(name, cx, cell.y + flag_sz + 5.0, 18.0, WHITE);
                if self.press(cell) {
                    nation = Some((*name).to_string());
                    variant = 0;
                }
            }
            let back = Rect::new(w / 2.0 - 60.0, gy + grid_h + 20.0, 120.0, 40.0);
            self.art.button(back, "Back", 20.0);
            if self.press(back) || is_key_pressed(KeyCode::Escape) {
                leave = true;
            }
        } else if let Some(sel) = nation.clone() {
            label_center(&format!("Codex — {sel}"), w * 0.5, 30.0, 36.0, gold());
            if let Some(tex) = self.art.flags.get(codex_flag(&sel)) {
                stretch(tex, Rect::new(w / 2.0 - 60.0, 70.0, 120.0, 120.0));
            }
            label_center(&sel, w * 0.5, 200.0, 32.0, WHITE);
            let alts = alt_flags(&sel);
            if !alts.is_empty() {
                label_center("Historical Flags:", w * 0.5, 235.0, 18.0, Color::from_rgba(200, 200, 200, 255));
                let mut fx = w / 2.0 - alts.len() as f32 * 50.0;
                for (key, label) in alts {
                    if let Some(tex) = self.art.flags.get(*key) {
                        stretch(tex, Rect::new(fx, 258.0, 50.0, 50.0));
                    }
                    label_center(label, fx + 25.0, 312.0, 14.0, Color::from_rgba(180, 180, 180, 255));
                    fx += 120.0;
                }
            }
            let variants = codex_variants(&sel);
            let n = variants.len().max(1) as i32;
            let idx = variant.rem_euclid(n) as usize;
            let var = &variants[idx];
            let unit_y = 350.0;
            if variants.len() > 1 {
                let left = Rect::new(w / 2.0 - 220.0, unit_y + 60.0, 30.0, 30.0);
                let right = Rect::new(w / 2.0 + 190.0, unit_y + 60.0, 30.0, 30.0);
                draw_triangle(vec2(left.x + left.w, left.y), vec2(left.x, left.y + left.h / 2.0), vec2(left.x + left.w, left.y + left.h), gold());
                draw_triangle(vec2(right.x, right.y), vec2(right.x + right.w, right.y + right.h / 2.0), vec2(right.x, right.y + right.h), gold());
                if self.press(left) {
                    variant -= 1;
                } else if self.press(right) {
                    variant += 1;
                }
            }
            label_center(&format!("{}  ({})", var.label, var.dates), w * 0.5, unit_y - 20.0, 18.0, gold());
            if let Some(key) = var.infantry {
                if let Some(anim) = self.art.sheet(key) {
                    anim.blit_size("walk_right", self.anim, 10, w / 2.0 - 150.0, unit_y + 10.0, 80.0, 160.0);
                }
                label_center("Infantry", w / 2.0 - 110.0, unit_y + 175.0, 18.0, Color::from_rgba(200, 200, 200, 255));
            }
            if let Some(key) = var.cavalry {
                if let Some(anim) = self.art.sheet(key) {
                    anim.blit_size("walk_right", self.anim, 10, w / 2.0 + 30.0, unit_y + 10.0, 160.0, 160.0);
                }
                label_center("Cavalry", w / 2.0 + 110.0, unit_y + 175.0, 18.0, Color::from_rgba(200, 200, 200, 255));
            }
            let back = Rect::new(w / 2.0 - 60.0, h - 80.0, 120.0, 40.0);
            self.art.button(back, "Back", 20.0);
            if self.press(back) {
                nation = None;
                variant = 0;
            } else if is_key_pressed(KeyCode::Escape) {
                leave = true;
            }
        }
        if leave {
            self.mode = Mode::Menu;
        } else {
            self.mode = Mode::Codex { nation, variant };
        }
    }

    fn begin(&mut self, year: i32) {
        self.game = Game::new();
        self.game.start_scenario(year, &self.maps);
        // The 1754 scenario's starting territories come from the map authored
        // in the editor, if one has been saved. Load it and install the
        // per-region claims so the painted borders show as the opening map.
        if year == 1754 {
            if let Ok(def) = MapDef::load(&map_path()) {
                self.game.set_region_claims(&def.regions, self.maps.region_count);
            }
        }
        self.seed_demographics();
        self.zoom = screen_width() / MAP_W as f32;
        self.cam_x = 0.0;
        self.cam_y = 0.0;
        self.selected = None;
        self.selected_region = -1;
        self.news_scroll = 0;
        self.news_open = true;
        self.farms.clear();
        self.plantations.clear();
        self.panning = false;
        self.game.claims_dirty = true;
        self.refresh_overlay();
        self.mode = Mode::Play;
    }

    /// Seeds each settlement's stored ethnicity and religion from the historical
    /// starting mix. After this, demographics live on the city and drift slowly
    /// over time (see `Game::drift_demographics`), rather than being recomputed
    /// from the current owner — so conquest does not instantly change a town's
    /// population.
    fn seed_demographics(&mut self) {
        let year = self.game.year;
        for i in 0..self.game.cities.len() {
            let eth: Vec<(String, f32)> = ethnic_mix(&self.game.cities[i], year)
                .into_iter()
                .map(|(key, _, share)| (key.to_string(), share))
                .collect();
            let rel: Vec<(String, f32)> = religion_mix(&self.game.cities[i], year)
                .into_iter()
                .map(|(key, _, share)| (key.to_string(), share))
                .collect();
            self.game.cities[i].ethnicity = eth;
            self.game.cities[i].religion = rel;
        }
    }

    fn play(&mut self, paused: bool) {
        let paper = self.game.paper.is_some();
        if !paper && !paused {
            self.map_clicks();
            self.camera_input();
            self.speed_keys();
            if is_key_pressed(KeyCode::Escape) {
                self.mode = Mode::Paused;
            }
            // Advance the simulation only every SIM_INTERVAL frames so the
            // whole game runs at a calmer, Python-like pace. Speed buttons
            // still scale how much each step advances.
            self.sim_accum += 1;
            if self.sim_accum >= SIM_INTERVAL {
                self.sim_accum = 0;
                self.game.tick(&self.maps);
                if self.game.claims_dirty {
                    self.refresh_overlay();
                    self.game.claims_dirty = false;
                }
            }
        }
        self.paint_world();
        if let Some((title, body)) = self.game.paper.clone() {
            if self.paint_paper(&title, &body) {
                self.game.paper = None;
                self.game.cue(Cue::Click);
            }
        }
        if paused {
            self.draw_pause();
        }
    }

    fn paint_world(&mut self) {
        clear_background(Color::from_rgba(10, 10, 30, 255));
        draw_map_layer(&self.map_tex, self.cam_x, self.cam_y, self.zoom);
        if let Some(overlay) = &self.overlay {
            draw_map_layer(overlay, self.cam_x, self.cam_y, self.zoom);
        }
        let tint = match self.game.season() {
            "Winter" => Some(Color::from_rgba(220, 230, 255, 25)),
            "Autumn" => Some(Color::from_rgba(255, 140, 40, 18)),
            "Summer" => Some(Color::from_rgba(255, 255, 180, 12)),
            _ => None,
        };
        if SEASON_TINT {
            if let Some(tint) = tint {
                draw_rectangle(0.0, 0.0, screen_width(), screen_height(), tint);
            }
        }
        self.draw_mountains();
        self.draw_nation_labels();
        self.paint_plots();
        self.draw_cities();
        self.draw_smoke();
        self.draw_armies();
        self.draw_merchants();
        self.draw_settlers();
        self.draw_hover();
        self.draw_city_panel();
        self.draw_governor_panel();
        self.draw_ethnicity_panel();
        self.draw_region_highlight();
        self.draw_region_panel();
        self.draw_news();
        self.draw_faction_panel();
        self.draw_leader_panel();
        self.draw_politics_panel();
        self.draw_date();
        self.draw_laws_panel();
    }

    fn backdrop(&self) {
        stretch(&self.map_tex, Rect::new(0.0, 0.0, screen_width(), screen_height()));
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::from_rgba(0, 0, 0, 120));
    }

    fn title_block(&self) {
        let w = screen_width();
        let h = screen_height();
        label_center("Colonial Simulator", w * 0.5, h / 4.0, 64.0, gold());
        label_center("Grand Strategy Colonial Simulation", w * 0.5, h / 4.0 + 60.0, 20.0, Color::from_rgba(200, 200, 200, 255));
    }

    fn kill_marcher(&mut self) -> bool {
        if !is_mouse_button_pressed(MouseButton::Left) {
            return false;
        }
        let (mx, my) = mouse_position();
        let h = screen_height();
        for marcher in &mut self.marchers {
            if marcher.dead {
                continue;
            }
            let (x, y, w, hh) = if marcher.cav {
                (marcher.x, h - 270.0, 260.0, 260.0)
            } else {
                (marcher.x, h - 250.0, 120.0, 240.0)
            };
            if mx >= x && mx <= x + w && my >= y && my <= y + hh {
                marcher.dead = true;
                self.game.cue(Cue::Shot);
                self.booms.push(Boom { x: x + w * 0.5 - 120.0, y: y + hh * 0.5 - 120.0, tick: 0 });
                return true;
            }
        }
        false
    }

    fn draw_march(&mut self) {
        let w = screen_width();
        let h = screen_height();
        // Single-file column: every marcher moves at the same speed, so their
        // spacing never changes while on screen. When one walks off the right
        // edge it is recycled to the BACK of the line (a fixed gap behind the
        // current left-most marcher) rather than a random spot, so marchers can
        // never land on top of each other no matter how long the menu runs.
        const MARCH_SPEED: f32 = 0.8;
        const MARCH_GAP: f32 = 300.0;
        let mut tail = self
            .marchers
            .iter()
            .filter(|m| !m.dead)
            .map(|m| m.x)
            .fold(f32::INFINITY, f32::min);
        if !tail.is_finite() {
            tail = 0.0;
        }
        for marcher in &mut self.marchers {
            if marcher.dead {
                continue;
            }
            marcher.x += MARCH_SPEED;
            if marcher.x > w + 160.0 {
                // Place behind the current tail and extend the tail so the next
                // recycled marcher queues up behind this one.
                tail -= MARCH_GAP;
                marcher.x = tail;
            }
        }
        for boom in &mut self.booms {
            boom.tick += 1;
        }
        let frames = self.art.explosion.len() as i32;
        self.booms.retain(|boom| frames > 0 && boom.tick / 3 < frames);
        let tick = self.anim;
        for marcher in &self.marchers {
            if marcher.dead || marcher.frames.is_empty() {
                continue;
            }
            let src = marcher.frames[((tick.max(0) / 12) as usize) % marcher.frames.len()];
            let (dw, dh, y) = if marcher.cav { (260.0, 260.0, h - 270.0) } else { (120.0, 240.0, h - 250.0) };
            draw_texture_ex(
                &marcher.tex,
                marcher.x,
                y,
                WHITE,
                DrawTextureParams { dest_size: Some(vec2(dw, dh)), source: Some(src), ..Default::default() },
            );
            if let Some(flag) = &marcher.flag {
                let fx = marcher.x + 60.0;
                let fy_top = h - 250.0 - 15.0;
                let fy_bottom = h - 250.0 + 65.0;
                draw_line(fx, fy_bottom, fx, fy_top, 3.0, Color::from_rgba(139, 90, 43, 255));
                draw_texture_ex(
                    flag,
                    fx - 70.0,
                    fy_top - 40.0,
                    WHITE,
                    DrawTextureParams { dest_size: Some(vec2(70.0, 70.0)), flip_x: true, ..Default::default() },
                );
            }
        }
        for boom in &self.booms {
            let idx = (boom.tick / 3) as usize;
            if let Some(tex) = self.art.explosion.get(idx) {
                stretch(tex, Rect::new(boom.x, boom.y, 240.0, 240.0));
            }
        }
    }

    fn camera_input(&mut self) {
        let (mx, my) = mouse_position();
        if is_mouse_button_pressed(MouseButton::Right) || is_mouse_button_pressed(MouseButton::Middle) {
            self.panning = true;
            self.pan_last = vec2(mx, my);
        }
        if is_mouse_button_released(MouseButton::Left)
            || is_mouse_button_released(MouseButton::Right)
            || is_mouse_button_released(MouseButton::Middle)
        {
            self.panning = false;
        }
        let dragging = self.panning
            && (is_mouse_button_down(MouseButton::Left)
                || is_mouse_button_down(MouseButton::Right)
                || is_mouse_button_down(MouseButton::Middle));
        if dragging {
            let now = vec2(mx, my);
            self.cam_x += now.x - self.pan_last.x;
            self.cam_y += now.y - self.pan_last.y;
            self.pan_last = now;
        }
        let wheel = mouse_wheel().1;
        if wheel.abs() > 0.0 {
            if mx > screen_width() - 426.0 {
                let max_scroll = (self.game.news.len() as i32 - 5).max(0);
                if wheel > 0.0 {
                    self.news_scroll += 1;
                } else {
                    self.news_scroll -= 1;
                }
                self.news_scroll = self.news_scroll.clamp(0, max_scroll);
            } else {
                let old = self.zoom;
                let min_z = screen_width() / MAP_W as f32;
                self.zoom = (self.zoom * (1.0 + wheel.signum() * 0.1)).clamp(min_z, min_z * 8.0);
                self.cam_x = mx - (mx - self.cam_x) * self.zoom / old;
                self.cam_y = my - (my - self.cam_y) * self.zoom / old;
            }
        }
        if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
            self.cam_x += PAN_SPEED;
        }
        if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
            self.cam_x -= PAN_SPEED;
        }
        if is_key_down(KeyCode::Up) || is_key_down(KeyCode::W) {
            self.cam_y += PAN_SPEED;
        }
        if is_key_down(KeyCode::Down) || is_key_down(KeyCode::S) {
            self.cam_y -= PAN_SPEED;
        }
        let min_x = screen_width() - MAP_W as f32 * self.zoom;
        let min_y = screen_height() - MAP_H as f32 * self.zoom;
        // Whole screen pixels. A fractional camera makes the satellite texture
        // resample on every pan step, which shimmers on the photo.
        self.cam_x = self.cam_x.round().clamp(min_x.min(0.0), 0.0);
        self.cam_y = self.cam_y.round().clamp(min_y.min(0.0), 0.0);
    }

    fn map_clicks(&mut self) {
        if !is_mouse_button_pressed(MouseButton::Left) {
            return;
        }
        let (mx, my) = mouse_position();
        // Laws side panel captures clicks that land inside it.
        if self.laws_open && self.laws_panel_rect().contains(vec2(mx, my)) {
            self.laws_click(mx, my);
            return;
        }
        // Politics side panel captures clicks that land inside it.
        if self.politics_open && self.politics_panel_rect().contains(vec2(mx, my)) {
            if self.politics_close_rect().contains(vec2(mx, my)) {
                self.politics_open = false;
                self.game.cue(Cue::Click);
            }
            return;
        }
        // Tabs on the faction panel (need a selection). Opening one closes the
        // other so the two side panels don't overlap.
        if self.selected.is_some() && self.laws_tab_rect().contains(vec2(mx, my)) {
            self.laws_open = !self.laws_open;
            if self.laws_open {
                self.politics_open = false;
            }
            self.game.cue(Cue::Click);
            return;
        }
        if self.selected.is_some() && self.politics_tab_rect().contains(vec2(mx, my)) {
            self.politics_open = !self.politics_open;
            if self.politics_open {
                self.laws_open = false;
            }
            self.game.cue(Cue::Click);
            return;
        }
        if self.news_toggle_rect().contains(vec2(mx, my)) {
            self.news_open = !self.news_open;
            self.game.cue(Cue::Click);
            return;
        }
        if self.speed_click(mx, my) || self.hud_hit(mx, my) {
            return;
        }
        let mut hit = None;
        let reach = (7.0 * self.zoom).max(5.0);
        for (i, city) in self.game.cities.iter().enumerate() {
            let (sx, sy) = self.world_to_screen(city.x, city.y);
            let dx = mx - sx;
            let dy = my - sy;
            if (dx * dx + dy * dy).sqrt() < reach {
                hit = Some(i);
                break;
            }
        }
        if hit.is_some() {
            if self.selected != hit {
                self.game.cue(Cue::Select);
            }
            self.selected = if self.selected == hit { None } else { hit };
            self.selected_region = -1;
        } else {
            self.selected = None;
            self.laws_open = false;
            self.politics_open = false;
            let wx = (mx - self.cam_x) / self.zoom;
            let wy = (my - self.cam_y) / self.zoom;
            let rid = self.maps.region_at(wx, wy);
            self.selected_region = if rid != self.selected_region { rid } else { -1 };
            self.panning = true;
            self.pan_last = vec2(mx, my);
        }
    }

    fn hud_hit(&self, mx: f32, my: f32) -> bool {
        let p = vec2(mx, my);
        if self.news_panel_rect().contains(p) {
            return true;
        }
        let layout = self.date_layout();
        if layout.frame.contains(p) || layout.coord.contains(p) {
            return true;
        }
        if self.laws_open && self.laws_panel_rect().contains(p) {
            return true;
        }
        if self.selected.is_some() {
            if Rect::new(6.0, 6.0, 420.0, 180.0).contains(p) {
                return true;
            }
            if Rect::new(6.0, 190.0, 430.0, 182.0).contains(p) {
                return true;
            }
        }
        if self.politics_open && self.politics_panel_rect().contains(p) {
            return true;
        }
        if self.selected.is_some() {
            if self.city_panel_rect().contains(p) {
                return true;
            }
            if self.ethnicity_panel_rect().contains(p) {
                return true;
            }
            if self.governor_panel_rect().contains(p) {
                return true;
            }
        }
        if let Some(rect) = self.region_panel_rect() {
            if rect.contains(p) {
                return true;
            }
        }
        false
    }

    fn speed_keys(&mut self) {
        let before = self.game.speed;
        if is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::Key0) {
            self.game.speed = 0;
        }
        if is_key_pressed(KeyCode::Key1) {
            self.game.speed = 1;
        }
        if is_key_pressed(KeyCode::Key2) {
            self.game.speed = 2;
        }
        if is_key_pressed(KeyCode::Key3) {
            self.game.speed = 3;
        }
        if is_key_pressed(KeyCode::Key4) {
            self.game.speed = 6;
        }
        if self.game.speed != before {
            self.game.cue(Cue::Click);
        }
    }

    fn speed_click(&mut self, mx: f32, my: f32) -> bool {
        if !is_mouse_button_pressed(MouseButton::Left) {
            return false;
        }
        let layout = self.date_layout();
        for (rect, spd) in layout.buttons {
            if rect.contains(vec2(mx, my)) {
                self.game.speed = spd;
                self.game.cue(Cue::Click);
                return true;
            }
        }
        false
    }

    fn world_to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.zoom + self.cam_x, y * self.zoom + self.cam_y)
    }

    fn city_xy(&self, id: u32) -> Option<(f32, f32)> {
        self.game.cities.iter().find(|city| city.id == id).map(|city| (city.x, city.y))
    }

    fn draw_mountains(&self) {
        let Some(tex) = &self.art.mountain else { return };
        let w = (14.0 * self.zoom).max(9.0);
        let h = (10.0 * self.zoom).max(6.0);
        let sw = screen_width();
        let sh = screen_height();
        for (x, y) in &self.mountains {
            let (sx, sy) = self.world_to_screen(*x, *y);
            if sx < -w || sy < -h || sx > sw + w || sy > sh + h {
                continue;
            }
            stretch(tex, Rect::new(sx - w * 0.5, sy - h * 0.5, w, h));
        }
    }

    fn paint_plots(&mut self) {
        let specs: Vec<(u32, f32, f32, i32, String, bool, bool, bool, bool)> = self
            .game
            .cities
            .iter()
            .map(|city| {
                let farm = city.buildings.iter().any(|b| b == "Farm");
                let plant = city.buildings.iter().any(|b| b == "Plantation");
                (city.id, city.x, city.y, city.region, city.owner.clone(), city.is_fort, city.is_camp, farm, plant)
            })
            .collect();
        for (id, x, y, region, owner, fort, camp, farm, plant) in specs {
            let (sx, sy) = self.world_to_screen(x, y);
            if !on_screen(sx, sy, 120.0) {
                continue;
            }
            if plant {
                self.ensure_plantation(id, x, y, region);
                if let Some((px, py)) = self.plantations.get(&id).copied().flatten() {
                    if let Some(tex) = &self.art.plantation {
                        let (psx, psy) = self.world_to_screen(px, py);
                        let ps = (4.0 * self.zoom).max(3.0);
                        stretch(tex, Rect::new(psx - ps * 0.5, psy - ps * 0.5, ps, ps));
                    }
                }
            }
            if farm && !fort && !camp {
                self.ensure_farm(id, x, y, region);
                if let Some((px, py, idx)) = self.farms.get(&id).copied().flatten() {
                    if let Some(imgs) = self.art.farms.get(&owner) {
                        if !imgs.is_empty() {
                            let tex = &imgs[idx % imgs.len()];
                            let (fsx, fsy) = self.world_to_screen(px, py);
                            let fs = (4.0 * self.zoom).max(3.0);
                            let sh = (fs * tex.height() / tex.width().max(1.0)).max(2.0);
                            stretch(tex, Rect::new(fsx - fs * 0.5, fsy - sh * 0.5, fs, sh));
                        }
                    }
                }
            }
        }
    }

    fn hunt_spot(&self, cx: f32, cy: f32, region: i32, avoid: Option<(f32, f32)>) -> Option<(f32, f32)> {
        let mut rng = ::rand::thread_rng();
        for _ in 0..60 {
            let angle = rng.gen_range(0.0..TAU);
            let dist = rng.gen_range(8.0..18.0) * COORD_SCALE;
            let px = cx + angle.cos() * dist;
            let py = cy + angle.sin() * dist;
            if px < 5.0 || py < 5.0 || px > self.maps.w as f32 - 5.0 || py > self.maps.h as f32 - 5.0 {
                continue;
            }
            if self.maps.is_water(px, py) {
                continue;
            }
            if region >= 0 && self.maps.region_at(px, py) != region {
                continue;
            }
            if let Some((ox, oy)) = avoid {
                let dx = px - ox;
                let dy = py - oy;
                if (dx * dx + dy * dy).sqrt() < 8.0 * COORD_SCALE {
                    continue;
                }
            }
            return Some((px, py));
        }
        None
    }

    fn ensure_plantation(&mut self, id: u32, cx: f32, cy: f32, region: i32) {
        if self.plantations.contains_key(&id) {
            return;
        }
        let spot = self.hunt_spot(cx, cy, region, None);
        self.plantations.insert(id, spot);
    }

    fn ensure_farm(&mut self, id: u32, cx: f32, cy: f32, region: i32) {
        if self.farms.contains_key(&id) {
            return;
        }
        let avoid = self.plantations.get(&id).copied().flatten();
        let spot = self.hunt_spot(cx, cy, region, avoid).map(|(x, y)| (x, y, ::rand::thread_rng().gen_range(0..3)));
        self.farms.insert(id, spot);
    }

    fn draw_cities(&self) {
        let year = self.game.year;
        // How much of the map width is on screen. 1.0 is the opening view.
        let span = screen_width() / self.zoom.max(0.0001) / MAP_W as f32;
        let show_names = span < NAME_SPAN;
        let show_troops = span < TROOP_SPAN;
        for city in &self.game.cities {
            let (sx, sy) = self.world_to_screen(city.x, city.y);
            if !on_screen(sx, sy, 120.0) {
                continue;
            }
            let (bw, bh) = building_size(city, self.zoom);
            let tex = self.art.map_building(&city.builder, city.is_camp, city.is_fort, city.is_village, city.tier);
            if let Some(tex) = tex {
                stretch(tex, Rect::new(sx - bw * 0.5, sy - bh * 0.5, bw, bh));
            } else {
                let c = faction_color(&city.owner);
                draw_circle(sx, sy, (5.0 * self.zoom).max(2.0), Color::from_rgba(c[0], c[1], c[2], 255));
            }
            let fs = (5.0 * self.zoom).max(3.0);
            if let Some(flag) = self.art.flag(&city.owner, year) {
                let top = if tex.is_some() { bh * 0.5 } else { 6.0 * self.zoom };
                stretch(flag, Rect::new(sx - fs * 0.5, sy - top - fs + 2.0, fs, fs));
            }
            let name_y = sy - (14.0 * self.zoom).max(10.0);
            // Only capitals show their name on the map — a displayed settlement
            // title is the cue that the city is its nation's capital. Every
            // other settlement is unlabeled (its name still shows in panels on
            // selection/hover). Both strings are skipped on the wide view.
            if show_names && city.is_capital {
                let tier = if city.tier > 0 { format!(" [T{}]", city.tier) } else { String::new() };
                outlined_center(&format!("{}{tier}", city.name), sx, name_y, 16.0, WHITE);
            }
            if show_troops {
                outlined_center(&city.troops.to_string(), sx, sy + (10.0 * self.zoom).max(8.0), 16.0, WHITE);
            }
            if show_names && city.owner != city.sovereign {
                let c = faction_color(&city.owner);
                outlined_center("(occupied)", sx, name_y + 12.0, 16.0, Color::from_rgba(c[0], c[1], c[2], 255));
            }
            if show_names && self.game.is_flooded(city.id) {
                outlined_center("(flooded)", sx, sy + (34.0 * self.zoom).max(34.0), 16.0, Color::from_rgba(80, 150, 255, 255));
            }
        }
    }

    fn draw_smoke(&self) {
        if self.art.smoke.is_empty() {
            return;
        }
        let frame = &self.art.smoke[((self.anim.max(0) / 10) as usize) % self.art.smoke.len()];
        for city in &self.game.cities {
            if city.is_camp && city.troops <= 0 {
                continue;
            }
            let max_hp = city_max_hp(city).max(1) as f32;
            let health = city.troops as f32 / max_hp;
            if health > 0.5 && city.regen_cooldown <= 0 {
                continue;
            }
            let (sx, sy) = self.world_to_screen(city.x, city.y);
            if !on_screen(sx, sy, 120.0) {
                continue;
            }
            let smoke = (7.0 * self.zoom).max(4.0);
            let sh = smoke * 1.3;
            let tint = Color::from_rgba(255, 255, 255, 210);
            if health < 0.25 {
                blit_tint(frame, sx - smoke - 6.0, sy - smoke * 1.5, smoke, sh, tint);
                blit_tint(frame, sx - smoke * 0.5 - 3.0, sy - smoke * 1.2, smoke, sh, tint);
            } else {
                blit_tint(frame, sx - smoke - 4.0, sy - smoke * 1.4, smoke, sh, tint);
            }
        }
    }

    fn draw_armies(&self) {
        for unit in &self.game.units {
            let (sx, sy) = self.world_to_screen(unit.x, unit.y);
            if !on_screen(sx, sy, 80.0) {
                continue;
            }
            // Level of detail: when zoomed out, draw a cheap shape instead of
            // the animated spritesheet and skip the combat effects.
            if self.zoom < LOD_ZOOM {
                self.draw_unit_blip(unit, sx, sy);
                continue;
            }
            let (dx, dy) = self.unit_aim(unit);
            let dist = (dx * dx + dy * dy).sqrt();
            let status = unit_status(unit, dist);
            if unit.ship && is_native(&unit.faction) {
                if let Some(canoe) = self.art.canoe() {
                    canoe.blit(water_clip(dx, dy), self.anim, 8, sx, sy, (15.0 * self.zoom).max(10.0));
                }
            } else if unit.ship {
                if let Some(ship) = self.art.ship() {
                    ship.blit(water_clip(dx, dy), self.anim, 8, sx, sy, (18.0 * self.zoom).max(12.0));
                }
            } else if let Some(sheet) = self.art.unit_sheet(&unit.faction, &unit.formation, self.game.year) {
                let (clip, speed, size) = if status == "Fighting" || status == "Sieging" {
                    ("attack", 6, (9.0 * self.zoom).max(6.0))
                } else if status == "Idle" || (status == "Guarding" && unit.idle) {
                    // A guard holding its post stands still like an idle unit.
                    ("idle", 1, (5.0 * self.zoom).max(3.0))
                } else {
                    let (clip, speed) = land_clip(dx, dy);
                    (clip, speed, (5.0 * self.zoom).max(3.0))
                };
                sheet.blit(clip, self.anim, speed, sx, sy, size);
            } else {
                let c = faction_color(&unit.faction);
                draw_circle(sx, sy, (3.0 * self.zoom).max(2.0), Color::from_rgba(c[0], c[1], c[2], 255));
            }
            if status == "Fighting" {
                if let Some(oid) = unit.opponent {
                    if unit.uid < oid {
                        if let Some((ox, oy)) = self.game.units.iter().find(|other| other.uid == oid).map(|other| self.world_to_screen(other.x, other.y)) {
                            let es = (12.0 * self.zoom).max(8.0);
                            self.blit_explosion(self.anim, 4, (sx + ox) * 0.5, (sy + oy) * 0.5, es);
                        }
                    }
                }
            } else if status == "Sieging" {
                let es = (6.0 * self.zoom).max(5.0);
                let n = 2 + ((unit.uid as i32 + self.anim / 8) % 3);
                for k in 0..n {
                    let ox = ((unit.uid as i32 * 17 + k * 13 + self.anim / 6) % 21 - 10) as f32;
                    let oy = ((unit.uid as i32 * 9 + k * 19 + self.anim / 5) % 21 - 10) as f32;
                    self.blit_explosion(self.anim + k * 3, 4, sx + ox, sy + oy, es);
                }
            }
        }
    }

    /// Low-detail army marker: a faction-colored shape. Land units are drawn
    /// as a filled square, ships as a diamond, so types stay distinguishable
    /// without any spritesheet work.
    fn draw_unit_blip(&self, unit: &Unit, sx: f32, sy: f32) {
        let c = faction_color(&unit.faction);
        let col = Color::from_rgba(c[0], c[1], c[2], 255);
        let r = (3.0 * self.zoom).max(2.5);
        if unit.ship {
            // Diamond.
            draw_triangle(vec2(sx, sy - r), vec2(sx + r, sy), vec2(sx - r, sy), col);
            draw_triangle(vec2(sx, sy + r), vec2(sx + r, sy), vec2(sx - r, sy), col);
        } else {
            draw_rectangle(sx - r, sy - r, r * 2.0, r * 2.0, col);
        }
    }

    /// Low-detail merchant marker: a small faction-colored circle.
    fn draw_merchant_blip(&self, merchant: &Merchant, sx: f32, sy: f32) {
        let c = faction_color(&merchant.faction);
        let r = (2.5 * self.zoom).max(2.0);
        draw_circle(sx, sy, r, Color::from_rgba(c[0], c[1], c[2], 220));
    }

    fn unit_aim(&self, unit: &Unit) -> (f32, f32) {
        if !unit.ship && unit.retreating {
            if let Some((x, y)) = unit.retreat {
                return (x - unit.x, y - unit.y);
            }
        }
        self.city_xy(unit.target).map(|(x, y)| (x - unit.x, y - unit.y)).unwrap_or((0.0, 0.0))
    }

    fn blit_explosion(&self, tick: i32, speed: i32, cx: f32, cy: f32, size: f32) {
        if self.art.explosion.is_empty() {
            return;
        }
        let tex = &self.art.explosion[((tick.max(0) / speed.max(1)) as usize) % self.art.explosion.len()];
        stretch(tex, Rect::new(cx - size * 0.5, cy - size * 0.5, size, size));
    }

    fn draw_merchants(&self) {
        for merchant in &self.game.merchants {
            let (sx, sy) = self.world_to_screen(merchant.x, merchant.y);
            if !on_screen(sx, sy, 80.0) {
                continue;
            }
            if self.zoom < LOD_ZOOM {
                self.draw_merchant_blip(merchant, sx, sy);
                continue;
            }
            let (dx, dy) = self.city_xy(merchant.dest).map(|(x, y)| (x - merchant.x, y - merchant.y)).unwrap_or((0.0, 0.0));
            if merchant.ship && is_native(&merchant.faction) {
                if let Some(canoe) = self.art.canoe() {
                    canoe.blit(water_clip(dx, dy), self.anim, 8, sx, sy, (9.0 * self.zoom).max(6.0));
                }
                continue;
            }
            if merchant.ship {
                if let Some(ship) = self.art.ship() {
                    ship.blit(water_clip(dx, dy), self.anim, 8, sx, sy, (15.0 * self.zoom).max(10.0));
                }
                continue;
            }
            let Some(sheet) = self.art.merchant(&merchant.faction) else {
                let c = faction_color(&merchant.faction);
                draw_circle(sx, sy, (3.0 * self.zoom).max(2.0), Color::from_rgba(c[0], c[1], c[2], 255));
                continue;
            };
            let (clip, speed, size) = match merchant.status {
                1 => ("trading", 6, (8.0 * self.zoom).max(6.0)),
                2 => ("resting", 10, (8.0 * self.zoom).max(6.0)),
                _ => {
                    let (clip, speed) = land_clip(dx, dy);
                    let clip = if clip == "idle" { "walk_right" } else { clip };
                    let speed = if clip == "walk_right" && speed == 1 { 1 } else { speed };
                    (clip, speed, (5.0 * self.zoom).max(3.0))
                }
            };
            sheet.blit(clip, self.anim, speed, sx, sy, size);
        }
    }

    fn draw_settlers(&self) {
        for settler in &self.game.settlers {
            let (sx, sy) = self.world_to_screen(settler.x, settler.y);
            if !on_screen(sx, sy, 40.0) {
                continue;
            }
            let c = faction_color(&settler.faction);
            let color = Color::from_rgba(c[0], c[1], c[2], 255);
            // Crossing water: draw as a boat (canoe for native factions, a ship
            // otherwise), oriented toward the destination, like units/merchants.
            if settler.ship {
                let (dx, dy) = (settler.tx - settler.x, settler.ty - settler.y);
                let drawn = if is_native(&settler.faction) {
                    self.art.canoe().map(|canoe| {
                        canoe.blit(water_clip(dx, dy), self.anim, 8, sx, sy, (12.0 * self.zoom).max(8.0));
                    })
                } else {
                    self.art.ship().map(|ship| {
                        ship.blit(water_clip(dx, dy), self.anim, 8, sx, sy, (15.0 * self.zoom).max(10.0));
                    })
                };
                if drawn.is_none() {
                    draw_circle(sx, sy, (3.0 * self.zoom).max(2.0), color);
                }
            } else {
                draw_circle(sx, sy, (3.0 * self.zoom).max(2.0), color);
            }
            if self.zoom >= 1.3 {
                let label = if settler.ship { "Settler (at sea)" } else { "Settler" };
                outlined_center(label, sx, sy - (12.0 * self.zoom).max(10.0), 16.0, WHITE);
                outlined_center(&settler.faction, sx, sy + (8.0 * self.zoom).max(6.0), 16.0, color);
            }
        }
    }

    fn draw_hover(&self) {
        let (mx, my) = mouse_position();
        let reach = (7.0 * self.zoom).max(5.0);
        for city in &self.game.cities {
            let (sx, sy) = self.world_to_screen(city.x, city.y);
            let dx = mx - sx;
            let dy = my - sy;
            if (dx * dx + dy * dy).sqrt() < reach {
                self.city_tooltip(city, mx, my);
                return;
            }
        }
        let ureach = (10.0 * self.zoom).max(6.0);
        for unit in &self.game.units {
            let (sx, sy) = self.world_to_screen(unit.x, unit.y);
            let dx = mx - sx;
            let dy = my - sy;
            if (dx * dx + dy * dy).sqrt() < ureach {
                self.unit_tooltip(unit, mx, my);
                return;
            }
        }
        for merchant in &self.game.merchants {
            let (sx, sy) = self.world_to_screen(merchant.x, merchant.y);
            let dx = mx - sx;
            let dy = my - sy;
            if (dx * dx + dy * dy).sqrt() < ureach {
                self.merchant_tooltip(merchant, mx, my);
                return;
            }
        }
    }

    fn city_tooltip(&self, city: &City, mx: f32, my: f32) {
        let biome = self.maps.biome_at(city.x, city.y);
        let kind = settlement_kind(city);
        let mut lines = vec![
            format!("{} ({kind})", city.name),
            format!("Founded: {}", founded_label(city)),
            format!("Owner: {}", city.owner),
            format!("Health: {}/{}", city.troops, city_max_hp(city)),
            format!("Tier: {}", city.tier),
            format!("Biome: {}", titled(biome_name(biome))),
            format!("Material: {} ({})", city.material, material_rate(city, biome, false)),
            format!("Food: {}", food_rate(city, biome)),
            format!("Gold Tax: +{}/day", gold_tax(city, biome, false)),
        ];
        if city.owner != city.sovereign {
            lines.push(format!("Occupied by: {}", city.owner));
        }
        let merchants = self.game.merchants.iter().filter(|m| m.source == city.id || m.dest == city.id).count();
        if merchants > 0 {
            lines.push(format!("Merchants: {merchants}"));
        }
        if city.buildings.is_empty() {
            lines.push("Buildings: None".into());
        } else {
            lines.push(format!("Buildings: {}", city.buildings.join(", ")));
        }
        if let Some((name, left)) = &city.construction {
            let days_left = (*left / DAY_TICKS).max(1);
            lines.push(format!("Building: {name} ({days_left}d left)"));
        }
        lines.push(format!("Storage Cap: {}", storage_cap(&self.game, &city.owner)));
        tooltip(&lines, mx, my);
    }

    fn unit_tooltip(&self, unit: &Unit, mx: f32, my: f32) {
        let (dx, dy) = self.unit_aim(unit);
        let dist = (dx * dx + dy * dy).sqrt();
        let kind = if unit.formation == "Square" { "Cavalry" } else { "Infantry" };
        let biome = titled(biome_name(self.maps.biome_at(unit.x, unit.y)));
        let mut lines = vec![
            unit.name.clone(),
            format!("Owner: {}", unit.faction),
            format!("Type: {kind} ({})", unit.formation),
            format!("HP: {}/{}", unit.men, unit.max_men),
            format!("Power: {}", unit.power),
            format!("Speed: {:.2}", unit.speed),
            format!("Tier: {}", unit.tier),
            format!("Status: {}", unit_status(unit, dist)),
            format!("Biome: {biome}"),
        ];
        if unit.ship {
            let mode = if is_native(&unit.faction) { "Canoe" } else { "Ship" };
            lines.push(format!("Mode: {mode}"));
        }
        if let Some(name) = self.game.cities.iter().find(|c| c.id == unit.target).map(|c| c.name.as_str()) {
            lines.push(format!("Target: {name}"));
        }
        if is_native(&unit.faction) {
            lines.push("Upkeep: 2 Food/day".into());
        } else if unit.formation == "Square" {
            lines.push("Upkeep: 6G + 8F/day".into());
        } else {
            lines.push("Upkeep: 3G + 5F/day".into());
        }
        tooltip(&lines, mx, my);
    }

    fn merchant_tooltip(&self, merchant: &Merchant, mx: f32, my: f32) {
        let status = match merchant.status {
            1 => "Trading",
            2 => "Resting",
            _ => "Travelling",
        };
        let mut lines = vec![
            format!("Merchant ({})", merchant.material),
            format!("Owner: {}", merchant.faction),
            format!("Status: {status}"),
            format!("Biome: {}", titled(biome_name(self.maps.biome_at(merchant.x, merchant.y)))),
        ];
        if merchant.ship {
            let mode = if is_native(&merchant.faction) { "Canoe" } else { "Ship" };
            lines.push(format!("Mode: {mode}"));
        }
        if let Some(name) = self.game.cities.iter().find(|c| c.id == merchant.source).map(|c| c.name.as_str()) {
            lines.push(format!("From: {name}"));
        }
        if let Some(name) = self.game.cities.iter().find(|c| c.id == merchant.dest).map(|c| c.name.as_str()) {
            lines.push(format!("To: {name}"));
        }
        if is_native(&merchant.faction) {
            lines.push("Upkeep: 1 Food/day".into());
        } else {
            lines.push("Upkeep: 2G + 1F/day".into());
            lines.push("Travel cost: 3G/day".into());
        }
        tooltip(&lines, mx, my);
    }

    fn draw_city_panel(&self) {
        let Some(index) = self.selected else { return };
        let Some(city) = self.game.cities.get(index) else { return };
        let rect = self.city_panel_rect();
        gold_panel(rect.x, rect.y, rect.w, rect.h, 220);
        let ix = rect.x + 240.0;
        let iy = rect.y + 5.0;
        self.draw_portrait(city, ix, iy);
        let biome = self.maps.biome_at(city.x, city.y);
        let kind = settlement_kind(city);
        let happiness = city.happiness;
        let hap_color = if happiness >= 75 {
            Color::from_rgba(50, 200, 50, 255)
        } else if happiness >= 25 {
            Color::from_rgba(220, 200, 50, 255)
        } else {
            Color::from_rgba(220, 50, 50, 255)
        };
        let mut lines = vec![
            (format!("{} ({kind})", city.name), gold()),
            (format!("Founded: {}", founded_label(city)), Color::from_rgba(210, 190, 150, 255)),
            (format!("Owner: {}", city.owner), WHITE),
            (format!("Region: #{}", city.region), Color::from_rgba(180, 200, 255, 255)),
            (format!("Population: {}", city.population), Color::from_rgba(200, 230, 255, 255)),
            (format!("Health: {}/{}", city.troops, city_max_hp(city)), WHITE),
            (format!("Tier: {}", city.tier), WHITE),
            (format!("Biome: {}", titled(biome_name(biome))), WHITE),
            (format!("Material: {} ({})", city.material, material_rate(city, biome, true)), WHITE),
            (format!("Gold Tax: +{}/day", gold_tax(city, biome, true)), Color::from_rgba(255, 220, 50, 255)),
            (format!("Storage: {}", storage_cap(&self.game, &city.owner)), Color::from_rgba(200, 200, 200, 255)),
            (
                {
                    // Show the trend arrow toward the dynamic happiness target.
                    let trend = self.game.happiness_target_of(city.id).map(|t| {
                        if t > happiness + 1 {
                            " (rising)"
                        } else if t < happiness - 1 {
                            " (falling)"
                        } else {
                            " (steady)"
                        }
                    }).unwrap_or("");
                    format!("Happiness: {happiness}{trend}")
                },
                hap_color,
            ),
        ];
        let hap_idx = lines.len() - 1;
        if let Some((name, left)) = &city.construction {
            let days = building(name).map(|b| b.days).unwrap_or(1).max(1);
            let total = days * DAY_TICKS;
            let pct = 100 - 100 * left / total.max(1);
            let days_left = (*left / DAY_TICKS).max(1);
            lines.push((format!("Building: {name} ({pct}% - {days_left}d left)"), Color::from_rgba(255, 180, 80, 255)));
        }
        lines.push(("[Click elsewhere to close]".into(), Color::from_rgba(150, 150, 150, 255)));
        let lx = rect.x + 8.0;
        let mut ly = rect.y + 8.0;
        for (text, color) in &lines {
            if ly > rect.y + rect.h - 16.0 {
                break;
            }
            text_top(text, lx, ly, 15.0, *color);
            ly += 15.0;
        }
        let face = if happiness >= 75 { "happy" } else if happiness >= 25 { "med" } else { "sad" };
        if let Some(tex) = self.art.faces.get(face) {
            let label = format!("Happiness: {happiness}");
            let tw = measure_text(&label, None, 15, 1.0).width;
            stretch(tex, Rect::new(lx + tw + 5.0, rect.y + 8.0 + hap_idx as f32 * 15.0, 14.0, 14.0));
        }
        let mut ry = rect.y + 8.0;
        let rx = rect.x + 500.0;
        text_top("-- Buildings --", rx, ry, 15.0, gold());
        ry += 15.0;
        if city.buildings.is_empty() {
            text_top("(none built)", rx, ry, 15.0, Color::from_rgba(150, 150, 150, 255));
            ry += 15.0;
        } else {
            for name in &city.buildings {
                if ry > rect.y + rect.h - 16.0 {
                    break;
                }
                text_top(&format!("[Built] {name}"), rx, ry, 15.0, Color::from_rgba(100, 255, 100, 255));
                ry += 15.0;
            }
        }
        if city.is_fort || city.is_camp {
            return;
        }
        ry += 4.0;
        text_top("-- Available --", rx, ry, 15.0, Color::from_rgba(200, 200, 255, 255));
        ry += 15.0;
        let native = is_native(&city.owner);
        let hall = if native { "Council Lodge" } else { "Town Hall" };
        let has_hall = city.buildings.iter().any(|b| b == hall);
        let biome_name = biome_name(biome);
        for spec in BUILDINGS {
            if city.buildings.iter().any(|b| b == spec.name) {
                continue;
            }
            if spec.colonial_only && native || spec.native_only && !native {
                continue;
            }
            if spec.name != "Town Hall" && spec.name != "Council Lodge" && spec.name != "Road" && !has_hall {
                continue;
            }
            if spec.name == "Plantation" && !matches!(biome_name, "tropical" | "swamp" | "temperate" | "grassland") {
                continue;
            }
            if spec.name == "Fur Trading Post" && !matches!(biome_name, "taiga" | "tundra") {
                continue;
            }
            if spec.name == "Road" && ISLANDS.contains(&city.name.as_str()) {
                continue;
            }
            if spec.name == "Lumber Mill" && city.material != "Lumber" {
                continue;
            }
            if ry > rect.y + rect.h - 16.0 {
                break;
            }
            let cost = cost_label(spec.name, native, spec.gold, spec.lumber, spec.hide, spec.iron, spec.coal, spec.food);
            if city.tier < spec.tier_req {
                text_top(&format!("{} [Tier {}]: {cost} ({}d)", spec.name, spec.tier_req, spec.days), rx, ry, 14.0, Color::from_rgba(100, 100, 100, 255));
                ry += 15.0;
            } else {
                text_top(&format!("{}: {cost} ({}d)", spec.name, spec.days), rx, ry, 14.0, Color::from_rgba(180, 180, 255, 255));
                ry += 15.0;
                let blurb = building_blurb(spec.name);
                if !blurb.is_empty() && ry <= rect.y + rect.h - 16.0 {
                    text_top(&format!("  {blurb}"), rx, ry, 13.0, Color::from_rgba(150, 200, 150, 255));
                    ry += 15.0;
                }
            }
        }
    }

    fn draw_portrait(&self, city: &City, x: f32, y: f32) {
        if let Some(tex) = self.art.capitals.get(&city.name) {
            stretch(tex, Rect::new(x, y, 250.0, 180.0));
        } else {
            let biome = biome_name(self.maps.biome_at(city.x, city.y));
            if let Some(imgs) = self.art.biomes.get(biome) {
                if !imgs.is_empty() {
                    let tex = &imgs[name_hash(&city.name) % imgs.len()];
                    stretch(tex, Rect::new(x, y, 250.0, 180.0));
                }
            }
            if let Some((key, w, h, ox, oy)) = panel_overlay(city) {
                if let Some(tex) = self.art.buildings.get(key) {
                    stretch(tex, Rect::new(x + ox, y + oy, w, h));
                }
            }
        }
        self.draw_panel_walkers(city, x, y);
    }

    /// Panel to the left of the city panel, shown when a settlement is
    /// selected. Shows the settlement's governor (or native chief) layered
    /// back-to-front: portraitbackground.png, the animated governor/chief
    /// sprite, then frame.webp over the top, with a title below.
    fn draw_governor_panel(&self) {
        let Some(index) = self.selected else { return };
        let Some(city) = self.game.cities.get(index) else { return };
        let rect = self.governor_panel_rect();
        gold_panel(rect.x, rect.y, rect.w, rect.h, 220);
        let native = is_native(&city.owner) || is_native(&city.sovereign);
        let title = if native { "Chief" } else { "Governor" };
        text_top(&format!("-- {title} --"), rect.x + 10.0, rect.y + 8.0, 15.0, gold());
        // Small square framed portrait near the top. Frame spans the outer
        // square; the background and sprite are inset to sit inside the border.
        let fw = 86.0;
        let fh = 86.0;
        let fx = rect.x + (rect.w - fw) / 2.0;
        let fy = rect.y + 28.0;
        let inset = fw * 0.16;
        let bx = fx + inset;
        let by = fy + inset;
        let bw = fw - inset * 2.0;
        let bh = fh - inset * 2.0;
        if let Some(bg) = &self.art.portrait_bg {
            stretch(bg, Rect::new(bx, by, bw, bh));
        } else {
            draw_rectangle(bx, by, bw, bh, Color::from_rgba(40, 32, 24, 255));
        }
        let key = governor_sheet(city);
        if let Some(sheet) = self.art.sheet(key) {
            let size = bh * 0.72;
            // Natural, desynced idle: offset each governor by a per-settlement
            // phase and advance at an irregular rate so frames don't tick in a
            // steady loop.
            let h = name_hash(&city.name) as i32;
            let t = self.anim + h % 97;
            let jitter = ((t / 7 + h) % 5) - 2;
            let wobbly = (t / 6 + jitter).max(0);
            sheet.blit("idle", wobbly, 1, bx + bw * 0.5, by + bh * 0.5, size);
        }
        stretch(&self.art.frame, Rect::new(fx, fy, fw, fh));

        // Name, ethnicity, then a list of random (but stable) traits.
        let (name, ethnicity, religion, popularity, traits) = governor_identity(city, self.game.year);
        let mut ty = fy + fh + 8.0;
        let cx = rect.x + rect.w / 2.0;
        shadow_center(&name, cx, ty, 15.0, gold());
        ty += 18.0;
        shadow_center(&format!("{ethnicity} - {religion}"), cx, ty, 12.0, Color::from_rgba(210, 220, 235, 255));
        ty += 18.0;
        // Popularity, color-graded from red (hated) through yellow to green.
        let pop_col = if popularity >= 25 {
            Color::from_rgba(100, 220, 100, 255)
        } else if popularity >= 0 {
            Color::from_rgba(220, 210, 110, 255)
        } else if popularity >= -25 {
            Color::from_rgba(230, 150, 80, 255)
        } else {
            Color::from_rgba(230, 90, 90, 255)
        };
        let sign = if popularity > 0 { "+" } else { "" };
        shadow_center(&format!("Popularity: {sign}{popularity}"), cx, ty, 13.0, pop_col);
        ty += 18.0;
        // Party affiliation (elective governments only). The governor belongs
        // to a party, weighted toward whichever is favored in their settlement.
        let nation = if city.sovereign.is_empty() { &city.owner } else { &city.sovereign };
        let (gov_key, _) = government_of(nation, self.game.year);
        if is_elective(gov_key) {
            if let Some(favor) = self.game.settlement_party_favor_of(city.id) {
                let parties = parties_of(nation);
                let party = governor_party(city, &favor);
                let colors = [
                    Color::from_rgba(90, 150, 230, 255),
                    Color::from_rgba(220, 90, 80, 255),
                    Color::from_rgba(90, 200, 130, 255),
                    Color::from_rgba(220, 190, 90, 255),
                ];
                shadow_center(&format!("{} Party", parties[party]), cx, ty, 12.0, colors[party]);
                ty += 18.0;
            }
        }
        ty += 2.0;
        text_top("Traits:", rect.x + 12.0, ty, 13.0, Color::from_rgba(255, 225, 150, 255));
        ty += 16.0;
        for (tr, effect) in traits {
            if ty + 28.0 > rect.y + rect.h - 6.0 {
                break;
            }
            text_top(&format!("- {tr}"), rect.x + 14.0, ty, 12.0, Color::from_rgba(255, 225, 150, 255));
            ty += 14.0;
            text_top(effect, rect.x + 22.0, ty, 11.0, Color::from_rgba(190, 220, 190, 255));
            ty += 15.0;
        }
    }

    /// Rectangle for the governor panel, placed just left of the city panel.
    fn governor_panel_rect(&self) -> Rect {
        let city = self.city_panel_rect();
        Rect::new(city.x - 200.0, city.y, 190.0, city.h)
    }

    fn draw_panel_walkers(&self, city: &City, x: f32, y: f32) {
        let Some(anim) = self.art.merchant(&city.owner) else { return };
        let n = 1 + name_hash(&city.name) % 5;
        for i in 0..n {
            let t = (self.anim as f32 * 0.7 + i as f32 * 57.0).rem_euclid(400.0);
            let (local, clip) = if t < 200.0 { (10.0 + t, "walk_right") } else { (10.0 + 400.0 - t, "walk_left") };
            anim.blit(clip, self.anim, 8, x + local, y + 150.0, 22.0);
        }
    }

    fn draw_faction_panel(&self) {
        let Some(index) = self.selected else { return };
        let Some(city) = self.game.cities.get(index) else { return };
        let owner = city.owner.clone();
        self.draw_glow(&owner);
        gold_panel(6.0, 6.0, 420.0, 180.0, 180);
        if let Some(flag) = self.art.flag(&owner, self.game.year) {
            stretch(flag, Rect::new(12.0, 30.0, 120.0, 120.0));
        }
        let c = faction_color(&owner);
        outlined(&owner, 140.0, 14.0, 22.0, Color::from_rgba(c[0], c[1], c[2], 255));
        // Puppet / overlord standing.
        if let Some(overlord) = self.game.overlord_of(&owner) {
            outlined(&format!("Puppet of {overlord}"), 140.0, 36.0, 14.0, Color::from_rgba(220, 160, 90, 255));
        } else {
            let puppets = self.game.puppets_of(&owner);
            if !puppets.is_empty() {
                outlined(&format!("Overlord of: {}", puppets.join(", ")), 140.0, 36.0, 14.0, Color::from_rgba(150, 220, 150, 255));
            }
        }
        let cities = self.game.cities.iter().filter(|c| c.owner == owner).count();
        let units = self.game.units.iter().filter(|u| u.faction == owner).count();
        let merchants = self.game.merchants.iter().filter(|m| m.faction == owner).count();
        let ucap = self.game.cities.iter().filter(|c| c.owner == owner && !c.is_fort && !c.is_camp).count();
        let markets = self.game.cities.iter().filter(|c| c.owner == owner && c.buildings.iter().any(|b| b == "Market")).count();
        let mcap = if is_native(&owner) { 1 } else { 1 + markets };
        outlined(&format!("Cities: {cities}  Units: {units}/{ucap}  Merchants: {merchants}/{mcap}"), 140.0, 52.0, 16.0, WHITE);
        let (gold_n, food, lumber, hide, iron, coal) = self.game.purse(&owner);
        outlined(&format!("Gold: {gold_n}  Food: {food}  Lumber: {lumber}"), 140.0, 76.0, 16.0, gold());
        outlined(&format!("Hide: {hide}  Iron: {iron}  Coal: {coal}"), 140.0, 92.0, 16.0, gold());
        outlined(&format!("Manpower: {}", self.game.manpower_of(&owner)), 140.0, 108.0, 16.0, Color::from_rgba(180, 220, 180, 255));
        let (wars, allies) = diplomacy(&self.game.relations(), &owner);
        let mut y = 126.0;
        if !wars.is_empty() {
            outlined(&format!("At war: {}", wars.join(", ")), 140.0, y, 16.0, Color::from_rgba(255, 80, 80, 255));
            y += 16.0;
        }
        if !allies.is_empty() && y < 168.0 {
            outlined(&format!("Allied: {}", allies.join(", ")), 140.0, y, 16.0, Color::from_rgba(100, 255, 100, 255));
        }
        // "Laws" and "Politics" tab buttons at the bottom of the panel.
        let (mx, my) = mouse_position();
        let btn = self.laws_tab_rect();
        let hover = btn.contains(vec2(mx, my));
        draw_rectangle(btn.x, btn.y, btn.w, btn.h, Color::from_rgba(60, 50, 30, 230));
        draw_rectangle_lines(btn.x, btn.y, btn.w, btn.h, 2.0, gold());
        label_center("Laws", btn.x + btn.w / 2.0, btn.y + 4.0, 16.0, if hover { WHITE } else { gold() });
        let pbtn = self.politics_tab_rect();
        let phover = pbtn.contains(vec2(mx, my));
        draw_rectangle(pbtn.x, pbtn.y, pbtn.w, pbtn.h, Color::from_rgba(60, 50, 30, 230));
        draw_rectangle_lines(pbtn.x, pbtn.y, pbtn.w, pbtn.h, 2.0, gold());
        label_center("Politics", pbtn.x + pbtn.w / 2.0, pbtn.y + 4.0, 16.0, if phover { WHITE } else { gold() });
    }

    fn laws_tab_rect(&self) -> Rect {
        Rect::new(6.0, 160.0, 76.0, 24.0)
    }

    fn politics_tab_rect(&self) -> Rect {
        Rect::new(86.0, 160.0, 90.0, 24.0)
    }

    /// The laws panel sits just right of the top-left country-info panel.
    fn laws_panel_rect(&self) -> Rect {
        Rect::new(432.0, 6.0, 320.0, 366.0)
    }

    /// Rects of the category buttons (Ethnic / Conscription / Religion).
    fn laws_cat_rects(&self) -> Vec<(usize, Rect)> {
        let panel = self.laws_panel_rect();
        let n = law_categories().len();
        let bw = (panel.w - 20.0 - (n as f32 - 1.0) * 6.0) / n as f32;
        let by = panel.y + 36.0;
        (0..n)
            .map(|i| (i, Rect::new(panel.x + 10.0 + i as f32 * (bw + 6.0), by, bw, 26.0)))
            .collect()
    }

    /// Side panel showing the selected nation's laws (read-only). Two buttons
    /// switch between Ethnic and Conscription laws. The Ethnic view lists every
    /// ethnic group in the game and that nation's legal status for each. The
    /// Conscription view shows the four options with the nation's current one
    /// marked. Nothing here is editable.
    fn draw_laws_panel(&self) {
        if !self.laws_open {
            return;
        }
        let Some(index) = self.selected else { return };
        let Some(city) = self.game.cities.get(index) else { return };
        let nation = if city.sovereign.is_empty() { city.owner.clone() } else { city.sovereign.clone() };
        let chosen = self.game.laws.get(&nation).cloned().unwrap_or_default();
        let panel = self.laws_panel_rect();
        gold_panel(panel.x, panel.y, panel.w, panel.h, 230);
        text_top(&format!("{nation} - Laws"), panel.x + 10.0, panel.y + 8.0, 18.0, gold());
        // Category buttons.
        for (i, r) in self.laws_cat_rects() {
            let on = i == self.laws_category;
            let bg = if on { Color::from_rgba(90, 75, 40, 255) } else { Color::from_rgba(50, 45, 35, 255) };
            draw_rectangle(r.x, r.y, r.w, r.h, bg);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, gold());
            let short = match i {
                0 => "Ethnic",
                1 => "Conscript",
                2 => "Religion",
                _ => "Taxation",
            };
            label_center(short, r.x + r.w / 2.0, r.y + 7.0, 11.0, if on { WHITE } else { gold() });
        }
        let mut y = panel.y + 76.0;
        if self.laws_category == 0 {
            // Ethnic Laws: every ethnic group and its status in this nation.
            text_top("Ethnic group status:", panel.x + 10.0, y, 14.0, Color::from_rgba(255, 225, 150, 255));
            y += 20.0;
            for (key, label) in ALL_ETHNICITIES {
                if y + 20.0 > panel.y + panel.h - 10.0 {
                    break;
                }
                let rank = ethnic_group_status(&nation, label, self.game.year);
                let (status, col) = status_style(rank);
                if let Some(tex) = self.art.ethnicities.get(*key) {
                    stretch(tex, Rect::new(panel.x + 12.0, y, 18.0, 18.0));
                }
                text_top(label, panel.x + 36.0, y + 2.0, 13.0, WHITE);
                text_top(status, panel.x + 170.0, y + 2.0, 13.0, col);
                y += 22.0;
            }
        } else if self.laws_category == 2 {
            // Religion Laws: every religion and its status in this nation.
            text_top("Faith status:", panel.x + 10.0, y, 14.0, Color::from_rgba(255, 225, 150, 255));
            y += 20.0;
            for (key, label) in ALL_RELIGIONS {
                if y + 20.0 > panel.y + panel.h - 10.0 {
                    break;
                }
                let rank = religion_group_status(&nation, label, self.game.year);
                let (status, col) = faith_style(rank);
                if let Some(tex) = self.art.religions.get(*key) {
                    stretch(tex, Rect::new(panel.x + 12.0, y, 18.0, 18.0));
                }
                text_top(label, panel.x + 36.0, y + 2.0, 13.0, WHITE);
                text_top(status, panel.x + 170.0, y + 2.0, 13.0, col);
                y += 22.0;
            }
        } else {
            // Conscription (1) or Taxation (3): four options, current marked.
            let ci = self.laws_category;
            let sel = chosen.get(ci).copied().unwrap_or(0);
            if let Some(cat) = law_categories().get(ci) {
                for (oi, opt) in cat.options.iter().enumerate() {
                    if y + 36.0 > panel.y + panel.h - 10.0 {
                        break;
                    }
                    let active = oi == sel;
                    let marker = if active { "> " } else { "   " };
                    let name_col = if active { Color::from_rgba(140, 255, 140, 255) } else { Color::from_rgba(150, 150, 150, 255) };
                    text_top(&format!("{marker}{}", opt.name), panel.x + 12.0, y, 15.0, name_col);
                    let eff_col = if active { Color::from_rgba(200, 215, 195, 255) } else { Color::from_rgba(110, 110, 110, 255) };
                    text_top(opt.effect, panel.x + 22.0, y + 17.0, 12.0, eff_col);
                    y += 38.0;
                }
            }
        }
        self.art.button(self.laws_close_rect(), "Close", 18.0);
    }

    fn laws_close_rect(&self) -> Rect {
        let panel = self.laws_panel_rect();
        Rect::new(panel.x + panel.w - 86.0, panel.y + 6.0, 76.0, 24.0)
    }

    /// Handles a click inside the open laws panel. Laws are read-only; only the
    /// category buttons and the close button respond.
    fn laws_click(&mut self, mx: f32, my: f32) {
        let p = vec2(mx, my);
        if self.laws_close_rect().contains(p) {
            self.laws_open = false;
            self.game.cue(Cue::Click);
            return;
        }
        for (i, r) in self.laws_cat_rects() {
            if r.contains(p) {
                self.laws_category = i;
                self.game.cue(Cue::Click);
                return;
            }
        }
    }

    /// Leader panel below the top-left faction panel. Shows the ruling
    /// nation's leader portrait framed by `frame.webp`, with their name and
    /// reign dates. Driven by the selected city's sovereign and the game year.
    fn draw_leader_panel(&self) {
        let Some(index) = self.selected else { return };
        let Some(city) = self.game.cities.get(index) else { return };
        // Use the sovereign (the nation that rules), falling back to the owner.
        let nation = if city.sovereign.is_empty() { &city.owner } else { &city.sovereign };
        let Some((name, stem, dates)) = ruler_of(nation, self.game.year) else { return };
        let rect = Rect::new(6.0, 190.0, 430.0, 182.0);
        gold_panel(rect.x, rect.y, rect.w, rect.h, 190);
        // Portrait on the left, with the ornate frame drawn on top.
        let px = rect.x + 15.0;
        let py = rect.y + 14.0;
        let pw = 150.0;
        let ph = 118.0;
        // Puppets / colonial governments have no sovereign painting: the
        // "governor" stem draws the generic governor sprite over the portrait
        // background, matching the governor panel's layered look.
        if stem == "governor" {
            if let Some(bg) = &self.art.portrait_bg {
                stretch(bg, Rect::new(px, py, pw, ph));
            } else {
                draw_rectangle(px, py, pw, ph, Color::from_rgba(40, 32, 24, 255));
            }
            // One stable governor face per nation (govenor1..4).
            let faces = ["govenor1", "govenor2", "govenor3", "govenor4"];
            let key = faces[name_hash(nation) % faces.len()];
            if let Some(sheet) = self.art.sheet(key) {
                let h = name_hash(nation) as i32;
                let t = self.anim + h % 97;
                let jitter = ((t / 7 + h) % 5) - 2;
                let wobbly = (t / 6 + jitter).max(0);
                sheet.blit("idle", wobbly, 1, px + pw * 0.5, py + ph * 0.5, ph * 0.78);
            }
        } else if let Some(tex) = self.art.rulers.get(stem) {
            stretch(tex, Rect::new(px, py, pw, ph));
        } else {
            draw_rectangle(px, py, pw, ph, Color::from_rgba(30, 25, 20, 255));
        }
        // frame.webp overlaid around the portrait, oversized so its border
        // sits comfortably outside the image edges.
        stretch(&self.art.frame, Rect::new(px - 16.0, py - 16.0, pw + 32.0, ph + 32.0));
        shadow_center(name, px + pw / 2.0, rect.y + ph + 24.0, 15.0, gold());
        shadow_center(dates, px + pw / 2.0, rect.y + ph + 44.0, 12.0, Color::from_rgba(220, 210, 180, 255));
        // Traits / national buffs to the right of the portrait.
        let tx = px + pw + 26.0;
        let mut ty = rect.y + 12.0;
        text_top("-- Traits --", tx, ty, 15.0, gold());
        ty += 20.0;
        for (trait_name, effect) in ruler_traits(nation, stem) {
            if ty + 30.0 > rect.y + rect.h - 6.0 {
                break;
            }
            text_top(trait_name, tx, ty, 14.0, Color::from_rgba(255, 225, 150, 255));
            ty += 15.0;
            text_top(effect, tx + 8.0, ty, 12.0, Color::from_rgba(190, 220, 190, 255));
            ty += 19.0;
        }
    }

    /// Rect for the Politics side panel (same slot as the Laws panel).
    fn politics_panel_rect(&self) -> Rect {
        Rect::new(432.0, 6.0, 320.0, 300.0)
    }

    fn politics_close_rect(&self) -> Rect {
        let panel = self.politics_panel_rect();
        Rect::new(panel.x + panel.w - 86.0, panel.y + 6.0, 76.0, 24.0)
    }

    /// Politics side panel (toggled by its tab). Shows the government type and
    /// icon, then either the national party pie chart with a legend and the
    /// ruling party, or — for authoritarian governments — "No elections".
    fn draw_politics_panel(&self) {
        if !self.politics_open {
            return;
        }
        let Some(index) = self.selected else { return };
        let Some(city) = self.game.cities.get(index) else { return };
        let nation = if city.sovereign.is_empty() { city.owner.clone() } else { city.sovereign.clone() };
        let (gov_key, gov_name) = government_of(&nation, self.game.year);
        let panel = self.politics_panel_rect();
        gold_panel(panel.x, panel.y, panel.w, panel.h, 230);
        text_top(&format!("{nation} - Politics"), panel.x + 10.0, panel.y + 8.0, 16.0, gold());
        // Government type: icon + name.
        if let Some(tex) = self.art.governments.get(gov_key) {
            stretch(tex, Rect::new(panel.x + 12.0, panel.y + 36.0, 40.0, 40.0));
        }
        text_top("Government", panel.x + 60.0, panel.y + 40.0, 13.0, Color::from_rgba(200, 200, 200, 255));
        text_top(gov_name, panel.x + 60.0, panel.y + 58.0, 15.0, Color::from_rgba(230, 220, 180, 255));

        if !is_elective(gov_key) {
            label_center("No elections", panel.x + panel.w / 2.0, panel.y + 150.0, 20.0, Color::from_rgba(220, 160, 90, 255));
            label_center("Power is held without a vote.", panel.x + panel.w / 2.0, panel.y + 180.0, 13.0, Color::from_rgba(170, 170, 170, 255));
            self.art.button(self.politics_close_rect(), "Close", 18.0);
            return;
        }

        let pie = self.game.nation_party_pie(&nation);
        let parties = parties_of(&nation);
        let colors = [
            Color::from_rgba(90, 150, 230, 255),
            Color::from_rgba(220, 90, 80, 255),
            Color::from_rgba(90, 200, 130, 255),
            Color::from_rgba(220, 190, 90, 255),
        ];
        // Pie chart.
        let cx = panel.x + 70.0;
        let cy = panel.y + 150.0;
        let r = 54.0;
        let mut a0 = -std::f32::consts::FRAC_PI_2;
        for p in 0..4 {
            let sweep = pie[p] * std::f32::consts::TAU;
            let steps = (sweep / 0.1).ceil().max(1.0) as i32;
            for s in 0..steps {
                let t0 = a0 + sweep * (s as f32 / steps as f32);
                let t1 = a0 + sweep * ((s + 1) as f32 / steps as f32);
                draw_triangle(
                    vec2(cx, cy),
                    vec2(cx + t0.cos() * r, cy + t0.sin() * r),
                    vec2(cx + t1.cos() * r, cy + t1.sin() * r),
                    colors[p],
                );
            }
            a0 += sweep;
        }
        // Legend.
        let ruling = self.game.ruling_party.get(&nation).copied();
        let mut ly = panel.y + 100.0;
        for p in 0..4 {
            draw_rectangle(panel.x + 150.0, ly, 12.0, 12.0, colors[p]);
            let pct = (pie[p] * 100.0).round() as i32;
            let marker = if ruling == Some(p) { " *" } else { "" };
            let col = if ruling == Some(p) { WHITE } else { Color::from_rgba(200, 200, 200, 255) };
            text_top(&format!("{} {pct}%{marker}", parties[p]), panel.x + 168.0, ly, 12.0, col);
            ly += 22.0;
        }
        if let Some(rp) = ruling {
            text_top(&format!("In power: {}", parties[rp]), panel.x + 12.0, panel.y + panel.h - 24.0, 13.0, Color::from_rgba(140, 255, 140, 255));
        }
        self.art.button(self.politics_close_rect(), "Close", 18.0);
    }

    fn draw_glow(&self, owner: &str) {
        let c = faction_color(owner);
        for city in &self.game.cities {
            if city.owner != owner {
                continue;
            }
            let (sx, sy) = self.world_to_screen(city.x, city.y);
            if !on_screen(sx, sy, 40.0) {
                continue;
            }
            for r in [25.0_f32, 23.0, 21.0, 19.0, 17.0] {
                let alpha = (100.0_f32 - (r - 15.0) * 8.0).max(10.0) as u8;
                draw_circle_lines(sx, sy, r, 2.0, Color::from_rgba(c[0], c[1], c[2], alpha));
            }
        }
    }

    fn draw_news(&self) {
        let rect = self.news_panel_rect();
        gold_panel(rect.x, rect.y, rect.w, rect.h, 170);
        outlined("NEWS:", rect.x + 6.0, rect.y + 4.0, 18.0, gold());
        let btn = self.news_toggle_rect();
        draw_rectangle(btn.x, btn.y, btn.w, btn.h, Color::from_rgba(60, 60, 60, 255));
        if self.news_open {
            draw_triangle(vec2(btn.x + 4.0, btn.y + 6.0), vec2(btn.x + 14.0, btn.y + 6.0), vec2(btn.x + 9.0, btn.y + 13.0), gold());
        } else {
            draw_triangle(vec2(btn.x + 4.0, btn.y + 12.0), vec2(btn.x + 14.0, btn.y + 12.0), vec2(btn.x + 9.0, btn.y + 5.0), gold());
        }
        if !self.news_open {
            return;
        }
        let total = self.game.news.len() as i32;
        let max_scroll = (total - 5).max(0);
        let scroll = self.news_scroll.clamp(0, max_scroll);
        let window: Vec<&String> = self.game.news.iter().skip(scroll as usize).take(5).collect();
        for (i, msg) in window.iter().rev().enumerate() {
            outlined(msg, rect.x + 6.0, rect.y + 24.0 + i as f32 * 20.0, 16.0, Color::from_rgba(220, 220, 220, 255));
        }
        if max_scroll > 0 {
            text_top(&format!("scroll ({scroll}/{max_scroll})"), rect.x + 6.0, rect.y + rect.h - 16.0, 13.0, Color::from_rgba(160, 160, 160, 255));
        }
    }

    fn news_panel_rect(&self) -> Rect {
        let panel_w = 420.0;
        let header = 24.0;
        let h = if self.news_open { header + 5.0 * 20.0 + 10.0 } else { header };
        Rect::new(screen_width() - panel_w - 6.0, 6.0, panel_w, h)
    }

    fn news_toggle_rect(&self) -> Rect {
        let panel = self.news_panel_rect();
        Rect::new(panel.x + panel.w - 22.0, panel.y + 4.0, 18.0, 18.0)
    }

    fn city_panel_rect(&self) -> Rect {
        Rect::new((screen_width() - 700.0) / 2.0, screen_height() - 320.0 - 10.0, 700.0, 320.0)
    }

    /// Panel to the right of the city panel, shown when a settlement is
    /// selected. Lists the city's ethnic make-up as a bar graph: each row is a
    /// portrait, an ethnicity name, and a bar sized to that group's share.
    fn draw_ethnicity_panel(&self) {
        let Some(index) = self.selected else { return };
        let Some(city) = self.game.cities.get(index) else { return };
        if city.ethnicity.is_empty() {
            return;
        }
        let rect = self.ethnicity_panel_rect();
        gold_panel(rect.x, rect.y, rect.w, rect.h, 220);
        let x = rect.x + 10.0;
        let bar_x = x + 110.0;
        let bar_max = rect.w - 150.0;
        let row_h = 22.0;
        let bottom = rect.y + rect.h - 6.0;
        // Bar-graph row helper, reading stored (drifting) demographics by key.
        let draw_rows = |panel: &App, rows: &[(String, f32)], start_y: f32| -> f32 {
            let mut ry = start_y;
            // Largest share first.
            let mut sorted: Vec<&(String, f32)> = rows.iter().collect();
            sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (key, share) in sorted {
                if ry + row_h > bottom {
                    break;
                }
                if let Some(tex) = panel.art.ethnicities.get(key).or_else(|| panel.art.religions.get(key)) {
                    stretch(tex, Rect::new(x, ry, 18.0, 18.0));
                }
                text_top(group_label(key), x + 22.0, ry + 2.0, 12.0, WHITE);
                let pct = (share * 100.0).round() as i32;
                draw_rectangle(bar_x, ry + 4.0, bar_max, 10.0, Color::from_rgba(40, 30, 20, 200));
                draw_rectangle(bar_x, ry + 4.0, bar_max * share.clamp(0.0, 1.0), 10.0, gold());
                text_top(&format!("{pct}%"), bar_x + bar_max + 5.0, ry + 2.0, 12.0, Color::from_rgba(230, 220, 190, 255));
                ry += row_h;
            }
            ry
        };
        text_top("-- Ethnicities --", x, rect.y + 10.0, 15.0, gold());
        let mut ry = draw_rows(self, &city.ethnicity, rect.y + 30.0);
        // Religions below the ethnicities, in the same panel.
        ry += 8.0;
        if ry + 20.0 < bottom {
            text_top("-- Religions --", x, ry, 15.0, gold());
            ry = draw_rows(self, &city.religion, ry + 20.0);
        }
        // Party support below the religions, for elective-government nations.
        let nation = if city.sovereign.is_empty() { &city.owner } else { &city.sovereign };
        let (gov_key, _) = government_of(nation, self.game.year);
        if is_elective(gov_key) {
            if let Some(favor) = self.game.settlement_party_favor_of(city.id) {
                ry += 8.0;
                if ry + 20.0 < bottom {
                    text_top("-- Parties --", x, ry, 15.0, gold());
                    let mut py = ry + 20.0;
                    let parties = parties_of(nation);
                    let colors = [
                        Color::from_rgba(90, 150, 230, 255),
                        Color::from_rgba(220, 90, 80, 255),
                        Color::from_rgba(90, 200, 130, 255),
                        Color::from_rgba(220, 190, 90, 255),
                    ];
                    for p in 0..4 {
                        if py + row_h > bottom {
                            break;
                        }
                        draw_rectangle(x, py + 2.0, 14.0, 14.0, colors[p]);
                        text_top(parties[p], x + 22.0, py + 2.0, 12.0, WHITE);
                        let share = favor[p];
                        let pct = (share * 100.0).round() as i32;
                        draw_rectangle(bar_x, py + 4.0, bar_max, 10.0, Color::from_rgba(40, 30, 20, 200));
                        draw_rectangle(bar_x, py + 4.0, bar_max * share.clamp(0.0, 1.0), 10.0, colors[p]);
                        text_top(&format!("{pct}%"), bar_x + bar_max + 5.0, py + 2.0, 12.0, Color::from_rgba(230, 220, 190, 255));
                        py += row_h;
                    }
                }
            }
        }
    }

    /// Rectangle for the ethnicity panel, placed just right of the city panel.
    /// Same height as the city panel so it sits flush in the bottom row with
    /// the city and governor tabs; ethnicities, religions, and party support
    /// fill the available height and clip in priority order if they overflow.
    fn ethnicity_panel_rect(&self) -> Rect {
        let city = self.city_panel_rect();
        Rect::new(city.x + city.w + 10.0, city.y, 300.0, city.h)
    }

    fn date_layout(&self) -> DateLayout {
        let date = self.game.date_label();
        let season = self.game.season().to_string();
        let date_w = measure_text(&date, None, 22, 1.0).width;
        let season_w = measure_text(&season, None, 16, 1.0).width;
        let frame_w = date_w.max(season_w) + 220.0;
        let frame_h = 42.0;
        let frame_x = (screen_width() - frame_w) / 2.0;
        let frame_y = 4.0;
        let btn_y = frame_y + 11.0;
        let btn_x = frame_x + frame_w - 180.0;
        let speeds = [0, 1, 2, 3, 6];
        let mut buttons = Vec::new();
        buttons.push((Rect::new(btn_x, btn_y, 28.0, 20.0), 0));
        for i in 1..5 {
            let bx = btn_x + 32.0 + (i - 1) as f32 * 36.0;
            buttons.push((Rect::new(bx, btn_y, 34.0, 20.0), speeds[i]));
        }
        let (mx, my) = mouse_position();
        let world_x = (mx - self.cam_x) / self.zoom.max(0.0001);
        let world_y = (my - self.cam_y) / self.zoom.max(0.0001);
        let on_map = world_x >= 0.0 && world_y >= 0.0 && world_x < MAP_W as f32 && world_y < MAP_H as f32;
        let coord_text = if on_map {
            format!("X: {:.0}   Y: {:.0}", world_x / COORD_SCALE, world_y / COORD_SCALE)
        } else {
            "X: --   Y: --".to_string()
        };
        let cf_w = (measure_text(&coord_text, None, 16, 1.0).width + 24.0).max(140.0);
        DateLayout {
            frame: Rect::new(frame_x, frame_y, frame_w, frame_h),
            coord: Rect::new((screen_width() - cf_w) / 2.0, frame_y + frame_h + 4.0, cf_w, 22.0),
            buttons,
            date,
            season,
            date_w,
            season_w,
            coord_text,
        }
    }

    fn draw_date(&self) {
        let layout = self.date_layout();
        gold_panel(layout.frame.x, layout.frame.y, layout.frame.w, layout.frame.h, 180);
        let date_x = layout.frame.x + (layout.frame.w - layout.date_w) / 2.0 - 80.0;
        outlined(&layout.date, date_x, layout.frame.y + 6.0, 22.0, WHITE);
        let season_color = match layout.season.as_str() {
            "Winter" => Color::from_rgba(180, 220, 255, 255),
            "Spring" => Color::from_rgba(150, 255, 150, 255),
            "Summer" => Color::from_rgba(255, 220, 80, 255),
            _ => Color::from_rgba(255, 160, 50, 255),
        };
        outlined(&layout.season, date_x + (layout.date_w - layout.season_w) / 2.0, layout.frame.y + 24.0, 16.0, season_color);
        for (i, (rect, spd)) in layout.buttons.iter().enumerate() {
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(40, 40, 40, 255));
            let on = self.game.speed == *spd;
            let col = if *spd == 0 {
                if on { Color::from_rgba(255, 100, 100, 255) } else { Color::from_rgba(180, 180, 180, 255) }
            } else if *spd == 6 {
                if on { Color::from_rgba(255, 220, 50, 255) } else { Color::from_rgba(200, 200, 0, 255) }
            } else if on {
                Color::from_rgba(255, 220, 50, 255)
            } else {
                Color::from_rgba(180, 180, 180, 255)
            };
            if *spd == 0 {
                draw_rectangle(rect.x + 8.0, rect.y + 4.0, 4.0, 12.0, col);
                draw_rectangle(rect.x + 16.0, rect.y + 4.0, 4.0, 12.0, col);
            } else {
                let n = if i <= 3 { i } else { 4 };
                for t in 0..n {
                    let bx = rect.x + 4.0 + t as f32 * 7.0;
                    draw_triangle(vec2(bx, rect.y + 4.0), vec2(bx, rect.y + 16.0), vec2(bx + 6.0, rect.y + 10.0), col);
                }
            }
        }
        gold_panel(layout.coord.x, layout.coord.y, layout.coord.w, layout.coord.h, 180);
        let cw = measure_text(&layout.coord_text, None, 16, 1.0).width;
        outlined(&layout.coord_text, layout.coord.x + (layout.coord.w - cw) / 2.0, layout.coord.y + 3.0, 16.0, Color::from_rgba(200, 230, 255, 255));
    }

    fn paint_paper(&self, title: &str, body: &str) -> bool {
        let w = screen_width();
        let h = screen_height();
        draw_rectangle(0.0, 0.0, w, h, Color::from_rgba(0, 0, 0, 150));
        let nw = 800.0;
        let nh = 600.0;
        let px = (w - nw) / 2.0;
        let py = (h - nh) / 2.0;
        stretch(&self.art.news, Rect::new(px, py, nw, nh));
        let x_btn = Rect::new(px + nw - 55.0, py + 15.0, 40.0, 40.0);
        draw_rectangle(x_btn.x, x_btn.y, x_btn.w, x_btn.h, Color::from_rgba(180, 40, 40, 255));
        label_center("X", x_btn.x + 20.0, x_btn.y + 8.0, 28.0, WHITE);
        label_center(title, px + nw / 2.0, py + 160.0, 42.0, Color::from_rgba(30, 30, 30, 255));
        for (i, line) in wrap_px(body, nw - 120.0, 28.0).iter().enumerate() {
            label_center(line, px + nw / 2.0, py + 250.0 + i as f32 * 36.0, 28.0, Color::from_rgba(40, 40, 40, 255));
        }
        left_click(x_btn) || is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space)
    }

    /// Settings overlay: font size and effect volume. Shown over the menu or
    /// the pause screen. Font size stays in the range the fixed panels can
    /// tolerate. Volume runs from mute to full.
    fn draw_settings_overlay(&mut self) {
        let w = screen_width();
        let h = screen_height();
        draw_rectangle(0.0, 0.0, w, h, Color::from_rgba(0, 0, 0, 180));
        let pw = 460.0;
        let ph = 380.0;
        let px = (w - pw) / 2.0;
        let py = (h - ph) / 2.0;
        gold_panel(px, py, pw, ph, 245);
        label_center("Settings", px + pw / 2.0, py + 16.0, 30.0, gold());

        let scale = font_scale();
        label_center(&format!("Font Size: {:.0}%", scale * 100.0), px + pw / 2.0, py + 68.0, 20.0, WHITE);
        let font_track = Rect::new(px + 70.0, py + 112.0, pw - 140.0, 8.0);
        draw_slider(font_track, (scale - FONT_SCALE_MIN) / (FONT_SCALE_MAX - FONT_SCALE_MIN));
        let font_minus = Rect::new(px + 20.0, py + 100.0, 36.0, 32.0);
        let font_plus = Rect::new(px + pw - 56.0, py + 100.0, 36.0, 32.0);
        self.art.button(font_minus, "-", 26.0);
        self.art.button(font_plus, "+", 26.0);

        let volume = sound_volume();
        let volume_label = if volume <= 0.001 {
            "Sound: Muted".to_string()
        } else {
            format!("Sound: {:.0}%", volume * 100.0)
        };
        label_center(&volume_label, px + pw / 2.0, py + 176.0, 20.0, WHITE);
        let sound_track = Rect::new(px + 70.0, py + 220.0, pw - 140.0, 8.0);
        draw_slider(sound_track, (volume - SOUND_VOLUME_MIN) / (SOUND_VOLUME_MAX - SOUND_VOLUME_MIN));
        let sound_minus = Rect::new(px + 20.0, py + 208.0, 36.0, 32.0);
        let sound_plus = Rect::new(px + pw - 56.0, py + 208.0, 36.0, 32.0);
        self.art.button(sound_minus, "-", 26.0);
        self.art.button(sound_plus, "+", 26.0);

        let close = Rect::new(px + pw / 2.0 - 70.0, py + ph - 56.0, 140.0, 40.0);
        self.art.button(close, "Back", 24.0);

        let (mx, my) = mouse_position();
        if slider_drag(font_track, mx, my) {
            let f = ((mx - font_track.x) / font_track.w).clamp(0.0, 1.0);
            set_font_scale(FONT_SCALE_MIN + f * (FONT_SCALE_MAX - FONT_SCALE_MIN));
        } else if slider_drag(sound_track, mx, my) {
            let f = ((mx - sound_track.x) / sound_track.w).clamp(0.0, 1.0);
            set_sound_volume(SOUND_VOLUME_MIN + f * (SOUND_VOLUME_MAX - SOUND_VOLUME_MIN));
        }
        if self.press(font_minus) {
            set_font_scale(scale - 0.05);
        } else if self.press(font_plus) {
            set_font_scale(scale + 0.05);
        } else if self.press(sound_minus) {
            set_sound_volume(volume - 0.05);
        } else if self.press(sound_plus) {
            set_sound_volume(volume + 0.05);
        } else if self.press(close) || is_key_pressed(KeyCode::Escape) {
            self.settings_open = false;
        }
    }

    fn draw_pause(&mut self) {
        let w = screen_width();
        let h = screen_height();
        draw_rectangle(0.0, 0.0, w, h, Color::from_rgba(0, 0, 0, 100));
        label_center("PAUSED", w * 0.5, h / 2.0 - 180.0, 36.0, gold());
        // Five stacked buttons centered on the screen.
        let bx = w / 2.0 - 100.0;
        let resume = Rect::new(bx, h / 2.0 - 130.0, 200.0, 50.0);
        let save = Rect::new(bx, h / 2.0 - 70.0, 200.0, 50.0);
        let settings = Rect::new(bx, h / 2.0 - 10.0, 200.0, 50.0);
        let menu = Rect::new(bx, h / 2.0 + 50.0, 200.0, 50.0);
        let exit = Rect::new(bx, h / 2.0 + 110.0, 200.0, 50.0);
        self.art.button(resume, "Resume", 28.0);
        self.art.button(save, "Save Game", 28.0);
        self.art.button(settings, "Settings", 28.0);
        self.art.button(menu, "Main Menu", 28.0);
        self.art.button(exit, "Exit", 28.0);
        // Show the result of the most recent save for a few seconds.
        if let Some((msg, shown)) = self.save_notice.clone() {
            if get_time() - shown < 4.0 {
                label_center(&msg, w * 0.5, h / 2.0 + 180.0, 20.0, gold());
            } else {
                self.save_notice = None;
            }
        }
        // Settings overlay takes priority over the pause buttons.
        if self.settings_open {
            self.draw_settings_overlay();
            return;
        }
        if self.press(resume) || is_key_pressed(KeyCode::Escape) {
            self.mode = Mode::Play;
        } else if self.press(save) {
            self.save_game();
        } else if self.press(settings) {
            self.settings_open = true;
        } else if self.press(menu) {
            self.game.paper = None;
            self.mode = Mode::Menu;
        } else if self.press(exit) {
            std::process::exit(0);
        }
    }

    /// Writes the current game state (and its countries) to the save slot and
    /// records a short on-screen confirmation. Called from the pause menu.
    fn save_game(&mut self) {
        let path = save_path();
        let msg = match self.game.save(&path) {
            Ok(()) => format!("Game saved to {}", path.display()),
            Err(err) => format!("Save failed: {err}"),
        };
        self.save_notice = Some((msg, get_time()));
    }

    // ---------------------------------------------------------------------
    // Scenario map editor
    // ---------------------------------------------------------------------

    /// Enters the map editor. Starts a fresh blank map (or keeps the existing
    /// one if we're re-entering), resets the camera to fit the whole map, and
    /// renders the initial empty territory overlay.
    fn open_editor(&mut self) {
        if self.editor.is_none() {
            self.editor = Some(Editor::new(self.maps.region_count));
        }
        self.zoom = screen_width() / MAP_W as f32;
        self.cam_x = 0.0;
        self.cam_y = 0.0;
        self.panning = false;
        self.refresh_editor_overlay();
        self.mode = Mode::Editor;
    }

    /// Rebuilds the editor's territory overlay texture from the current
    /// ownership assignments, reusing `Maps::overlay`.
    fn refresh_editor_overlay(&mut self) {
        if self.maps.overlay_w == 0 || self.maps.overlay_h == 0 {
            return;
        }
        let Some(editor) = &self.editor else { return };
        let colors = editor.overlay_colors();
        let pixels = self.maps.overlay(&colors);
        let w = self.maps.overlay_w as u16;
        let h = self.maps.overlay_h as u16;
        match &self.editor_overlay {
            Some(tex) if tex.width() as u16 == w && tex.height() as u16 == h => {
                tex.update(&Image { bytes: pixels, width: w, height: h });
            }
            _ => {
                let tex = Texture2D::from_rgba8(w, h, &pixels);
                tex.set_filter(FilterMode::Nearest);
                self.editor_overlay = Some(tex);
            }
        }
    }

    /// One frame of the map editor: draws the blank territories map with the
    /// ownership overlay, handles panning/zoom and territory painting, and
    /// draws the country palette plus the Save/Load/Clear/Back controls.
    fn editor_screen(&mut self) {
        // --- Draw the map and territory overlay (no cities/units/HUD). ---
        clear_background(Color::from_rgba(10, 10, 30, 255));
        draw_map_layer(&self.map_tex, self.cam_x, self.cam_y, self.zoom);
        if let Some(overlay) = &self.editor_overlay {
            draw_map_layer(overlay, self.cam_x, self.cam_y, self.zoom);
        }

        let w = screen_width();
        let h = screen_height();
        let (mx, my) = mouse_position();

        // When the palette is hidden it collapses to a thin strip holding just
        // the show/hide toggle, so the territories underneath can be reached.
        let hidden = self.editor.as_ref().map(|e| e.panel_hidden).unwrap_or(false);
        let full_w = 220.0;
        let strip_w = 34.0;
        let panel_w = if hidden { strip_w } else { full_w };
        let panel = Rect::new(0.0, 0.0, panel_w, h);

        // Toggle button, always visible, pinned to the top of the panel strip.
        let toggle = Rect::new(panel_w - 30.0, 6.0, 24.0, 24.0);

        // --- Side palette geometry (only meaningful when expanded). ---
        let row_h = 26.0;
        let list_top = 92.0;
        let list_bottom = h - 150.0;
        let visible_h = (list_bottom - list_top).max(row_h);

        // Scroll the palette with the wheel while the cursor is over it.
        if !hidden && panel.contains(vec2(mx, my)) {
            let max_scroll = (EDITOR_NATIONS.len() as f32 * row_h - visible_h).max(0.0);
            if let Some(editor) = &mut self.editor {
                editor.palette_scroll =
                    (editor.palette_scroll - mouse_wheel().1 * 40.0).clamp(0.0, max_scroll);
            }
        }

        // --- Control buttons at the bottom of the panel. ---
        let save = Rect::new(10.0, h - 136.0, full_w - 20.0, 30.0);
        let load = Rect::new(10.0, h - 100.0, full_w - 20.0, 30.0);
        let clear = Rect::new(10.0, h - 64.0, full_w - 20.0, 30.0);
        let back = Rect::new(10.0, h - 28.0, full_w - 20.0, 24.0);

        // --- Handle input (clicks) before drawing the overlays on top. ---
        let scroll = self.editor.as_ref().map(|e| e.palette_scroll).unwrap_or(0.0);
        let selected = self.editor.as_ref().map(|e| e.selected).unwrap_or(0);
        if is_mouse_button_pressed(MouseButton::Left) {
            if left_click(toggle) {
                // Flip the panel's hidden state.
                if let Some(editor) = &mut self.editor {
                    editor.panel_hidden = !editor.panel_hidden;
                }
                self.game.cue(Cue::Click);
            } else if !hidden && panel.contains(vec2(mx, my)) {
                // Click inside the expanded palette: a control button or a row.
                if left_click(save) {
                    self.save_editor_map();
                    self.game.cue(Cue::Click);
                } else if left_click(load) {
                    self.load_editor_map();
                    self.game.cue(Cue::Click);
                } else if left_click(clear) {
                    if let Some(editor) = &mut self.editor {
                        editor.clear();
                    }
                    self.refresh_editor_overlay();
                    self.game.cue(Cue::Click);
                } else if left_click(back) {
                    self.game.cue(Cue::Click);
                    self.mode = Mode::Menu;
                    return;
                } else if my >= list_top && my <= list_bottom {
                    let idx = ((my - list_top + scroll) / row_h) as usize;
                    if idx < EDITOR_NATIONS.len() {
                        if let Some(editor) = &mut self.editor {
                            editor.selected = idx;
                        }
                        self.game.cue(Cue::Click);
                    }
                }
            } else {
                // Click on the map: paint the hovered territory.
                let wx = (mx - self.cam_x) / self.zoom;
                let wy = (my - self.cam_y) / self.zoom;
                let rid = self.maps.region_at(wx, wy);
                if rid >= 0 {
                    if let Some(editor) = &mut self.editor {
                        editor.paint(rid);
                    }
                    self.refresh_editor_overlay();
                }
            }
        }

        // Camera: right/middle-drag to pan, wheel to zoom (over the map only).
        self.editor_camera(panel);

        // Keyboard: H or Tab toggles the palette, Esc leaves the editor.
        if is_key_pressed(KeyCode::H) || is_key_pressed(KeyCode::Tab) {
            if let Some(editor) = &mut self.editor {
                editor.panel_hidden = !editor.panel_hidden;
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            self.mode = Mode::Menu;
            return;
        }

        // --- Draw the panel on top of the map. ---
        gold_panel(panel.x, panel.y, panel.w, panel.h, 220);
        // Toggle glyph: "<" to collapse, ">" to expand.
        draw_rectangle(toggle.x, toggle.y, toggle.w, toggle.h, Color::from_rgba(60, 55, 25, 255));
        draw_rectangle_lines(toggle.x, toggle.y, toggle.w, toggle.h, 1.0, gold());
        label_center(if hidden { ">" } else { "<" }, toggle.x + toggle.w / 2.0, toggle.y + 3.0, 20.0, gold());

        if !hidden {
            label_center("Map Editor", full_w / 2.0 - 14.0, 12.0, 22.0, gold());
            label_center("Click a territory to", full_w / 2.0, 44.0, 14.0, Color::from_rgba(200, 200, 200, 255));
            label_center("assign the country", full_w / 2.0, 62.0, 14.0, Color::from_rgba(200, 200, 200, 255));

            // Clip the country rows to the list area.
            for (i, name) in EDITOR_NATIONS.iter().enumerate() {
                let ry = list_top + i as f32 * row_h - scroll;
                if ry + row_h < list_top || ry > list_bottom {
                    continue;
                }
                let row = Rect::new(6.0, ry, full_w - 12.0, row_h - 3.0);
                let on = i == selected;
                let bg = if on {
                    Color::from_rgba(80, 70, 30, 255)
                } else {
                    Color::from_rgba(30, 30, 40, 180)
                };
                draw_rectangle(row.x, row.y, row.w, row.h, bg);
                if on {
                    draw_rectangle_lines(row.x, row.y, row.w, row.h, 2.0, gold());
                }
                // Color swatch.
                let c = faction_color(name);
                draw_rectangle(row.x + 4.0, row.y + 3.0, 16.0, row.h - 6.0, Color::from_rgba(c[0], c[1], c[2], 255));
                draw_rectangle_lines(row.x + 4.0, row.y + 3.0, 16.0, row.h - 6.0, 1.0, BLACK);
                text_top(name, row.x + 26.0, row.y + 3.0, 15.0, WHITE);
            }
            // Mask any rows that spilled past the list bounds.
            draw_rectangle(0.0, list_top - row_h, full_w, row_h, Color::from_rgba(20, 18, 10, 255));
            draw_rectangle(0.0, list_bottom, full_w, 20.0, Color::from_rgba(20, 18, 10, 255));

            self.art.button(save, "Save Map", 18.0);
            self.art.button(load, "Load Map", 18.0);
            self.art.button(clear, "Clear All", 18.0);
            self.art.button(back, "Back", 16.0);
        }

        // --- Hover readout and selected-country footer on the map side. ---
        let sel_name = EDITOR_NATIONS.get(selected).copied().unwrap_or("");
        let count = self
            .editor
            .as_ref()
            .map(|e| e.owners.iter().filter(|s| **s == Some(selected)).count())
            .unwrap_or(0);
        let footer = if hidden {
            format!("Painting: {sel_name}  ({count} territories)   -   press H or > to show panel")
        } else {
            format!("Painting: {sel_name}  ({count} territories)   -   press H to hide panel")
        };
        label_center(&footer, panel_w + (w - panel_w) / 2.0, 12.0, 20.0, gold());

        // Status notice (save/load result) centered at the bottom of the map.
        if let Some((msg, shown)) = self.editor.as_ref().and_then(|e| e.notice.clone()) {
            if get_time() - shown < 4.0 {
                label_center(&msg, panel_w + (w - panel_w) / 2.0, h - 30.0, 18.0, gold());
            } else if let Some(editor) = &mut self.editor {
                editor.notice = None;
            }
        }
    }

    /// Camera controls for the editor: drag with the right or middle mouse
    /// button to pan, wheel to zoom toward the cursor. Left-drag is reserved
    /// for painting, and panning is ignored while the cursor is over `panel`.
    fn editor_camera(&mut self, panel: Rect) {
        let (mx, my) = mouse_position();
        if is_mouse_button_pressed(MouseButton::Right) || is_mouse_button_pressed(MouseButton::Middle) {
            self.panning = true;
            self.pan_last = vec2(mx, my);
        }
        if is_mouse_button_released(MouseButton::Right) || is_mouse_button_released(MouseButton::Middle) {
            self.panning = false;
        }
        if self.panning
            && (is_mouse_button_down(MouseButton::Right) || is_mouse_button_down(MouseButton::Middle))
        {
            self.cam_x += mx - self.pan_last.x;
            self.cam_y += my - self.pan_last.y;
            self.pan_last = vec2(mx, my);
        }
        let wheel = mouse_wheel().1;
        if wheel.abs() > 0.0 && !panel.contains(vec2(mx, my)) {
            let old = self.zoom;
            let min_z = screen_width() / MAP_W as f32;
            self.zoom = (self.zoom * (1.0 + wheel.signum() * 0.1)).clamp(min_z, min_z * 8.0);
            self.cam_x = mx - (mx - self.cam_x) * self.zoom / old;
            self.cam_y = my - (my - self.cam_y) * self.zoom / old;
        }
        // Arrow/WASD panning, matching the in-game camera.
        if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
            self.cam_x += PAN_SPEED;
        }
        if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
            self.cam_x -= PAN_SPEED;
        }
        if is_key_down(KeyCode::Up) || is_key_down(KeyCode::W) {
            self.cam_y += PAN_SPEED;
        }
        if is_key_down(KeyCode::Down) || is_key_down(KeyCode::S) {
            self.cam_y -= PAN_SPEED;
        }
        let min_x = screen_width() - MAP_W as f32 * self.zoom;
        let min_y = screen_height() - MAP_H as f32 * self.zoom;
        self.cam_x = self.cam_x.round().clamp(min_x.min(0.0), 0.0);
        self.cam_y = self.cam_y.round().clamp(min_y.min(0.0), 0.0);
    }

    /// Saves the current editor map (region -> country) to the scenario-map
    /// slot and records an on-screen status line.
    fn save_editor_map(&mut self) {
        let path = map_path();
        let Some(editor) = &self.editor else { return };
        let def = editor.to_mapdef("Custom Scenario");
        let owned = def.regions.len();
        let msg = match def.save(&path) {
            Ok(()) => format!("Saved {owned} territories to {}", path.display()),
            Err(err) => format!("Save failed: {err}"),
        };
        if let Some(editor) = &mut self.editor {
            editor.notice = Some((msg, get_time()));
        }
    }

    /// Loads the scenario-map slot into the editor, replacing the current
    /// assignments, and refreshes the overlay.
    fn load_editor_map(&mut self) {
        let path = map_path();
        let msg = match MapDef::load(&path) {
            Ok(def) => {
                let applied = self
                    .editor
                    .as_mut()
                    .map(|e| e.apply_mapdef(&def))
                    .unwrap_or(0);
                format!("Loaded {applied} territories")
            }
            Err(err) => format!("Load failed: {err}"),
        };
        self.refresh_editor_overlay();
        if let Some(editor) = &mut self.editor {
            editor.notice = Some((msg, get_time()));
        }
    }

    fn refresh_overlay(&mut self) {
        if self.maps.overlay_w == 0 || self.maps.overlay_h == 0 {
            return;
        }
        // Nation titles depend on which regions each nation holds, so rebuild
        // them whenever the ownership overlay is rebuilt (i.e. on claim change).
        self.rebuild_nation_labels();
        let colors = self.game.region_owners(self.maps.region_count);
        let pixels = self.maps.overlay(&colors);
        let w = self.maps.overlay_w as u16;
        let h = self.maps.overlay_h as u16;
        // Update the existing texture in place instead of allocating a new one
        // each time. Recreating the GPU texture on every ownership change
        // caused a visible full-screen dim flash; an in-place pixel upload to
        // a texture created once does not.
        match &self.overlay {
            Some(tex) if tex.width() as u16 == w && tex.height() as u16 == h => {
                let image = Image {
                    bytes: pixels,
                    width: w,
                    height: h,
                };
                tex.update(&image);
            }
            _ => {
                let tex = Texture2D::from_rgba8(w, h, &pixels);
                tex.set_filter(FilterMode::Nearest);
                self.overlay = Some(tex);
            }
        }
    }

    /// Recomputes the cached nation titles from current territory ownership.
    /// Each title is laid out as a chain of glyphs that follow the nation's
    /// principal axis (its longest spread) and are nudged to sit on the
    /// nation's OWN land, so the word bends around coastlines and borders
    /// instead of spilling over the sea or a neighbor. Called only when claims
    /// change (from `refresh_overlay`), so this is not per-frame work.
    fn rebuild_nation_labels(&mut self) {
        self.nation_labels.clear();
        for name in self.game.active() {
            let regions = self.game.owned_regions(&name);
            if regions.is_empty() {
                continue;
            }
            // A nation's land can be split across separate landmasses (a
            // mainland plus islands, or colonies across an ocean). Group its
            // regions into connected clumps via land-border adjacency and give
            // EACH clump its own title, so the word is never stretched across
            // the sea between two disconnected pieces.
            for clump in self.connected_region_groups(&regions) {
                let pts: Vec<(f32, f32)> = clump
                    .iter()
                    .filter_map(|rid| self.maps.region_centroid_world(*rid))
                    .collect();
                if pts.is_empty() {
                    continue;
                }
                let owned: std::collections::HashSet<i32> = clump.iter().copied().collect();
                if let Some(label) = self.build_label(&name, &pts, &owned) {
                    self.nation_labels.push(label);
                }
            }
        }
    }

    /// Partitions a nation's region ids into connected components, where two
    /// regions are connected if they share a land border (`region_neighbors`).
    /// Each returned group is one contiguous landmass the nation holds, so a
    /// mainland and its offshore islands come back as separate groups and get
    /// their own titles.
    fn connected_region_groups(&self, regions: &[i32]) -> Vec<Vec<i32>> {
        let set: std::collections::HashSet<i32> = regions.iter().copied().collect();
        let mut seen: std::collections::HashSet<i32> = std::collections::HashSet::new();
        let mut groups = Vec::new();
        for &start in regions {
            if seen.contains(&start) {
                continue;
            }
            // Flood fill from `start` through owned neighbors only.
            let mut group = Vec::new();
            let mut stack = vec![start];
            seen.insert(start);
            while let Some(rid) = stack.pop() {
                group.push(rid);
                for &nb in self.maps.region_neighbors(rid) {
                    if set.contains(&nb) && seen.insert(nb) {
                        stack.push(nb);
                    }
                }
            }
            groups.push(group);
        }
        groups
    }

    /// Builds a single nation's bent, on-territory title. `pts` are the world
    /// centroids of its regions and `owned` the set of its region ids (used to
    /// test whether a world point lies on the nation's own land).
    fn build_label(&self, name: &str, pts: &[(f32, f32)], owned: &std::collections::HashSet<i32>) -> Option<NationLabel> {
        let text = name.to_uppercase();
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return None;
        }
        let n = pts.len() as f32;
        let cx = pts.iter().map(|p| p.0).sum::<f32>() / n;
        let cy = pts.iter().map(|p| p.1).sum::<f32>() / n;

        // Principal axis via the 2x2 covariance of the region centroids: the
        // eigenvector of the larger eigenvalue is the direction the territory
        // stretches most, which reads naturally as the title's baseline.
        let (mut vxx, mut vxy, mut vyy) = (0.0f32, 0.0f32, 0.0f32);
        for (px, py) in pts {
            let dx = px - cx;
            let dy = py - cy;
            vxx += dx * dx;
            vxy += dx * dy;
            vyy += dy * dy;
        }
        vxx /= n;
        vxy /= n;
        vyy /= n;
        // Dominant eigenvector direction of [[vxx,vxy],[vxy,vyy]].
        let (mut ax, mut ay) = if vxy.abs() < 1e-6 {
            if vxx >= vyy { (1.0, 0.0) } else { (0.0, 1.0) }
        } else {
            let tr = vxx + vyy;
            let det = vxx * vyy - vxy * vxy;
            let disc = ((tr * tr) / 4.0 - det).max(0.0).sqrt();
            let l1 = tr / 2.0 + disc; // larger eigenvalue
            // Eigenvector for l1: (vxy, l1 - vxx) (unnormalized).
            (vxy, l1 - vxx)
        };
        let alen = (ax * ax + ay * ay).sqrt();
        if alen < 1e-6 {
            ax = 1.0;
            ay = 0.0;
        } else {
            ax /= alen;
            ay /= alen;
        }
        // Keep the baseline reading left-to-right (never upside down).
        if ax < 0.0 {
            ax = -ax;
            ay = -ay;
        }

        // Extent of the territory projected onto the axis (how long the word
        // can be) and perpendicular to it (how thick the band is).
        let mut proj_min = f32::MAX;
        let mut proj_max = f32::MIN;
        // Project the landmass's actual FOOTPRINT (not just region centroids)
        // onto the principal axis, so sizing reflects how much room the land
        // really offers. Using edge cells means a single big region still gets
        // a long axis, while a tiny island gets a correspondingly tiny one.
        let inv_w = self.maps.w as f32 / self.maps.region_w.max(1) as f32;
        let inv_h = self.maps.h as f32 / self.maps.region_h.max(1) as f32;
        for &rid in owned {
            for &(gx, gy) in self.maps.region_edges(rid) {
                let wx = (gx as f32 + 0.5) * inv_w;
                let wy = (gy as f32 + 0.5) * inv_h;
                let t = (wx - cx) * ax + (wy - cy) * ay;
                proj_min = proj_min.min(t);
                proj_max = proj_max.max(t);
            }
        }
        if proj_min > proj_max {
            // No edge cells resolved — fall back to the centroid spread.
            for (px, py) in pts {
                let t = (px - cx) * ax + (py - cy) * ay;
                proj_min = proj_min.min(t);
                proj_max = proj_max.max(t);
            }
        }
        let axis_len = (proj_max - proj_min).max(1.0);

        // Font size (world units): fit the word across ~85% of the land's
        // extent along its long axis. ~0.62 is the average glyph advance /
        // font-size for the default font in caps.
        let usable = axis_len * 0.85;
        let per_glyph = usable / chars.len().max(1) as f32;
        let natural = per_glyph / 0.62;
        // If the biggest the title can be while still fitting on this landmass
        // is below a legible threshold, the land is simply too small for a
        // title — skip it (tiny islands get no label) rather than stamping an
        // oversized name across the water.
        const MIN_WORLD_FONT: f32 = 7.0;
        if natural < MIN_WORLD_FONT {
            return None;
        }
        let world_font = natural.min(150.0);
        let advance = world_font * 0.62;
        let total = advance * chars.len() as f32;

        // Lay glyphs out along the axis, centered on the centroid, then pull
        // each onto the nation's own land so the word bends to stay inside the
        // borders rather than crossing sea or foreign territory.
        let mut glyphs = Vec::with_capacity(chars.len());
        let mut anchors: Vec<(f32, f32)> = Vec::with_capacity(chars.len());
        let start = -total * 0.5 + advance * 0.5;
        for i in 0..chars.len() {
            let t = start + advance * i as f32;
            let ix = cx + ax * t;
            let iy = cy + ay * t;
            // Perpendicular (for nudging onto land) is (-ay, ax).
            let (px, py) = (-ay, ax);
            let (gx, gy) = self.snap_onto_land(ix, iy, px, py, owned, world_font * 2.0);
            anchors.push((gx, gy));
        }

        // Smooth the bent baseline a touch so rotations don't jitter per glyph.
        for i in 0..anchors.len() {
            let prev = anchors[i.saturating_sub(1)];
            let next = anchors[(i + 1).min(anchors.len() - 1)];
            let rot = (next.1 - prev.1).atan2(next.0 - prev.0);
            // Keep rotation sane; the axis-flip above already prevents upside
            // down, but clamp wild per-glyph swings from the land-snapping.
            let rot = rot.clamp(-1.2, 1.2);
            glyphs.push(LabelGlyph { ch: chars[i], x: anchors[i].0, y: anchors[i].1, rot });
        }

        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for g in &glyphs {
            min_x = min_x.min(g.x);
            min_y = min_y.min(g.y);
            max_x = max_x.max(g.x);
            max_y = max_y.max(g.y);
        }
        Some(NationLabel {
            glyphs,
            world_font,
            color: self.game.label_color(name),
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    /// Returns a point near `(x, y)` that lies on one of the nation's own
    /// regions. If `(x, y)` is already on owned land it is returned unchanged;
    /// otherwise the search walks outward along `±(px, py)` (the perpendicular
    /// to the baseline) looking for the closest owned-land point, so a glyph
    /// that would land in the sea or a neighbor is pulled back inside the
    /// border. Falls back to the original point if nothing owned is found
    /// within `reach` world units.
    fn snap_onto_land(&self, x: f32, y: f32, px: f32, py: f32, owned: &std::collections::HashSet<i32>, reach: f32) -> (f32, f32) {
        if owned.contains(&self.maps.region_at(x, y)) {
            return (x, y);
        }
        let step = 6.0f32;
        let mut d = step;
        while d <= reach {
            for sign in [1.0f32, -1.0] {
                let nx = x + px * d * sign;
                let ny = y + py * d * sign;
                if owned.contains(&self.maps.region_at(nx, ny)) {
                    return (nx, ny);
                }
            }
            d += step;
        }
        (x, y)
    }

    /// Draws the big, faded, grand-strategy style country titles. Each title is
    /// a chain of individually rotated glyphs (precomputed in world space by
    /// `rebuild_nation_labels`) so the word follows the nation's land and bends
    /// around its coasts and borders. Drawn semi-transparent so the map shows
    /// through, and sized by `zoom` so it grows and shrinks with the camera.
    fn draw_nation_labels(&self) {
        for label in &self.nation_labels {
            // Off-screen cull on the title's world bounding box.
            let (ax, ay) = self.world_to_screen(label.min_x, label.min_y);
            let (bx, by) = self.world_to_screen(label.max_x, label.max_y);
            let m = 200.0;
            if bx < -m || by < -m || ax > screen_width() + m || ay > screen_height() + m {
                continue;
            }
            let size = (label.world_font * self.zoom).clamp(9.0, 160.0);
            if size < 9.0 {
                continue;
            }
            // Darken the nation color toward black and raise opacity so the
            // title reads clearly over bright terrain while still showing its
            // faction tint. The drop shadow is darker/stronger too for contrast.
            let c = label.color;
            let dark = |v: u8| (v as f32 * 0.55) as u8;
            let fill = Color::from_rgba(dark(c[0]), dark(c[1]), dark(c[2]), 180);
            let shadow = Color::from_rgba(0, 0, 0, 140);
            for g in &label.glyphs {
                let (sx, sy) = self.world_to_screen(g.x, g.y);
                if !on_screen(sx, sy, 120.0) {
                    continue;
                }
                let mut tmp = [0u8; 4];
                let s: &str = g.ch.encode_utf8(&mut tmp);
                // Shadow then fill, each rotated to follow the local baseline.
                draw_text_ex(s, sx + 1.5, sy + 1.5, TextParams { font_size: size as u16, rotation: g.rot, color: shadow, ..Default::default() });
                draw_text_ex(s, sx, sy, TextParams { font_size: size as u16, rotation: g.rot, color: fill, ..Default::default() });
            }
        }
    }

    /// Yellow squares along the selected territory's edge, matching
    /// `_highlight_selected_region` in `index.py`.
    fn draw_region_highlight(&self) {
        let rid = self.selected_region;
        if rid < 0 || self.maps.region_w == 0 || self.maps.region_h == 0 {
            return;
        }
        let pts = self.maps.region_edges(rid);
        if pts.is_empty() {
            return;
        }
        let cell_w = (MAP_W as f32 / self.maps.region_w as f32) * self.zoom;
        let cell_h = (MAP_H as f32 / self.maps.region_h as f32) * self.zoom;
        let sz = (cell_w.max(cell_h) as i32 + 1).max(2);
        let off = sz / 2;
        let stride = if cell_w < 1.0 { (1.0 / cell_w.max(0.0001)) as usize } else { 1 }.max(1);
        let inv_w = MAP_W as f32 / self.maps.region_w as f32;
        let inv_h = MAP_H as f32 / self.maps.region_h as f32;
        let col = Color::from_rgba(255, 255, 0, 255);
        let sw = screen_width() as i32;
        let sh = screen_height() as i32;
        let sz_f = sz as f32;
        let mut i = 0;
        while i < pts.len() {
            let (gx, gy) = pts[i];
            i += stride;
            let wx = (gx as f32 + 0.5) * inv_w;
            let wy = (gy as f32 + 0.5) * inv_h;
            let sx = (wx * self.zoom + self.cam_x) as i32;
            let sy = (wy * self.zoom + self.cam_y) as i32;
            if sx < -sz || sx > sw + sz || sy < -sz || sy > sh + sz {
                continue;
            }
            draw_rectangle((sx - off) as f32, (sy - off) as f32, sz_f, sz_f, col);
        }
    }

    fn region_panel_rect(&self) -> Option<Rect> {
        if self.selected_region < 0 {
            return None;
        }
        let lines = self.region_panel_lines();
        let pad = 10.0;
        let line_h = 20.0;
        let ph = pad * 2.0 + 24.0 + lines as f32 * line_h;
        Some(Rect::new(12.0, screen_height() - ph - 12.0, 300.0, ph))
    }

    fn region_panel_lines(&self) -> usize {
        let rid = self.selected_region;
        let settlements = self.game.cities.iter().filter(|city| city.region == rid).count();
        let extra = usize::from(settlements > 8);
        4 + settlements.min(8) + extra + 1
    }

    fn draw_region_panel(&self) {
        let Some(rect) = self.region_panel_rect() else { return };
        let rid = self.selected_region;
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(20, 20, 35, 225));
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, Color::from_rgba(120, 100, 50, 255));
        let col = self.maps.region_color(rid);
        let swatch = Rect::new(rect.x + 10.0, rect.y + 14.0, 20.0, 16.0);
        draw_rectangle(swatch.x, swatch.y, swatch.w, swatch.h, Color::from_rgba(col[0], col[1], col[2], 255));
        draw_rectangle_lines(swatch.x, swatch.y, swatch.w, swatch.h, 1.0, WHITE);
        let biome = self
            .maps
            .region_centroid_world(rid)
            .map(|(x, y)| biome_name(self.maps.biome_at(x, y)))
            .unwrap_or("-");
        let settlements: Vec<&City> = self.game.cities.iter().filter(|city| city.region == rid).collect();
        let total = settlements.len();
        let pad = 10.0;
        let mut ty = rect.y + pad;
        text_top(&format!("Territory #{rid}"), rect.x + pad + 28.0, ty, 17.0, WHITE);
        ty += 24.0;
        let body = Color::from_rgba(220, 220, 220, 255);
        text_top(&format!("Tile color: rgb({}, {}, {})", col[0], col[1], col[2]), rect.x + pad, ty, 14.0, body);
        ty += 20.0;
        text_top(&format!("Biome: {biome}"), rect.x + pad, ty, 14.0, body);
        ty += 20.0;
        text_top(&format!("Settlements: {total}"), rect.x + pad, ty, 14.0, body);
        ty += 20.0;
        let city_col = Color::from_rgba(180, 210, 255, 255);
        for city in settlements.iter().take(8) {
            text_top(&format!("  - {} ({})", city.name, city.owner), rect.x + pad, ty, 14.0, city_col);
            ty += 20.0;
        }
        if total > 8 {
            text_top(
                &format!("  ...and {} more", total - 8),
                rect.x + pad,
                ty,
                14.0,
                Color::from_rgba(170, 170, 170, 255),
            );
            ty += 20.0;
        }
        text_top("(click empty tile to close)", rect.x + pad, ty, 14.0, Color::from_rgba(150, 150, 150, 255));
    }
}

struct DateLayout {
    frame: Rect,
    coord: Rect,
    buttons: Vec<(Rect, i32)>,
    date: String,
    season: String,
    date_w: f32,
    season_w: f32,
    coord_text: String,
}

fn images_dir() -> PathBuf {
    #[cfg(feature = "bundle")]
    {
        bundled_images()
    }
    #[cfg(not(feature = "bundle"))]
    {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("images")
    }
}

/// Directory where game saves are written. Kept next to the crate (a `saves/`
/// folder) for the normal build; for a bundled executable the images are
/// unpacked to a temp dir, so saves go beside that so they persist with the
/// unpacked data.
fn save_dir() -> PathBuf {
    #[cfg(feature = "bundle")]
    {
        images_dir().parent().map(|p| p.join("saves")).unwrap_or_else(|| PathBuf::from("saves"))
    }
    #[cfg(not(feature = "bundle"))]
    {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("saves")
    }
}

/// Path of the single quick-save slot.
fn save_path() -> PathBuf {
    save_dir().join("savegame.json")
}

/// Directory where scenario maps authored in the map editor are written,
/// alongside the game saves.
fn map_dir() -> PathBuf {
    save_dir().join("maps")
}

/// Path of the single scenario-map slot used by the editor.
fn map_path() -> PathBuf {
    map_dir().join("scenario_map.json")
}

/// Unpack the images stored in the executable. Later launches reuse the copy
/// when the packed archive is unchanged.
#[cfg(feature = "bundle")]
fn bundled_images() -> PathBuf {
    let data: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/images.zip"));
    let root = std::env::temp_dir().join(format!("colonial_simulator-{}", env!("CARGO_PKG_VERSION")));
    let images = root.join("images");
    let stamp = root.join("stamp");
    let expected = data.len().to_string();
    if stamp.is_file()
        && std::fs::read_to_string(&stamp).ok().as_deref() == Some(expected.as_str())
        && images.join("nasasatelliteview.jpg").is_file()
    {
        return images;
    }
    if root.exists() {
        let _ = std::fs::remove_dir_all(&root);
    }
    std::fs::create_dir_all(&images).unwrap_or_else(|err| panic!("could not create {}: {err}", images.display()));
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
        .unwrap_or_else(|err| panic!("bundled images are unreadable: {err}"));
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap_or_else(|err| panic!("bundled images are unreadable: {err}"));
        let Some(rel) = file.enclosed_name().map(|path| path.to_owned()) else {
            continue;
        };
        let path = images.join(&rel);
        if file.is_dir() {
            std::fs::create_dir_all(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|err| panic!("{}: {err}", parent.display()));
        }
        let mut out = std::fs::File::create(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        std::io::copy(&mut file, &mut out).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    }
    std::fs::write(&stamp, &expected).unwrap_or_else(|err| panic!("could not write {}: {err}", stamp.display()));
    images
}

#[cfg(windows)]
fn install_panic_dialog() {
    std::panic::set_hook(Box::new(|info| {
        let msg = info.to_string();
        eprintln!("{msg}");
        message_box(&msg);
    }));
}

#[cfg(windows)]
fn message_box(text: &str) {
    let text = std::ffi::CString::new(text).unwrap_or_else(|_| std::ffi::CString::new("Colonial Simulator failed").unwrap());
    let caption = std::ffi::CString::new("Colonial Simulator").unwrap();
    unsafe {
        MessageBoxA(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
    }
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn MessageBoxA(
        hwnd: *mut std::ffi::c_void,
        text: *const std::ffi::c_char,
        caption: *const std::ffi::c_char,
        utype: u32,
    ) -> i32;
}

fn gold() -> Color {
    Color::from_rgba(255, 220, 100, 255)
}

fn stretch(tex: &Texture2D, rect: Rect) {
    draw_texture_ex(
        tex,
        rect.x,
        rect.y,
        WHITE,
        DrawTextureParams { dest_size: Some(vec2(rect.w, rect.h)), ..Default::default() },
    );
}

/// Satellite photo with a mip chain.
///
/// The map is minified to fit the window and magnified when zoomed. Without
/// mipmaps the GPU picks one texel per screen pixel from the full photo, and
/// that choice flips as the camera moves, so coastlines and cities blink.
/// `Texture2D::set_filter` turns mipmaps back off, so the sampler is set here.
fn satellite_texture(pixels: &[u8], w: u32, h: u32) -> Texture2D {
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

/// Draws the on-screen part of a native-map texture.
///
/// The old draw stretched the whole map into one quad several times larger
/// than the window. Some Windows drivers interpolate that quad poorly and the
/// photo flashes while panning or zooming, with the cleared background showing
/// through a seam. This quad is only the visible window, in whole pixels.
fn draw_map_layer(tex: &Texture2D, cam_x: f32, cam_y: f32, zoom: f32) {
    let sw = screen_width();
    let sh = screen_height();
    let map_w = MAP_W as f32;
    let map_h = MAP_H as f32;
    let z = zoom.max(0.0001);
    let x0 = cam_x.max(0.0).floor();
    let y0 = cam_y.max(0.0).floor();
    let x1 = (cam_x + map_w * z).min(sw).ceil();
    let y1 = (cam_y + map_h * z).min(sh).ceil();
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let u0 = ((x0 - cam_x) / z).clamp(0.0, map_w);
    let v0 = ((y0 - cam_y) / z).clamp(0.0, map_h);
    let u1 = ((x1 - cam_x) / z).clamp(0.0, map_w);
    let v1 = ((y1 - cam_y) / z).clamp(0.0, map_h);
    if u1 <= u0 || v1 <= v0 {
        return;
    }
    let tw = tex.width();
    let th = tex.height();
    let source = Rect::new(u0 / map_w * tw, v0 / map_h * th, (u1 - u0) / map_w * tw, (v1 - v0) / map_h * th);
    draw_texture_ex(
        tex,
        x0,
        y0,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(x1 - x0, y1 - y0)),
            source: Some(source),
            ..Default::default()
        },
    );
}

fn draw_slider(track: Rect, frac: f32) {
    let frac = frac.clamp(0.0, 1.0);
    draw_rectangle(track.x, track.y, track.w, track.h, Color::from_rgba(60, 50, 35, 255));
    draw_rectangle(track.x, track.y, track.w * frac, track.h, gold());
    draw_circle(track.x + track.w * frac, track.y + track.h / 2.0, 10.0, gold());
}

fn slider_drag(track: Rect, mx: f32, my: f32) -> bool {
    is_mouse_button_down(MouseButton::Left)
        && mx >= track.x - 12.0
        && mx <= track.x + track.w + 12.0
        && (my - (track.y + track.h / 2.0)).abs() < 22.0
}

fn blit_tint(tex: &Texture2D, x: f32, y: f32, w: f32, h: f32, color: Color) {
    draw_texture_ex(tex, x, y, color, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
}

fn label_center(text: &str, cx: f32, top: f32, size: f32, color: Color) {
    let size = scaled_font(size);
    let dims = measure_text(text, None, size as u16, 1.0);
    draw_text(text, cx - dims.width * 0.5, top + dims.offset_y, size, color);
}

fn shadow_center(text: &str, cx: f32, top: f32, size: f32, color: Color) {
    label_center(text, cx + 1.0, top + 1.0, size, BLACK);
    label_center(text, cx, top, size, color);
}

fn text_top(text: &str, x: f32, top: f32, size: f32, color: Color) {
    let size = scaled_font(size);
    let dims = measure_text(text, None, size as u16, 1.0);
    draw_text(text, x, top + dims.offset_y, size, color);
}

fn on_screen(x: f32, y: f32, margin: f32) -> bool {
    x >= -margin && y >= -margin && x <= screen_width() + margin && y <= screen_height() + margin
}

fn left_click(rect: Rect) -> bool {
    if !is_mouse_button_pressed(MouseButton::Left) {
        return false;
    }
    let (x, y) = mouse_position();
    rect.contains(vec2(x, y))
}

fn tooltip(lines: &[String], mx: f32, my: f32) {
    if lines.is_empty() {
        return;
    }
    let mut panel_w: f32 = 40.0;
    for line in lines {
        panel_w = panel_w.max(measure_text(line, None, 16, 1.0).width);
    }
    panel_w += 12.0;
    let panel_h = lines.len() as f32 * 16.0 + 12.0;
    let mut tx = mx + 15.0;
    let mut ty = my + 10.0;
    if tx + panel_w > screen_width() {
        tx = mx - panel_w - 10.0;
    }
    if ty + panel_h > screen_height() {
        ty = my - panel_h - 10.0;
    }
    draw_rectangle(tx, ty, panel_w, panel_h, Color::from_rgba(20, 20, 20, 200));
    for (i, line) in lines.iter().enumerate() {
        let color = if i == 0 { gold() } else { WHITE };
        text_top(line, tx + 6.0, ty + 6.0 + i as f32 * 16.0, 16.0, color);
    }
}

fn wrap_px(text: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let test = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if measure_text(&test, None, size as u16, 1.0).width > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = test;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn mountain_spots(maps: &Maps) -> Vec<(f32, f32)> {
    let mut rng = ::rand::thread_rng();
    let step = 6.0 * COORD_SCALE;
    let half = step * 0.5;
    let mut pts = Vec::new();
    let mut y = half;
    while y < maps.h as f32 {
        let mut x = half;
        while x < maps.w as f32 {
            if biome_name(maps.biome_at(x, y)) == "mountains" && !maps.is_water(x, y) {
                let jx = (x + rng.gen_range(-half * 0.5..half * 0.5)).clamp(0.0, maps.w as f32 - 1.0);
                let jy = (y + rng.gen_range(-half * 0.5..half * 0.5)).clamp(0.0, maps.h as f32 - 1.0);
                pts.push((jx, jy));
            }
            x += step;
        }
        y += step;
    }
    pts
}

fn name_hash(name: &str) -> usize {
    let mut hash: u32 = 2166136261;
    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash as usize
}

fn titled(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Rough historical ethnic make-up of a settlement at a given year. Returns
/// (image key, label, share) rows, largest first, summing to about 1.0. The
/// mix is driven by the colonial power that holds the city, its region, and
/// the date, with overrides for places whose history differs sharply from
/// their ruler (sugar colonies with enslaved African majorities, Spanish
/// America's mestizo and indigenous majorities, Arctic outposts).
fn ethnic_mix(city: &City, year: i32) -> Vec<(&'static str, &'static str, f32)> {
    // Native settlements. Isolated interior tribes are essentially entirely
    // indigenous; only peoples in regular contact with European traders carry
    // a small foreign minority, and that minority matches the nearby colonial
    // power rather than being French everywhere. European contact grew over
    // time, so the trader share rises a little with the date.
    if is_native(&city.owner) || is_native(&city.sovereign) {
        let contact = ((year - 1754).max(0) as f32 / 104.0) * 0.04; // up to +4pts by 1858
        return match city.owner.as_str() {
            // Eastern woodlands tribes trading with New France, then the British.
            "Iroquois" | "Wabanaki" | "Cherokee" => vec![
                ("indigenous", "Indigenous", 0.94 - contact),
                ("french", "French", 0.04),
                ("british", "British", 0.02 + contact),
            ],
            // Cree in the Hudson Bay fur trade: a few French and British traders.
            "Cree" => vec![
                ("indigenous", "Indigenous", 0.96 - contact),
                ("british", "British", 0.03 + contact),
                ("french", "French", 0.01),
            ],
            // Plains and far-interior peoples with little European presence.
            "Comanche" | "Dakota" => vec![("indigenous", "Indigenous", 1.0)],
            _ => vec![("indigenous", "Indigenous", 0.98), ("french", "French", 0.02)],
        };
    }
    let name = city.name.as_str();
    // Caribbean sugar colonies: enslaved Africans were the large majority.
    let caribbean = matches!(
        name,
        "Port-au-Prince" | "Havana" | "Santo Domingo" | "Kingston" | "Nassau" | "Belize Town" | "George Town"
    );
    // Formerly French towns (New France) stayed overwhelmingly French-speaking
    // long after Britain took them in 1763.
    let french_heritage = matches!(name, "Quebec" | "Montreal" | "Tadoussac" | "Fort Detroit" | "New Orleans");
    let mut mix: Vec<(&'static str, &'static str, f32)> = match city.owner.as_str() {
        "Great Britain" => {
            if french_heritage {
                // Conquered New France: a French majority under British rule,
                // with a slowly growing British/anglophone minority.
                let brit = 0.10 + ((year - 1763).max(0) as f32 / 95.0) * 0.25;
                vec![("french", "French", 0.86 - brit), ("british", "British", 0.10 + brit), ("indigenous", "Indigenous", 0.04)]
            } else if caribbean {
                // Enslaved African majority; falls after British abolition (1833).
                let afr = if year >= 1835 { 0.55 } else { 0.80 };
                vec![("africans", "African", afr), ("british", "British", 0.95 - afr - 0.02), ("indigenous", "Indigenous", 0.02)]
            } else if matches!(name, "Charleston" | "Richmond" | "Williamsburg") {
                // Southern plantation ports: large enslaved African population.
                vec![("british", "British", 0.55), ("africans", "African", 0.42), ("indigenous", "Indigenous", 0.03)]
            } else {
                // Northern seaboard: British majority, small African share that
                // shrinks as northern slavery ended and free immigration grew.
                let afr = if year >= 1809 { 0.04 } else { 0.08 };
                vec![("british", "British", 0.92 - afr), ("africans", "African", afr), ("indigenous", "Indigenous", 0.04)]
            }
        }
        "United States" => {
            let southern = matches!(name, "Charleston" | "Richmond" | "Austin" | "New Orleans" | "Mobile");
            if french_heritage && year < 1830 {
                // New Orleans stayed heavily French/Creole early on.
                vec![("french", "French", 0.45), ("africans", "African", 0.35), ("americans", "American", 0.18), ("indigenous", "Indigenous", 0.02)]
            } else if southern {
                // Deep South: enslaved African share stays high through 1858.
                vec![("americans", "American", 0.56), ("africans", "African", 0.42), ("indigenous", "Indigenous", 0.02)]
            } else {
                // Northern cities: African share falls, immigration grows the
                // settler (American) majority through the 1835/1858 waves.
                let afr = if year >= 1835 { 0.06 } else { 0.11 };
                vec![("americans", "American", 0.94 - afr), ("africans", "African", afr), ("indigenous", "Indigenous", 0.06)]
            }
        }
        "France" => {
            if caribbean {
                // Pre-1804 Saint-Domingue: an enormous enslaved majority.
                vec![("africans", "African", 0.88), ("french", "French", 0.10), ("indigenous", "Indigenous", 0.02)]
            } else {
                // New France: French majority with a notable Métis/indigenous share.
                vec![("french", "French", 0.82), ("indigenous", "Indigenous", 0.14), ("africans", "African", 0.04)]
            }
        }
        // French Louisiana: New Orleans held a large enslaved African and Creole
        // population; the upper Mississippi (Saint-Louis) was a French fur-trade
        // frontier with a heavy Métis/indigenous presence.
        "Louisiana" => {
            if name == "New Orleans" {
                vec![("french", "French", 0.45), ("africans", "African", 0.40), ("indigenous", "Indigenous", 0.15)]
            } else {
                vec![("french", "French", 0.55), ("indigenous", "Indigenous", 0.35), ("africans", "African", 0.10)]
            }
        }
        // Saint-Domingue: the richest sugar colony in the world, worked by an
        // enormous enslaved African majority under a small French planter elite.
        "Saint-Domingue" => vec![("africans", "African", 0.88), ("french", "French", 0.10), ("indigenous", "Indigenous", 0.02)],
        "Spain" => {
            if caribbean {
                // Spanish Caribbean: African share rose with the sugar boom.
                let afr = if year >= 1809 { 0.45 } else { 0.35 };
                vec![("latinamericans", "Latin American", 0.85 - afr - 0.15), ("africans", "African", afr), ("spanish", "Spanish", 0.12), ("indigenous", "Indigenous", 0.03)]
            } else {
                // Spanish America: mestizo majority slowly displacing the
                // indigenous base as mixing continued through the period.
                let indig = if year >= 1809 { 0.30 } else { 0.36 };
                vec![("latinamericans", "Latin American", 0.86 - indig - 0.05), ("indigenous", "Indigenous", indig), ("spanish", "Spanish", 0.09), ("africans", "African", 0.05)]
            }
        }
        // Dutch Suriname: a brutal sugar-plantation colony with an enslaved
        // African majority, a small Dutch planter elite, and surviving
        // indigenous (and later Asian) populations.
        "Netherlands" => {
            let afr = if year >= 1800 { 0.68 } else { 0.60 };
            vec![("africans", "African", afr), ("dutch", "Dutch", 0.12), ("indigenous", "Indigenous", 1.0 - afr - 0.12)]
        }
        // Peru: an Andean viceroyalty with a very large indigenous highland
        // majority under a Spanish colonial elite and a growing mestizo share.
        "Peru" => {
            let indig = if year >= 1809 { 0.52 } else { 0.58 };
            vec![("indigenous", "Indigenous", indig), ("latinamericans", "Latin American", 0.92 - indig - 0.08), ("spanish", "Spanish", 0.08), ("africans", "African", 0.08)]
        }
        // New Granada: Spanish-American mestizo majority over an indigenous
        // base, with a notable African coastal population and Spanish elite.
        "New Granada" => {
            let indig = if year >= 1809 { 0.26 } else { 0.32 };
            vec![("latinamericans", "Latin American", 0.84 - indig), ("indigenous", "Indigenous", indig), ("africans", "African", 0.08), ("spanish", "Spanish", 0.08)]
        }
        // Rio de la Plata: a more European and mestizo viceroyalty than the
        // Andean ones, with a smaller indigenous share, a notable African
        // (porteno) minority, and a Spanish colonial elite.
        "Rio de la Plata" => {
            let indig = if year >= 1809 { 0.14 } else { 0.18 };
            vec![("latinamericans", "Latin American", 0.90 - indig), ("indigenous", "Indigenous", indig), ("spanish", "Spanish", 0.08), ("africans", "African", 0.02)]
        }
        // New Spain: the great Mexican/Mesoamerican viceroyalty, with a large
        // indigenous base, a growing mestizo (Latin American) share, a Spanish
        // colonial elite, and a small African coastal minority.
        "New Spain" => {
            let indig = if year >= 1809 { 0.40 } else { 0.46 };
            vec![("indigenous", "Indigenous", indig), ("latinamericans", "Latin American", 0.90 - indig - 0.06), ("spanish", "Spanish", 0.10), ("africans", "African", 0.06)]
        }
        // Portuguese Brazil: a large enslaved African majority on the sugar and
        // later coffee economy, a Portuguese colonial elite, and a surviving
        // indigenous share. The African share grew with the slave trade.
        "Portugal" => {
            let afr = if year >= 1800 { 0.50 } else { 0.44 };
            vec![
                ("africans", "African", afr),
                ("portugese", "Portuguese", 0.30),
                ("indigenous", "Indigenous", 1.0 - afr - 0.30),
            ]
        }
        // Independent Mexico: Spanish-born minority shrank after independence.
        "Mexico" => vec![
            ("latinamericans", "Latin American", 0.60),
            ("indigenous", "Indigenous", 0.34),
            ("spanish", "Spanish", if year >= 1858 { 0.02 } else { 0.04 }),
            ("africans", "African", 0.03),
        ],
        // Haiti: an overwhelming African-descended majority after the revolution.
        "Haiti" => vec![("africans", "African", 0.95), ("latinamericans", "Latin American", 0.05)],
        // Texas: Anglo-American settlers overtook Tejanos through the 1840s-50s.
        "Texas" => {
            let anglo = if year >= 1845 { 0.62 } else { 0.52 };
            vec![("americans", "American", anglo), ("latinamericans", "Latin American", 0.78 - anglo), ("africans", "African", 0.17), ("indigenous", "Indigenous", 0.03)]
        }
        "Russia" => vec![("russians", "Russian", 0.55), ("indigenous", "Indigenous", 0.45)],
        "Denmark" => vec![("danes", "Scandinavian", 0.45), ("indigenous", "Indigenous", 0.55)],
        "Confederate States" => {
            vec![("americans", "American", 0.60), ("africans", "African", 0.38), ("indigenous", "Indigenous", 0.02)]
        }
        "Canada" => {
            if french_heritage {
                vec![("french", "French", 0.78), ("british", "British", 0.18), ("indigenous", "Indigenous", 0.04)]
            } else {
                vec![("british", "British", 0.72), ("french", "French", 0.24), ("indigenous", "Indigenous", 0.04)]
            }
        }
        "Rupert's Land" => vec![("british", "British", 0.40), ("indigenous", "Indigenous", 0.60)],
        _ => vec![("british", "British", 0.70), ("indigenous", "Indigenous", 0.30)],
    };
    // Give each city its own flavor instead of identical numbers: nudge the
    // shares by a small stable per-name amount (keeping the mix summing to ~1).
    perturb_mix(&mut mix, name);
    mix
}

/// Applies a small deterministic per-city shift to an ethnic mix so cities of
/// the same owner don't all show identical percentages. The first (majority)
/// group absorbs the opposite of the shifts applied to the others, keeping the
/// total near 1.0. Shares stay within sane bounds.
fn perturb_mix(mix: &mut [(&'static str, &'static str, f32)], name: &str) {
    if mix.len() < 2 {
        return;
    }
    let mut seed = name_hash(name) as u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut total_shift = 0.0f32;
    for row in mix.iter_mut().skip(1) {
        // Up to +/-6 percentage points, scaled so small groups move less.
        let delta = ((next() % 1200) as f32 / 10000.0 - 0.06) * row.2.max(0.05);
        let new = (row.2 + delta).clamp(0.0, 0.95);
        total_shift += new - row.2;
        row.2 = new;
    }
    mix[0].2 = (mix[0].2 - total_shift).max(0.02);
    // Normalize so the displayed shares always sum to exactly 1.0, correcting
    // any small drift in the hand-tuned base figures.
    let sum: f32 = mix.iter().map(|(_, _, s)| *s).sum();
    if sum > 0.0 {
        for row in mix.iter_mut() {
            row.2 /= sum;
        }
    }
}

/// The historical leader of `nation` at `year`, as (display name, portrait
/// file stem, reign dates). Returns None if no portrait is mapped. Covers the
/// scenario start years (1754, 1790, 1809, 1835, 1858); the latest entry whose
/// start year is <= `year` wins.
fn ruler_of(nation: &str, year: i32) -> Option<(&'static str, &'static str, &'static str)> {
    // Each row: (reign start year, name, portrait stem, reign dates label).
    let timeline: &[(i32, &str, &str, &str)] = match nation {
        "Great Britain" | "Canada" => &[
            (1727, "George II", "georgeii", "1727-1760"),
            (1760, "George III", "georgeiii", "1760-1820"),
            (1830, "William IV", "williamiv", "1830-1837"),
            (1837, "Queen Victoria", "victoria", "1837-1901"),
        ],
        "France" => &[
            (1715, "Louis XV", "louisxv", "1715-1774"),
            (1774, "Louis XVI", "louisxvi", "1774-1792"),
            (1804, "Napoleon I", "napoleoni", "1804-1814"),
            (1830, "Louis-Philippe I", "louis-philippei", "1830-1848"),
            (1852, "Napoleon III", "napoleoniii", "1852-1870"),
        ],
        "Spain" => &[
            (1746, "Ferdinand VI", "ferdinandvi", "1746-1759"),
            (1788, "Charles IV", "charlesiv", "1788-1808"),
            (1808, "Ferdinand VII", "ferdinandvii", "1808-1833"),
            (1833, "Isabella II", "isabellaii", "1833-1868"),
        ],
        // Dutch heads of state: Stadtholder William V of the Republic, then
        // the Kingdom of Holland under Louis Bonaparte, then the kings of the
        // Netherlands from 1815.
        "Netherlands" => &[
            (1751, "Stadtholder William V", "williamv", "Stadtholder, 1751-1795"),
            (1806, "King Louis Bonaparte", "louisbonaparte", "King of Holland, 1806-1810"),
            (1815, "King William I", "williami", "King, 1815-1840"),
            (1840, "King William II", "williamii", "King, 1840-1849"),
            (1849, "King William III", "williamiii", "King, 1849-1890"),
        ],
        "Portugal" => &[
            (1750, "Joseph I", "josephi(portugal)", "1750-1777"),
            (1777, "Maria I", "mariai", "1777-1816"),
            (1816, "John VI", "joãovi", "1816-1826"),
            (1826, "Pedro IV", "pedroiv", "1826"),
            (1826, "Maria II", "mariaii", "1826-1853"),
            (1828, "Miguel I", "migueli", "1828-1834"),
            (1834, "Maria II", "mariaii", "1834-1853"),
            (1853, "Pedro V", "pedrov", "1853-1861"),
            (1861, "Luís I", "luísi", "1861-1889"),
            (1889, "Carlos I", "carlosi", "1889-1908"),
        ],
        // Spanish viceroyalties. These were governed by appointed viceroys, so
        // the portrait uses the generic governor art ("governor" stem) rather
        // than a sovereign's painting. Names are the historical viceroys in
        // office at each scenario era.
        "Peru" => &[
            (1745, "Viceroy Manso de Velasco", "governor", "Viceroy, 1745-1761"),
            (1790, "Viceroy Gil de Taboada", "governor", "Viceroy, 1790-1796"),
            (1806, "Viceroy Abascal", "governor", "Viceroy, 1806-1816"),
            (1816, "Viceroy Pezuela", "governor", "Viceroy, 1816-1821"),
            (1821, "Viceroy de la Serna", "governor", "Viceroy, 1821-1824"),
        ],
        "New Granada" => &[
            (1753, "Viceroy Solis Folch", "governor", "Viceroy, 1753-1761"),
            (1789, "Viceroy de Ezpeleta", "governor", "Viceroy, 1789-1797"),
            (1803, "Viceroy Amar y Borbon", "governor", "Viceroy, 1803-1810"),
            (1816, "Viceroy Montalvo", "governor", "Viceroy, 1816-1818"),
            (1818, "Viceroy de Samano", "governor", "Viceroy, 1818-1819"),
        ],
        "Rio de la Plata" => &[
            (1745, "Gov. Jose de Andonaegui", "governor", "Governor, 1745-1756"),
            (1776, "Viceroy de Cevallos", "governor", "Viceroy, 1776-1778"),
            (1778, "Viceroy de Vertiz", "governor", "Viceroy, 1778-1784"),
            (1804, "Viceroy del Pino", "governor", "Viceroy, 1804-1807"),
            (1809, "Viceroy de Cisneros", "governor", "Viceroy, 1809-1810"),
        ],
        "New Spain" => &[
            (1746, "Viceroy de Guemes", "governor", "Viceroy, 1746-1755"),
            (1789, "Viceroy de Revillagigedo", "governor", "Viceroy, 1789-1794"),
            (1803, "Viceroy de Iturrigaray", "governor", "Viceroy, 1803-1808"),
            (1810, "Viceroy Venegas", "governor", "Viceroy, 1810-1813"),
            (1813, "Viceroy Calleja", "governor", "Viceroy, 1813-1816"),
        ],
        // French Louisiana: run by an appointed colonial governor from New
        // Orleans until the 1803 Louisiana Purchase.
        "Louisiana" => &[
            (1743, "Gov. de Vaudreuil", "governor", "Governor, 1743-1753"),
            (1753, "Gov. de Kerlerec", "governor", "Governor, 1753-1763"),
            (1769, "Gov. O'Reilly", "governor", "Governor, 1769-1770"),
            (1791, "Gov. de Carondelet", "governor", "Governor, 1791-1797"),
            (1801, "Gov. de Laussat", "governor", "Prefect, 1801-1803"),
        ],
        // Saint-Domingue: a French sugar colony run by an appointed governor
        // from Cap-Français until the 1804 Haitian Revolution.
        "Saint-Domingue" => &[
            (1746, "Gov. de Larnage", "governor", "Governor, 1746-1748"),
            (1757, "Gov. de Bart", "governor", "Governor, 1757-1761"),
            (1792, "Gov. de Blanchelande", "governor", "Governor, 1792-1793"),
            (1796, "Gov. de Laveaux", "governor", "Governor, 1796-1798"),
            (1802, "Gov. de Rochambeau", "governor", "Governor, 1802-1803"),
        ],
        // Rupert's Land: the Hudson's Bay Company's chartered territory, run by
        // an appointed Company governor, not the British Crown directly.
        "Rupert's Land" => &[
            (1746, "Gov. James Isham", "governor", "HBC Governor, 1740s-50s"),
            (1770, "Gov. Ferdinand Jacobs", "governor", "HBC Governor, 1770s"),
            (1810, "Gov. William Williams", "governor", "HBC Governor, 1810s"),
            (1821, "Gov. George Simpson", "governor", "HBC Governor, 1821-1860"),
        ],
        "Russia" => &[
            (1741, "Empress Elizabeth", "elizabeth", "1741-1762"),
            (1762, "Catherine II", "catherineii", "1762-1796"),
            (1801, "Alexander I", "alexanderi", "1801-1825"),
            (1825, "Nicholas I", "nicholasi", "1825-1855"),
            (1855, "Alexander II", "alexanderii", "1855-1881"),
        ],
        "Denmark" => &[
            (1746, "Frederick V", "frederickv", "1746-1766"),
            (1766, "Christian VII", "christianvii", "1766-1808"),
            (1808, "Frederick VI", "frederickvi", "1808-1839"),
            (1848, "Frederick VII", "frederickvii", "1848-1863"),
        ],
        "United States" => &[
            (1789, "George Washington", "georgewashington", "1789-1797"),
            (1809, "James Madison", "jamesmadison", "1809-1817"),
            (1829, "Andrew Jackson", "andrewjackson", "1829-1837"),
            (1857, "James Buchanan", "jamesbuchanan", "1857-1861"),
        ],
        "Mexico" => &[
            (1833, "Antonio L. de Santa Anna", "antoniolópezdesantaanna", "1833-1855"),
            (1858, "Benito Juarez", "benitojuárez", "1858-1872"),
        ],
        "Haiti" => &[
            (1807, "Alexandre Petion", "alexandrepétion", "1807-1818"),
            (1818, "Jean-Pierre Boyer", "jeanpierreboyer", "1818-1843"),
            (1847, "Faustin I", "faustini", "1847-1859"),
        ],
        _ => return None,
    };
    let mut chosen: Option<(&str, &str, &str)> = None;
    for (start, name, stem, dates) in timeline {
        if year >= *start {
            chosen = Some((name, stem, dates));
        }
    }
    chosen
}

/// Trait list for a leader: (trait name, effect description). Keyed by the
/// portrait stem so each individual ruler has their own flavor, with a
/// per-nation fallback when a specific ruler is not listed.
fn ruler_traits(nation: &str, stem: &str) -> Vec<(&'static str, &'static str)> {
    // Specific, historically-themed rulers first.
    let specific: &[(&str, &[(&str, &str)])] = &[
        ("georgeiii", &[("Empire Builder", "+10% gold tax"), ("Naval Power", "+15% ship speed")]),
        ("victoria", &[("Industrial Age", "+15% material output"), ("Pax Britannica", "+10% happiness")]),
        ("napoleoni", &[("Grande Armee", "+20% unit power"), ("Conscription", "+15% manpower")]),
        ("napoleoniii", &[("Modernizer", "+10% material output"), ("Ambitious", "+10% unit power")]),
        ("louisxvi", &[("Ancien Regime", "-5% happiness"), ("Court Spending", "+5% gold tax")]),
        ("catherineii", &[("Enlightened Despot", "+10% gold tax"), ("Expansionist", "+10% unit power")]),
        ("nicholasi", &[("Iron Tsar", "+15% unit power"), ("Autocrat", "-5% happiness")]),
        ("alexanderii", &[("Reformer", "+10% happiness"), ("Great Reforms", "+10% manpower")]),
        ("ferdinandvii", &[("Absolutist", "-10% happiness"), ("Colonial Grip", "+5% gold tax")]),
        ("isabellaii", &[("Liberal Monarchy", "+5% happiness"), ("Weak Rule", "-5% unit power")]),
        ("georgewashington", &[("Founding Father", "+15% happiness"), ("Veteran General", "+10% unit power")]),
        ("andrewjackson", &[("Old Hickory", "+15% unit power"), ("Frontier Spirit", "+10% settler rate")]),
        ("jamesbuchanan", &[("Divided Union", "-10% happiness"), ("Cautious", "-5% unit power")]),
        ("antoniolópezdesantaanna", &[("Caudillo", "+10% unit power"), ("Unstable Rule", "-10% happiness")]),
        ("benitojuárez", &[("Reform Leader", "+10% happiness"), ("Resolute", "+5% manpower")]),
        ("alexandrepétion", &[("Republican", "+10% happiness"), ("Land Reform", "+10% food output")]),
    ];
    if let Some((_, traits)) = specific.iter().find(|(s, _)| *s == stem) {
        return traits.to_vec();
    }
    // National defaults for any ruler without a specific entry.
    match nation {
        "Great Britain" | "Canada" => vec![("Royal Navy", "+10% ship speed"), ("Mercantile", "+5% gold tax")],
        // Rupert's Land: the Hudson's Bay Company fur-trading charter.
        "Rupert's Land" => vec![("Fur Charter", "+10% hide output"), ("Company Rule", "+5% gold tax")],
        "France" => vec![("Grande Nation", "+10% unit power"), ("Fine Culture", "+5% happiness")],
        // French Louisiana: a sprawling fur-trade and river-port colony.
        "Louisiana" => vec![("Fur & River Trade", "+10% merchant income"), ("Vast Frontier", "+5% settler rate")],
        // Saint-Domingue: the Pearl of the Antilles, a sugar-and-coffee boom colony.
        "Saint-Domingue" => vec![("Pearl of the Antilles", "+10% material output"), ("Plantation Economy", "+5% gold tax")],
        "Spain" => vec![("Colonial Empire", "+5% gold tax"), ("Devout", "+5% happiness")],
        // Viceroyalties: a resource-rich appointed administration under Madrid.
        "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => vec![("Viceroyalty", "+5% gold tax"), ("Silver & Gold", "+5% material output")],
        "Portugal" => vec![("Age of Discovery", "+10% ship speed"), ("Sugar & Gold", "+5% material output")],
        "Netherlands" => vec![("Merchant Republic", "+10% gold tax"), ("Trading Company", "+5% material output")],
        "Russia" => vec![("Vast Manpower", "+10% manpower"), ("Harsh Winters", "+5% unit power")],
        "Denmark" => vec![("Seafarers", "+10% ship speed"), ("Thrifty", "+5% gold tax")],
        "United States" => vec![("Manifest Destiny", "+10% settler rate"), ("Free Enterprise", "+5% material output")],
        "Mexico" => vec![("Independent Spirit", "+5% happiness"), ("Caudillo Politics", "-5% happiness")],
        "Haiti" => vec![("Hard-Won Freedom", "+10% happiness"), ("Defensive", "+5% unit power")],
        _ => vec![("Local Rule", "No special buffs")],
    }
}

/// Picks a governor/chief sprite sheet key for a settlement. Native places
/// use a chief; Haitian (African-led) settlements use the black governor; all
/// others pick one of the four generic governors by a stable per-name hash so
/// each settlement keeps the same figure.
fn governor_sheet(city: &City) -> &'static str {
    if is_native(&city.owner) || is_native(&city.sovereign) {
        return if name_hash(&city.name) % 2 == 0 { "natchief1" } else { "natchief2" };
    }
    if city.owner == "Haiti" {
        return "blackgovenor";
    }
    const GOVERNORS: [&str; 4] = ["govenor1", "govenor2", "govenor3", "govenor4"];
    GOVERNORS[name_hash(&city.name) % GOVERNORS.len()]
}

/// Rough historical religious make-up of a settlement at a given year, as
/// (image key, label, share). Mirrors `ethnic_mix` but for faith: driven by
/// the ruling power, whether the settlement is native, and the region.
fn religion_mix(city: &City, year: i32) -> Vec<(&'static str, &'static str, f32)> {
    let name = city.name.as_str();
    // Native settlements: animist traditions, with a small Catholic convert
    // share near missions.
    if is_native(&city.owner) || is_native(&city.sovereign) {
        return vec![("animism", "Animism", 0.92), ("catholic", "Catholic", 0.08)];
    }
    let caribbean_fr = matches!(name, "Port-au-Prince" | "Saint-Pierre");
    let mut mix: Vec<(&'static str, &'static str, f32)> = match city.owner.as_str() {
        "Great Britain" | "United States" | "Confederate States" | "Canada" | "Rupert's Land" => {
            // Protestant majority; Catholic minority (Irish, Québécois, Spanish).
            if matches!(name, "Quebec" | "Montreal" | "New Orleans" | "Fort Detroit" | "Mobile") {
                vec![("catholic", "Catholic", 0.80), ("protestant", "Protestant", 0.18), ("animism", "Animism", 0.02)]
            } else {
                vec![("protestant", "Protestant", 0.78), ("catholic", "Catholic", 0.18), ("animism", "Animism", 0.04)]
            }
        }
        "France" => {
            if caribbean_fr {
                vec![("voodoo", "Voodoo", 0.60), ("catholic", "Catholic", 0.38), ("animism", "Animism", 0.02)]
            } else {
                vec![("catholic", "Catholic", 0.90), ("protestant", "Protestant", 0.06), ("animism", "Animism", 0.04)]
            }
        }
        // French Louisiana: Catholic, with African/indigenous traditions kept
        // by the enslaved and frontier population.
        "Louisiana" => vec![("catholic", "Catholic", 0.78), ("animism", "Animism", 0.20), ("protestant", "Protestant", 0.02)],
        // Saint-Domingue: a Catholic planter elite over an enslaved majority
        // that kept its Vodou traditions.
        "Saint-Domingue" => vec![("voodoo", "Voodoo", 0.60), ("catholic", "Catholic", 0.38), ("animism", "Animism", 0.02)],
        "Spain" | "Mexico" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => {
            vec![("catholic", "Catholic", 0.72), ("mesoamerican", "Mesoamerican", 0.24), ("animism", "Animism", 0.04)]
        }
        "Haiti" => vec![("voodoo", "Voodoo", 0.72), ("catholic", "Catholic", 0.28)],
        // Dutch Reformed planter elite over an African-descended majority that
        // kept its own traditions in the plantation colonies.
        "Netherlands" => vec![("protestant", "Protestant", 0.45), ("animism", "Animism", 0.50), ("catholic", "Catholic", 0.05)],
        "Texas" => vec![("protestant", "Protestant", 0.55), ("catholic", "Catholic", 0.43), ("animism", "Animism", 0.02)],
        "Russia" => vec![("orthodox", "Orthodox", 0.60), ("animism", "Animism", 0.40)],
        "Denmark" => vec![("protestant", "Protestant", 0.70), ("animism", "Animism", 0.30)],
        _ => vec![("catholic", "Catholic", 0.80), ("animism", "Animism", 0.20)],
    };
    perturb_mix(&mut mix, name);
    let _ = year;
    mix
}

/// Display label for an ethnicity or religion image key.
fn group_label(key: &str) -> &'static str {
    match key {
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
        "protestant" => "Protestant",
        "catholic" => "Catholic",
        "orthodox" => "Orthodox",
        "voodoo" => "Voodoo",
        "mesoamerican" => "Mesoamerican",
        "animism" => "Animism",
        _ => "Other",
    }
}

/// Every religion in the game: (image key, label).
const ALL_RELIGIONS: &[(&str, &str)] = &[
    ("protestant", "Protestant"),
    ("catholic", "Catholic"),
    ("orthodox", "Orthodox"),
    ("voodoo", "Voodoo"),
    ("mesoamerican", "Mesoamerican"),
    ("animism", "Animism"),
];

/// Per-religion legal status within a nation (rank 0 best -> 4 worst), under
/// its religion law: Free Faith / Limited Faith / Suppressed / Banned. The
/// state religion is always Free; others reflect the era's tolerance.
fn religion_group_status(nation: &str, religion: &str, year: i32) -> u8 {
    let state = match nation {
        "Great Britain" | "United States" | "Confederate States" | "Canada" | "Rupert's Land" | "Texas" | "Denmark" | "Netherlands" => "Protestant",
        "France" | "Louisiana" | "Saint-Domingue" | "Spain" | "Portugal" | "Mexico" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => "Catholic",
        "Russia" => "Orthodox",
        "Haiti" => "Voodoo",
        _ => "Animism",
    };
    if religion == state {
        return 0;
    }
    match nation {
        "Great Britain" | "United States" | "Confederate States" | "Canada" | "Rupert's Land" | "Texas" | "Denmark" | "Netherlands" => match religion {
            "Catholic" => if year >= 1835 { 1 } else { 2 },
            "Orthodox" => 1,
            "Voodoo" | "Mesoamerican" | "Animism" => 3,
            _ => 2,
        },
        "France" | "Louisiana" | "Saint-Domingue" | "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => match religion {
            "Protestant" => 3,
            "Mesoamerican" | "Animism" | "Voodoo" => if year >= 1800 { 3 } else { 4 },
            _ => 2,
        },
        "Mexico" => match religion {
            "Mesoamerican" | "Animism" => 2,
            "Protestant" => 2,
            _ => 1,
        },
        "Haiti" => match religion {
            "Catholic" => 1,
            _ => 2,
        },
        "Russia" => 2,
        _ => 2,
    }
}

/// Religion-law status labels (rank 0 best -> 3 worst).
fn faith_style(rank: u8) -> (&'static str, Color) {
    match rank {
        0 => ("Free Faith", Color::from_rgba(120, 230, 120, 255)),
        1 => ("Limited Faith", Color::from_rgba(230, 210, 110, 255)),
        2 => ("Suppressed", Color::from_rgba(230, 150, 80, 255)),
        _ => ("Banned", Color::from_rgba(230, 90, 90, 255)),
    }
}

/// Every ethnic group in the game: (image key, display label).
const ALL_ETHNICITIES: &[(&str, &str)] = &[
    ("americans", "American"),
    ("british", "British"),
    ("french", "French"),
    ("spanish", "Spanish"),
    ("latinamericans", "Latin American"),
    ("africans", "African"),
    ("indigenous", "Indigenous"),
    ("danes", "Scandinavian"),
    ("russians", "Russian"),
    ("portugese", "Portuguese"),
    ("dutch", "Dutch"),
];

/// Maps a status rank to a (label, color). 0 best -> 4 worst.
fn status_style(rank: u8) -> (&'static str, Color) {
    match rank {
        0 => ("Protected Rights", Color::from_rgba(120, 230, 120, 255)),
        1 => ("Tolerated", Color::from_rgba(180, 220, 140, 255)),
        2 => ("Segregated", Color::from_rgba(230, 210, 110, 255)),
        3 => ("Suppressed", Color::from_rgba(230, 150, 80, 255)),
        _ => ("Enslaved", Color::from_rgba(230, 90, 90, 255)),
    }
}

/// Per-ethnicity legal status within a nation, by group — not one blanket law.
/// Returns a status rank 0 (best) to 4 (worst); the "ruling" group of a nation
/// gets Protected Rights, while others reflect that nation's historical
/// treatment at the time. The `scandinavian` image key is "danes".
fn ethnic_group_status(nation: &str, ethnicity: &str, year: i32) -> u8 {
    // The nation's own dominant ethnicity is always Protected (rank 0).
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
    // Africans: enslaved in slaveholding states until abolition.
    if ethnicity == "African" {
        return match nation {
            "Haiti" => 0,
            "United States" => if year >= 1858 { 4 } else { 4 },
            "Confederate States" | "Texas" => 4,
            "Great Britain" => if year >= 1835 { 2 } else { 4 },
            "France" | "Louisiana" | "Saint-Domingue" => if year >= 1794 && year < 1803 { 2 } else { 4 },
            "Spain" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => if year >= 1835 { 3 } else { 4 },
            // Portugal/Brazil: slavery persisted until 1888, late to abolish.
            "Portugal" => 4,
            // Dutch Suriname: plantation slavery until abolition in 1863.
            "Netherlands" => if year >= 1863 { 3 } else { 4 },
            "Mexico" => 1, // abolished slavery early
            _ => 3,
        };
    }
    // Indigenous peoples: broadly suppressed/displaced by colonial states.
    if ethnicity == "Indigenous" {
        return match nation {
            "United States" | "Confederate States" | "Texas" => 3,
            "Spain" | "Mexico" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => 2, // large mixed/indigenous populace
            "Portugal" => 2, // large mixed/indigenous populace in Brazil
            "Russia" | "Denmark" => 3,
            _ => 3,
        };
    }
    // Other European/American groups: generally tolerated minorities.
    match nation {
        // Catholic/absolutist Spain was harsher on non-Spanish Europeans.
        "Spain" | "Portugal" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain" => 2,
        // The US, Britain, and the mercantile Dutch were open to Europeans.
        "United States" | "Great Britain" | "Canada" | "Netherlands" => 1,
        _ => 2,
    }
}

/// Deterministic identity for a settlement's governor/chief: (full name,
/// ethnicity label, trait list). Driven by a stable per-name hash so each
/// settlement keeps the same leader. The ethnicity is the dominant group in
/// the settlement's historical mix, and the name is drawn from that culture's
/// name pool.
/// The party a settlement's governor belongs to: a deterministic weighted pick
/// from the local party favor, so a governor usually (but not always) belongs
/// to a party that is strong in their town.
fn governor_party(city: &City, favor: &[f32; 4]) -> usize {
    let total: f32 = favor.iter().sum();
    if total <= 0.0 {
        return 0;
    }
    // Deterministic roll in [0, total) from the settlement name hash.
    let roll = (name_hash(&city.name) % 10_000) as f32 / 10_000.0 * total;
    let mut acc = 0.0;
    for (p, &w) in favor.iter().enumerate() {
        acc += w;
        if roll <= acc {
            return p;
        }
    }
    3
}

fn governor_identity(city: &City, year: i32) -> (String, &'static str, &'static str, i32, Vec<(&'static str, &'static str)>) {
    let native = is_native(&city.owner) || is_native(&city.sovereign);
    let nation = if city.sovereign.is_empty() { &city.owner } else { &city.sovereign };
    // The governor must come from an ethnicity that is not suppressed or
    // enslaved under the nation's laws. Pick the largest such group in the
    // settlement; if none qualify, fall back to the ruling nation's own group.
    let ethnicity = if native {
        "Indigenous"
    } else {
        let mut groups: Vec<&(String, f32)> = city.ethnicity.iter().collect();
        groups.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let eligible = groups.iter().find(|(k, _)| {
            // Rank 0 Protected, 1 Tolerated, 2 Segregated are eligible to rule;
            // 3 Suppressed and 4 Enslaved are not.
            ethnic_group_status(nation, group_label(k), year) <= 2
        });
        match eligible {
            Some((k, _)) => group_label(k),
            None => group_label(owner_ethnicity_key(nation)),
        }
    };

    // The governor's faith follows the settlement's religion population,
    // choosing the largest faith that is not suppressed or banned by law.
    // Native chiefs are animist. Falls back to the state faith.
    let religion = if native {
        "Animism"
    } else {
        let mut faiths: Vec<&(String, f32)> = city.religion.iter().collect();
        faiths.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let eligible = faiths.iter().find(|(k, _)| religion_group_status(nation, group_label(k), year) <= 1);
        match eligible {
            Some((k, _)) => group_label(k),
            None => group_label(owner_religion_key(nation)),
        }
    };

    // Name pools per culture.
    let (firsts, lasts): (&[&str], &[&str]) = match ethnicity {
        "British" => (&["William", "James", "Henry", "Charles", "Edward", "Thomas", "George"], &["Hastings", "Clarke", "Whitmore", "Ashford", "Pembroke", "Cromwell"]),
        "American" => (&["John", "Samuel", "Nathaniel", "Benjamin", "Josiah", "Franklin"], &["Carter", "Hale", "Winslow", "Prescott", "Barlow", "Dawes"]),
        "French" => (&["Jean", "Pierre", "Louis", "Henri", "Jacques", "Antoine"], &["Beaumont", "Laval", "Rochefort", "Thibault", "Dufort", "Montcalm"]),
        "Spanish" => (&["Diego", "Alonso", "Felipe", "Rodrigo", "Alvaro", "Carlos"], &["de Vega", "Mendoza", "Castilla", "Herrera", "Salazar", "Ortiz"]),
        "Portuguese" => (&["João", "Manuel", "Duarte", "Afonso", "Henrique", "Sebastião"], &["de Sousa", "Pereira", "Albuquerque", "Carvalho", "Teixeira", "Pombal"]),
        "Dutch" => (&["Willem", "Jan", "Hendrik", "Pieter", "Cornelis", "Dirk"], &["van der Berg", "de Vries", "van Dijk", "Bakker", "Visser", "van Leeuwen"]),
        "Latin American" => (&["Miguel", "Jose", "Agustin", "Mariano", "Ignacio", "Vicente"], &["Herrera", "Reyes", "Montero", "Alcantara", "Delgado", "Guerrero"]),
        "African" => (&["Toussaint", "Henri", "Jean", "Augustin", "Paul", "Baptiste"], &["Laurent", "Christophe", "Belair", "Moise", "Delva", "Lacroix"]),
        "Scandinavian" => (&["Frederik", "Christian", "Niels", "Johan", "Soren", "Mads"], &["Jensen", "Holm", "Dahl", "Lund", "Bang", "Krag"]),
        "Russian" => (&["Ivan", "Alexei", "Nikolai", "Dmitri", "Grigory", "Pavel"], &["Baranov", "Volkov", "Orlov", "Sokolov", "Lebedev", "Popov"]),
        _ => (&["Little Hawk", "Grey Wolf", "Standing Bear", "Red Cloud", "Black Elk", "Swift River"], &[""]),
    };
    let h = name_hash(&city.name);
    let first = firsts[h % firsts.len()];
    let last = lasts[(h / 7) % lasts.len()];
    let full = if last.is_empty() { first.to_string() } else { format!("{first} {last}") };

    // Trait pool: (trait, effect). Pick two or three stable traits.
    const COLONIAL_TRAITS: &[(&str, &str)] = &[
        ("Ambitious", "+10% unit power"),
        ("Just", "+10% happiness"),
        ("Greedy", "+10% gold tax, -5% happiness"),
        ("Pious", "+8% happiness"),
        ("Shrewd Trader", "+15% merchant income"),
        ("Harsh", "+10% troops, -8% happiness"),
        ("Charismatic", "+10% happiness"),
        ("Cautious", "+10% defense, -5% unit power"),
        ("Industrious", "+15% material output"),
        ("Corrupt", "+10% gold tax, -10% happiness"),
        ("Diplomat", "slower to go to war"),
        ("Warmonger", "+15% unit power, -5% happiness"),
        ("Builder", "-15% construction time"),
        ("Scholar", "+10% upgrade speed"),
        ("Ruthless", "+12% unit power, -8% happiness"),
        ("Beloved", "+15% happiness"),
    ];
    const NATIVE_TRAITS: &[(&str, &str)] = &[
        ("Wise", "+10% happiness"),
        ("Fierce Warrior", "+15% unit power"),
        ("Peacemaker", "slower to go to war"),
        ("Great Hunter", "+15% food output"),
        ("Spiritual", "+10% happiness"),
        ("Cunning", "+10% unit power"),
        ("Generous", "+12% happiness"),
        ("Stoic", "+10% troops"),
        ("Swift", "+10% unit speed"),
        ("Honored Elder", "+8% happiness"),
        ("Fearless", "+12% unit power, -5% defense"),
        ("Patient", "+10% defense"),
    ];
    let pool = if native { NATIVE_TRAITS } else { COLONIAL_TRAITS };
    let count = 2 + (h / 11) % 2; // 2 or 3 traits
    let mut traits = Vec::new();
    for i in 0..count {
        let idx = (h / (13 + i * 17)) % pool.len();
        let t = pool[idx];
        if !traits.iter().any(|(n, _)| *n == t.0) {
            traits.push(t);
        }
    }

    // Popularity: how much the settlement likes this leader. Representing the
    // local majority ethnicity/faith is popular; representing a tiny minority
    // is resented. Personality traits add or subtract on top.
    let share_of = |mix: &[(String, f32)], label: &str| -> f32 {
        mix.iter().find(|(k, _)| group_label(k) == label).map(|(_, s)| *s).unwrap_or(0.0)
    };
    let eth_share = share_of(&city.ethnicity, ethnicity);
    let rel_share = share_of(&city.religion, religion);
    // Each maps roughly [0..1] share to [-30..+40] popularity.
    let mut popularity = ((eth_share - 0.4) * 70.0) as i32 + ((rel_share - 0.4) * 70.0) as i32;
    // Trait personality modifiers.
    for (tr, _) in &traits {
        popularity += match *tr {
            "Beloved" => 20,
            "Just" | "Charismatic" | "Generous" | "Wise" | "Honored Elder" => 12,
            "Peacemaker" | "Pious" | "Spiritual" | "Builder" | "Diplomat" => 6,
            "Greedy" | "Cautious" => -6,
            "Harsh" | "Warmonger" | "Fierce Warrior" => -10,
            "Corrupt" | "Ruthless" => -16,
            _ => 0,
        };
    }
    let popularity = popularity.clamp(-100, 100);
    (full, ethnicity, religion, popularity, traits)
}

fn settlement_kind(city: &City) -> &'static str {
    if city.is_camp {
        "Camp"
    } else if city.is_fort {
        "Fort"
    } else if city.is_village {
        "Village"
    } else if city.is_capital {
        "Capital"
    } else {
        "City"
    }
}

/// A settlement's founding year as a display string. Native camps and villages
/// have only approximate traditional dates, so those are shown with a "c."
/// (circa) prefix; permanent colonial foundings are shown as an exact year.
fn founded_label(city: &City) -> String {
    if city.is_camp || (city.is_village && is_native(&city.owner)) {
        format!("c. {}", city.founded_year)
    } else {
        city.founded_year.to_string()
    }
}

fn city_max_hp(city: &City) -> i32 {
    let base = if city.is_camp {
        30
    } else if city.is_village {
        50
    } else if city.is_capital {
        200
    } else {
        100
    };
    base + city.tier * 25
}

fn building_size(city: &City, zoom: f32) -> (f32, f32) {
    if city.is_camp {
        ((7.0 * zoom).max(4.0), (5.0 * zoom).max(3.0))
    } else if city.is_fort || city.is_village {
        ((8.0 * zoom).max(5.0), (6.0 * zoom).max(4.0))
    } else if city.is_capital {
        ((11.0 * zoom).max(7.0), (8.0 * zoom).max(5.0))
    } else {
        ((9.0 * zoom).max(6.0), (7.0 * zoom).max(5.0))
    }
}

fn material_rate(city: &City, biome: u8, panel: bool) -> String {
    let mods = biome_mods(biome);
    match city.material.as_str() {
        "Lumber" => {
            let scale = if panel { 4.0 } else { 2.0 };
            if !panel && mods.2 == 0.0 {
                "None (tundra)".into()
            } else {
                format!("{}/10s", (scale * mods.2) as i32)
            }
        }
        "Hide" => {
            let base = if is_native(&city.owner) { 2.0 } else { 1.0 };
            format!("{}/5s", (base * mods.3) as i32)
        }
        "Iron" | "Coal" => "1/15s".into(),
        _ => "N/A".into(),
    }
}

fn food_rate(city: &City, biome: u8) -> String {
    let base = if is_native(&city.owner) { 40.0 } else { 20.0 };
    format!("+{}/1s", (base * biome_mods(biome).1) as i32)
}

fn gold_tax(city: &City, biome: u8, panel: bool) -> i32 {
    let mut mult = biome_mods(biome).4;
    if panel && city.buildings.iter().any(|b| b == "Town Hall" || b == "Council Lodge") {
        mult *= 1.25;
    }
    let base = if city.is_village {
        if is_native(&city.owner) { 0 } else { 4 }
    } else if city.is_fort || city.is_camp {
        4
    } else {
        10
    };
    (base as f32 * mult) as i32
}

fn storage_cap(game: &Game, owner: &str) -> i32 {
    let warehouses = game.cities.iter().filter(|c| c.owner == owner && c.buildings.iter().any(|b| b == "Warehouse")).count();
    100 + warehouses as i32 * 100
}

fn unit_status(unit: &Unit, dist: f32) -> &'static str {
    if unit.opponent.is_some() {
        "Fighting"
    } else if unit.retreating {
        "Retreating"
    } else if unit.ship {
        "Sailing"
    } else if unit.behavior == "Guard" {
        "Guarding"
    } else if unit.behavior == "Patrol" || unit.patrolling {
        if unit.idle {
            "Idle"
        } else {
            "Patrolling"
        }
    } else if unit.idle {
        "Idle"
    } else if dist <= 5.0 {
        "Sieging"
    } else {
        "Attacking"
    }
}

fn panel_overlay(city: &City) -> Option<(&'static str, f32, f32, f32, f32)> {
    let native_sov = is_native(&city.sovereign);
    let native_owner = is_native(&city.owner);
    if city.is_fort && !native_sov {
        return Some(("fort", 150.0, 113.0, 50.0, 52.0));
    }
    if (city.is_camp || city.is_village) && native_sov && native_owner {
        return Some(("camp", 113.0, 85.0, 68.5, 80.0));
    }
    if city.is_fort || city.is_camp || city.is_village {
        return None;
    }
    // Architecture follows the original builder, not the current owner, so a
    // conquered city keeps its founder's look (e.g. Quebec stays French).
    let (key, oy) = if matches!(city.builder.as_str(), "Great Britain" | "United States" | "Texas" | "Confederate States" | "Canada" | "Rupert's Land" | "Netherlands") {
        (tier_building("brit", city.tier), 100.0)
    } else if matches!(city.builder.as_str(), "Spain" | "Portugal" | "Mexico" | "New Granada" | "Peru" | "Rio de la Plata" | "New Spain") {
        (tier_building("span", city.tier), 100.0)
    } else if matches!(city.builder.as_str(), "France" | "Haiti" | "Louisiana" | "Saint-Domingue") {
        (tier_building("fren", city.tier), 100.0)
    } else if city.builder == "Russia" {
        ("rus", 100.0)
    } else if city.builder == "Denmark" {
        ("dan", 100.0)
    } else {
        return None;
    };
    Some((key, 113.0, 85.0, 68.5, oy))
}

fn tier_building(prefix: &str, tier: i32) -> &'static str {
    match (prefix, tier) {
        ("brit", 3..) => "brit3",
        ("brit", 2) => "brit2",
        ("brit", _) => "brit",
        ("span", 3..) => "span3",
        ("span", 2) => "span2",
        ("span", _) => "span",
        ("fren", 3..) => "fren3",
        ("fren", 2) => "fren2",
        _ => "fren",
    }
}

fn cost_label(name: &str, native: bool, gold: i32, lumber: i32, hide: i32, iron: i32, coal: i32, food: i32) -> String {
    let parts: Vec<(i32, &str)> = if native && name == "Market" {
        vec![(20, "F"), (15, "L"), (5, "H")]
    } else if native && name == "Warehouse" {
        vec![(30, "L"), (10, "H")]
    } else if native && name == "Lumber Mill" {
        vec![(15, "L"), (5, "H")]
    } else if native && name == "Fur Trading Post" {
        vec![(40, "F"), (20, "L"), (10, "H")]
    } else {
        vec![(gold, "G"), (food, "F"), (lumber, "L"), (hide, "H"), (iron, "I"), (coal, "C")]
    };
    parts.into_iter().filter(|(n, _)| *n > 0).map(|(n, k)| format!("{n}{k}")).collect::<Vec<_>>().join(" ")
}

fn building_blurb(name: &str) -> &'static str {
    match name {
        "Town Hall" => "+25% gold tax, enables upgrades",
        "Council Lodge" => "+25% gold tax, enables upgrades",
        "Road" => "+25% unit speed, connects cities",
        "Market" => "+25% merchant income, +1 merchant cap",
        "Warehouse" => "+100 resource storage",
        "Lumber Mill" => "2x lumber production",
        "Plantation" => "Produces luxury (Sugar/Tobacco/Coffee/Cotton/Cocoa)",
        "Fur Trading Post" => "Produces luxury (Fur)",
        _ => "",
    }
}

fn diplomacy(lines: &[String], owner: &str) -> (Vec<String>, Vec<String>) {
    let mut wars = Vec::new();
    let mut allies = Vec::new();
    for line in lines {
        if let Some(rest) = line.strip_prefix("WAR: ") {
            if let Some((a, b)) = rest.split_once(" vs ") {
                if a == owner {
                    wars.push(b.to_string());
                } else if b == owner {
                    wars.push(a.to_string());
                }
            }
        } else if let Some(rest) = line.strip_prefix("ALLIANCE: ") {
            let rest = rest.split(" (").next().unwrap_or(rest);
            if let Some((a, b)) = rest.split_once(" & ") {
                if a == owner {
                    allies.push(b.to_string());
                } else if b == owner {
                    allies.push(a.to_string());
                }
            }
        }
    }
    (wars, allies)
}

//! Colonial Simulator — Rust port.
//!
//! Run from the repository with `cargo run --release`. Images are loaded from
//! the `images/` directory next to this crate. A Windows build with
//! `--features bundle` carries those images inside the executable.

mod art;
mod config;
mod sim;
mod world;

use art::{
    alt_flags, codex_flag, codex_nations, codex_variants, gold_panel, land_clip, outlined, outlined_center,
    water_clip, Art, Boom, Marcher,
};
use config::{
    biome_mods, biome_name, building, faction_color, is_native, BUILDINGS, COORD_SCALE, DAY_TICKS, ISLANDS, MAP_H,
    MAP_W, PAN_SPEED,
};
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
        fullscreen: true,
        window_width: 1600,
        window_height: 900,
        ..Default::default()
    }
}

enum Mode {
    Menu,
    Scenarios { scroll: f32 },
    Codex { nation: Option<String>, variant: i32 },
    Play,
    Paused,
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
    marchers: Vec<Marcher>,
    booms: Vec<Boom>,
    farms: HashMap<u32, Option<(f32, f32, usize)>>,
    plantations: HashMap<u32, Option<(f32, f32)>>,
    mountains: Vec<(f32, f32)>,
}

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
    let map_tex = Texture2D::from_rgba8(maps.w as u16, maps.h as u16, &maps.display);
    map_tex.set_filter(FilterMode::Linear);
    let art = Art::load(&images);
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
        fullscreen: true,
        anim: 0,
        marchers,
        booms: Vec::new(),
        farms: HashMap::new(),
        plantations: HashMap::new(),
        mountains,
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
            self.anim = self.anim.wrapping_add(1);
        }
        match self.mode {
            Mode::Menu => self.menu(),
            Mode::Scenarios { .. } => self.scenarios(),
            Mode::Codex { .. } => self.codex(),
            Mode::Play => self.play(false),
            Mode::Paused => self.play(true),
        }
    }

    fn menu(&mut self) {
        self.backdrop();
        self.title_block();
        let killed = self.kill_marcher();
        let w = screen_width();
        let h = screen_height();
        let play = Rect::new(w / 2.0 - 100.0, h / 2.0 - 30.0, 200.0, 60.0);
        let codex = Rect::new(w / 2.0 - 100.0, h / 2.0 + 50.0, 200.0, 60.0);
        let exit = Rect::new(w / 2.0 - 100.0, h / 2.0 + 130.0, 200.0, 60.0);
        self.art.button(play, "Play", 36.0);
        self.art.button(codex, "Codex", 36.0);
        self.art.button(exit, "Exit", 36.0);
        if !killed {
            if left_click(play) {
                self.mode = Mode::Scenarios { scroll: 0.0 };
            } else if left_click(codex) {
                self.mode = Mode::Codex { nation: None, variant: 0 };
            } else if left_click(exit) {
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
        if !killed && (left_click(back) || is_key_pressed(KeyCode::Escape)) {
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
                if left_click(cell) {
                    nation = Some((*name).to_string());
                    variant = 0;
                }
            }
            let back = Rect::new(w / 2.0 - 60.0, gy + grid_h + 20.0, 120.0, 40.0);
            self.art.button(back, "Back", 20.0);
            if left_click(back) || is_key_pressed(KeyCode::Escape) {
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
                if left_click(left) {
                    variant -= 1;
                } else if left_click(right) {
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
            if left_click(back) {
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

    fn play(&mut self, paused: bool) {
        let paper = self.game.paper.is_some();
        if !paper && !paused {
            self.map_clicks();
            self.camera_input();
            self.speed_keys();
            if is_key_pressed(KeyCode::Escape) {
                self.mode = Mode::Paused;
            }
            self.game.tick(&self.maps);
            if self.game.claims_dirty {
                self.refresh_overlay();
                self.game.claims_dirty = false;
            }
        }
        self.paint_world();
        if let Some((title, body)) = self.game.paper.clone() {
            if self.paint_paper(&title, &body) {
                self.game.paper = None;
            }
        }
        if paused {
            self.draw_pause();
        }
    }

    fn paint_world(&mut self) {
        clear_background(Color::from_rgba(10, 10, 30, 255));
        stretch(
            &self.map_tex,
            Rect::new(self.cam_x, self.cam_y, MAP_W as f32 * self.zoom, MAP_H as f32 * self.zoom),
        );
        if let Some(overlay) = &self.overlay {
            stretch(overlay, Rect::new(self.cam_x, self.cam_y, MAP_W as f32 * self.zoom, MAP_H as f32 * self.zoom));
        }
        let tint = match self.game.season() {
            "Winter" => Some(Color::from_rgba(220, 230, 255, 25)),
            "Autumn" => Some(Color::from_rgba(255, 140, 40, 18)),
            "Summer" => Some(Color::from_rgba(255, 255, 180, 12)),
            _ => None,
        };
        if let Some(tint) = tint {
            draw_rectangle(0.0, 0.0, screen_width(), screen_height(), tint);
        }
        self.draw_mountains();
        self.paint_plots();
        self.draw_cities();
        self.draw_smoke();
        self.draw_armies();
        self.draw_merchants();
        self.draw_settlers();
        self.draw_hover();
        self.draw_city_panel();
        self.draw_region_highlight();
        self.draw_region_panel();
        self.draw_news();
        self.draw_faction_panel();
        self.draw_date();
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
                self.booms.push(Boom { x: x + w * 0.5 - 120.0, y: y + hh * 0.5 - 120.0, tick: 0 });
                return true;
            }
        }
        false
    }

    fn draw_march(&mut self) {
        let w = screen_width();
        let h = screen_height();
        let mut rng = ::rand::thread_rng();
        for marcher in &mut self.marchers {
            if marcher.dead {
                continue;
            }
            marcher.x += 0.8;
            if marcher.x > w + 160.0 {
                marcher.x = -160.0 - rng.gen_range(0.0..200.0);
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
        self.cam_x = self.cam_x.clamp(min_x.min(0.0), 0.0);
        self.cam_y = self.cam_y.clamp(min_y.min(0.0), 0.0);
    }

    fn map_clicks(&mut self) {
        if !is_mouse_button_pressed(MouseButton::Left) {
            return;
        }
        let (mx, my) = mouse_position();
        if self.news_toggle_rect().contains(vec2(mx, my)) {
            self.news_open = !self.news_open;
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
            self.selected = if self.selected == hit { None } else { hit };
            self.selected_region = -1;
        } else {
            self.selected = None;
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
        if self.selected.is_some() {
            if Rect::new(6.0, 6.0, 420.0, 180.0).contains(p) {
                return true;
            }
            if self.city_panel_rect().contains(p) {
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
    }

    fn speed_click(&mut self, mx: f32, my: f32) -> bool {
        if !is_mouse_button_pressed(MouseButton::Left) {
            return false;
        }
        let layout = self.date_layout();
        for (rect, spd) in layout.buttons {
            if rect.contains(vec2(mx, my)) {
                self.game.speed = spd;
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
        for city in &self.game.cities {
            let (sx, sy) = self.world_to_screen(city.x, city.y);
            if !on_screen(sx, sy, 120.0) {
                continue;
            }
            let (bw, bh) = building_size(city, self.zoom);
            let tex = self.art.map_building(&city.owner, &city.sovereign, city.is_camp, city.is_fort, city.is_village, city.tier);
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
            let tier = if city.tier > 0 { format!(" [T{}]", city.tier) } else { String::new() };
            outlined_center(&format!("{}{tier}", city.name), sx, name_y, 16.0, WHITE);
            outlined_center(&city.troops.to_string(), sx, sy + (10.0 * self.zoom).max(8.0), 16.0, WHITE);
            if city.owner != city.sovereign {
                let c = faction_color(&city.owner);
                outlined_center("(occupied)", sx, name_y + 12.0, 16.0, Color::from_rgba(c[0], c[1], c[2], 255));
            }
            if self.game.is_flooded(city.id) {
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
                } else if status == "Idle" {
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
            draw_circle(sx, sy, (3.0 * self.zoom).max(2.0), color);
            if self.zoom >= 1.3 {
                outlined_center("Settler", sx, sy - (12.0 * self.zoom).max(10.0), 16.0, WHITE);
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
            (format!("Owner: {}", city.owner), WHITE),
            (format!("Region: #{}", city.region), Color::from_rgba(180, 200, 255, 255)),
            (format!("Population: {}", city.population), Color::from_rgba(200, 230, 255, 255)),
            (format!("Health: {}/{}", city.troops, city_max_hp(city)), WHITE),
            (format!("Tier: {}", city.tier), WHITE),
            (format!("Biome: {}", titled(biome_name(biome))), WHITE),
            (format!("Material: {} ({})", city.material, material_rate(city, biome, true)), WHITE),
            (format!("Gold Tax: +{}/day", gold_tax(city, biome, true)), Color::from_rgba(255, 220, 50, 255)),
            (format!("Storage: {}", storage_cap(&self.game, &city.owner)), Color::from_rgba(200, 200, 200, 255)),
            (format!("Happiness: {happiness}"), hap_color),
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

    fn draw_pause(&mut self) {
        let w = screen_width();
        let h = screen_height();
        draw_rectangle(0.0, 0.0, w, h, Color::from_rgba(0, 0, 0, 100));
        label_center("PAUSED", w * 0.5, h / 2.0 - 150.0, 36.0, gold());
        let resume = Rect::new(w / 2.0 - 100.0, h / 2.0 - 80.0, 200.0, 50.0);
        let menu = Rect::new(w / 2.0 - 100.0, h / 2.0 - 10.0, 200.0, 50.0);
        let exit = Rect::new(w / 2.0 - 100.0, h / 2.0 + 60.0, 200.0, 50.0);
        self.art.button(resume, "Resume", 28.0);
        self.art.button(menu, "Main Menu", 28.0);
        self.art.button(exit, "Exit", 28.0);
        if left_click(resume) || is_key_pressed(KeyCode::Escape) {
            self.mode = Mode::Play;
        } else if left_click(menu) {
            self.game.paper = None;
            self.mode = Mode::Menu;
        } else if left_click(exit) {
            std::process::exit(0);
        }
    }

    fn refresh_overlay(&mut self) {
        if self.maps.overlay_w == 0 || self.maps.overlay_h == 0 {
            return;
        }
        let colors = self.game.region_owners(self.maps.region_count);
        let pixels = self.maps.overlay(&colors);
        let tex = Texture2D::from_rgba8(self.maps.overlay_w as u16, self.maps.overlay_h as u16, &pixels);
        tex.set_filter(FilterMode::Nearest);
        self.overlay = Some(tex);
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

fn blit_tint(tex: &Texture2D, x: f32, y: f32, w: f32, h: f32, color: Color) {
    draw_texture_ex(tex, x, y, color, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
}

fn label_center(text: &str, cx: f32, top: f32, size: f32, color: Color) {
    let dims = measure_text(text, None, size as u16, 1.0);
    draw_text(text, cx - dims.width * 0.5, top + dims.offset_y, size, color);
}

fn shadow_center(text: &str, cx: f32, top: f32, size: f32, color: Color) {
    label_center(text, cx + 1.0, top + 1.0, size, BLACK);
    label_center(text, cx, top, size, color);
}

fn text_top(text: &str, x: f32, top: f32, size: f32, color: Color) {
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
    } else if unit.idle {
        "Idle"
    } else if unit.patrolling {
        "Patrolling"
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
    let (key, oy) = if matches!(city.sovereign.as_str(), "Great Britain" | "United States" | "Texas" | "Confederate States" | "Canada" | "Rupert's Land") {
        (tier_building("brit", city.tier), 100.0)
    } else if matches!(city.sovereign.as_str(), "Spain" | "Mexico") {
        (tier_building("span", city.tier), 100.0)
    } else if matches!(city.sovereign.as_str(), "France" | "Haiti") {
        (tier_building("fren", city.tier), 100.0)
    } else if city.sovereign == "Russia" {
        ("rus", 100.0)
    } else if city.sovereign == "Denmark" {
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

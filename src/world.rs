//! Satellite map, collision mask, climates, and territory regions.
//!
//! Collision follows `collision.py`: red pixels are land, blue pixels are water.
//! Biomes follow the palette in `index.py`.
//!
//! `regionmap.png` draws each territory as a solid color inside a black border.
//! Those borders are only a few pixels wide on the full image, so the detection
//! grid is 2400px (the width where the Python port's region count stabilizes).
//! The on-map overlay is painted at native map size, with the black border
//! pixels drawn on top of the ownership tint.

use crate::config::{MAP_H, MAP_W};
use std::path::Path;

/// Width of the territory detection grid. Thinner grids merge neighboring tiles.
const REGION_GRID_W: u32 = 2400;
const SPECK_PX: usize = 40;

pub struct Maps {
    pub w: u32,
    pub h: u32,
    pub display: Vec<u8>,
    pub water: Vec<bool>,
    pub biome: Vec<u8>,
    pub region_w: u32,
    pub region_h: u32,
    pub region_id: Vec<i32>,
    pub region_count: i32,
    pub overlay_w: u32,
    pub overlay_h: u32,
    region_colors: Vec<[u8; 3]>,
    region_centroids: Vec<(i32, i32)>,
    region_edge: Vec<Vec<(u16, u16)>>,
    /// Per native-map pixel: 0 ocean, 1 black border, 2 land interior.
    overlay_kind: Vec<u8>,
}

impl Maps {
    pub fn load(images: &Path) -> Result<Self, String> {
        let w = MAP_W;
        let h = MAP_H;
        eprintln!("Loading map ({w}x{h})...");
        let display = load_rgba(&images.join("nasasatelliteview.jpg"), w, h)?;
        let collision = load_rgba(&images.join("collisonmap.png"), w, h)?;
        let climate = load_rgba(&images.join("climates.png"), w, h)?;
        let mut water = vec![false; (w * h) as usize];
        let mut biome = vec![0u8; (w * h) as usize];
        for i in 0..(w * h) as usize {
            let r = collision[i * 4];
            let b = collision[i * 4 + 2];
            water[i] = b > r;
            biome[i] = classify_biome(climate[i * 4], climate[i * 4 + 1], climate[i * 4 + 2], (i as u32 / w) as i32, h);
        }
        let regions = load_regions(&images.join("regionmap.png"));
        eprintln!("Map ready. {} territories.", regions.count);
        Ok(Self {
            w,
            h,
            display,
            water,
            biome,
            region_w: regions.w,
            region_h: regions.h,
            region_id: regions.ids,
            region_count: regions.count,
            overlay_w: regions.overlay_w,
            overlay_h: regions.overlay_h,
            region_colors: regions.colors,
            region_centroids: regions.centroids,
            region_edge: regions.edges,
            overlay_kind: regions.overlay_kind,
        })
    }

    pub fn is_water(&self, x: f32, y: f32) -> bool {
        let ix = x as i32;
        let iy = y as i32;
        if ix < 0 || iy < 0 || ix >= self.w as i32 || iy >= self.h as i32 {
            return true;
        }
        self.water[(iy as u32 * self.w + ix as u32) as usize]
    }

    pub fn biome_at(&self, x: f32, y: f32) -> u8 {
        let ix = x as i32;
        let iy = y as i32;
        if ix < 0 || iy < 0 || ix >= self.w as i32 || iy >= self.h as i32 {
            return 0;
        }
        self.biome[(iy as u32 * self.w + ix as u32) as usize]
    }

    /// Region under a native-map point. A click on the black border resolves to
    /// the nearest land tile, matching `_region_at` in `index.py`.
    pub fn region_at(&self, x: f32, y: f32) -> i32 {
        if self.region_w == 0 || self.region_h == 0 {
            return -1;
        }
        let gx = (x / self.w as f32 * self.region_w as f32) as i32;
        let gy = (y / self.h as f32 * self.region_h as f32) as i32;
        let w = self.region_w as i32;
        let h = self.region_h as i32;
        if gx < 0 || gy < 0 || gx >= w || gy >= h {
            return -1;
        }
        let rid = self.id_at(gx, gy);
        if rid >= 0 {
            return rid;
        }
        let max_r = (w / 120).max(6);
        for radius in 1..=max_r {
            for dx in -radius..=radius {
                for dy in [-radius, radius] {
                    if let Some(id) = self.land_at(gx + dx, gy + dy, w, h) {
                        return id;
                    }
                }
            }
            for dy in (-radius + 1)..radius {
                for dx in [-radius, radius] {
                    if let Some(id) = self.land_at(gx + dx, gy + dy, w, h) {
                        return id;
                    }
                }
            }
        }
        -1
    }

    pub fn region_color(&self, rid: i32) -> [u8; 3] {
        self.region_colors.get(rid as usize).copied().unwrap_or([150, 150, 150])
    }

    /// Grid centroid converted to native map coordinates.
    pub fn region_centroid_world(&self, rid: i32) -> Option<(f32, f32)> {
        let (gx, gy) = self.region_centroids.get(rid as usize).copied()?;
        if self.region_w == 0 || self.region_h == 0 {
            return None;
        }
        Some((
            (gx as f32 + 0.5) / self.region_w as f32 * self.w as f32,
            (gy as f32 + 0.5) / self.region_h as f32 * self.h as f32,
        ))
    }

    /// Land cells that touch a border, another region, or the grid edge.
    pub fn region_edges(&self, rid: i32) -> &[(u16, u16)] {
        if rid < 0 {
            return &[];
        }
        self.region_edge.get(rid as usize).map(Vec::as_slice).unwrap_or(&[])
    }

    /// RGBA overlay at native map size. `owner_rgba[region]` tints a claimed
    /// tile; None stays the unclaimed gray. Black border pixels are painted last.
    pub fn overlay(&self, owner_rgba: &[Option<[u8; 4]>]) -> Vec<u8> {
        let ow = self.overlay_w;
        let oh = self.overlay_h;
        let n = (ow * oh) as usize;
        let mut px = vec![0u8; n * 4];
        if ow == 0 || oh == 0 || self.region_w == 0 || self.region_h == 0 {
            return px;
        }
        let gw = self.region_w;
        let gh = self.region_h;
        for y in 0..oh {
            let gy = y * gh / oh;
            let grow = (gy * gw) as usize;
            let orow = (y * ow) as usize;
            for x in 0..ow {
                let i = orow + x as usize;
                let tint = match self.overlay_kind[i] {
                    1 => [0, 0, 0, 110],
                    2 => {
                        let gx = x * gw / ow;
                        let id = self.region_id[grow + gx as usize];
                        if id >= 0 {
                            owner_rgba.get(id as usize).copied().flatten().unwrap_or([130, 130, 130, 70])
                        } else {
                            [130, 130, 130, 70]
                        }
                    }
                    _ => continue,
                };
                let o = i * 4;
                px[o] = tint[0];
                px[o + 1] = tint[1];
                px[o + 2] = tint[2];
                px[o + 3] = tint[3];
            }
        }
        px
    }

    fn id_at(&self, x: i32, y: i32) -> i32 {
        self.region_id[(y as u32 * self.region_w + x as u32) as usize]
    }

    fn land_at(&self, x: i32, y: i32, w: i32, h: i32) -> Option<i32> {
        if x < 0 || y < 0 || x >= w || y >= h {
            return None;
        }
        let id = self.id_at(x, y);
        if id >= 0 { Some(id) } else { None }
    }
}

fn load_rgba(path: &Path, w: u32, h: u32) -> Result<Vec<u8>, String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let src = img.to_rgba8();
    let sw = src.width();
    let sh = src.height();
    let raw = src.into_raw();
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let sy = (y as u64 * sh as u64 / h as u64) as u32;
        for x in 0..w {
            let sx = (x as u64 * sw as u64 / w as u64) as u32;
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((y * w + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&raw[si..si + 4]);
        }
    }
    Ok(out)
}

fn classify_biome(r: u8, g: u8, b: u8, y: i32, h: u32) -> u8 {
    if b > 200 && r < 40 && g < 40 {
        return if y < 200 {
            2
        } else if y > (h as i32 * 350 / 2205) {
            6
        } else {
            0
        };
    }
    const PAL: [(u8, (u8, u8, u8)); 9] = [
        (0, (0x0f, 0xc9, 0x38)),
        (1, (0x09, 0x61, 0x1c)),
        (2, (0x98, 0xda, 0xeb)),
        (3, (0xfe, 0xff, 0x00)),
        (4, (0xff, 0x3a, 0x00)),
        (5, (0x81, 0x81, 0x81)),
        (6, (0xa3, 0x00, 0xd0)),
        (7, (0x52, 0x3f, 0x10)),
        (8, (0xf9, 0x1e, 0x59)),
    ];
    let mut best = 0u8;
    let mut best_d = i32::MAX;
    for (id, (cr, cg, cb)) in PAL {
        let d = (r as i32 - cr as i32).abs() + (g as i32 - cg as i32).abs() + (b as i32 - cb as i32).abs();
        if d < best_d {
            best = id;
            best_d = d;
        }
    }
    if best_d < 120 { best } else { 0 }
}

struct Regions {
    w: u32,
    h: u32,
    ids: Vec<i32>,
    count: i32,
    colors: Vec<[u8; 3]>,
    centroids: Vec<(i32, i32)>,
    edges: Vec<Vec<(u16, u16)>>,
    overlay_w: u32,
    overlay_h: u32,
    overlay_kind: Vec<u8>,
}

fn empty_regions() -> Regions {
    Regions {
        w: 0,
        h: 0,
        ids: Vec::new(),
        count: 0,
        colors: Vec::new(),
        centroids: Vec::new(),
        edges: Vec::new(),
        overlay_w: 0,
        overlay_h: 0,
        overlay_kind: Vec::new(),
    }
}

fn load_regions(path: &Path) -> Regions {
    let img = match image::open(path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            eprintln!("region map skipped ({}): {e}", path.display());
            return empty_regions();
        }
    };
    let sw = img.width();
    let sh = img.height();
    if sw == 0 || sh == 0 {
        return empty_regions();
    }
    let raw = img.into_raw();
    let gw = REGION_GRID_W;
    let gh = (gw as f64 * MAP_H as f64 / MAP_W as f64) as u32;
    let (ids, count, colors, centroids, edges) = detect_regions(&raw, sw, sh, gw, gh.max(1));
    let overlay_w = MAP_W;
    let overlay_h = MAP_H;
    let overlay_kind = classify_overlay(&raw, sw, sh, overlay_w, overlay_h);
    Regions { w: gw, h: gh.max(1), ids, count, colors, centroids, edges, overlay_w, overlay_h, overlay_kind }
}

/// Pillow's NEAREST resize: sample the source pixel under the destination center.
fn nearest_index(dst: u32, dst_len: u32, src_len: u32) -> u32 {
    let center = (dst as f64 + 0.5) * (src_len as f64) / (dst_len as f64) - 0.5;
    let ix = center.floor() as i32;
    ix.clamp(0, src_len as i32 - 1) as u32
}

fn pixel_class(r: u8, g: u8, b: u8) -> u8 {
    if r > 235 && g > 235 && b > 235 {
        0
    } else if r <= 3 && g <= 3 && b <= 3 {
        1
    } else {
        2
    }
}

fn detect_regions(
    raw: &[u8],
    sw: u32,
    sh: u32,
    gw: u32,
    gh: u32,
) -> (Vec<i32>, i32, Vec<[u8; 3]>, Vec<(i32, i32)>, Vec<Vec<(u16, u16)>>) {
    let xs: Vec<u32> = (0..gw).map(|x| nearest_index(x, gw, sw)).collect();
    let ys: Vec<u32> = (0..gh).map(|y| nearest_index(y, gh, sh)).collect();
    let n = (gw * gh) as usize;
    let mut land = vec![false; n];
    for y in 0..gh {
        let src_row = (ys[y as usize] * sw) as usize;
        let dst_row = (y * gw) as usize;
        for x in 0..gw {
            let si = (src_row + xs[x as usize] as usize) * 4;
            land[dst_row + x as usize] = pixel_class(raw[si], raw[si + 1], raw[si + 2]) == 2;
        }
    }
    let (ids, count) = label_land(&mut land, gw, gh);
    let mut sum_r = vec![0u64; count as usize];
    let mut sum_g = vec![0u64; count as usize];
    let mut sum_b = vec![0u64; count as usize];
    let mut sum_x = vec![0u64; count as usize];
    let mut sum_y = vec![0u64; count as usize];
    let mut seen = vec![0u32; count as usize];
    let mut edges = vec![Vec::new(); count as usize];
    let wi = gw as i32;
    let hi = gh as i32;
    for y in 0..hi {
        let row = (y as u32 * gw) as usize;
        for x in 0..wi {
            let id = ids[row + x as usize];
            if id < 0 {
                continue;
            }
            let slot = id as usize;
            let si = ((ys[y as usize] * sw + xs[x as usize]) * 4) as usize;
            sum_r[slot] += raw[si] as u64;
            sum_g[slot] += raw[si + 1] as u64;
            sum_b[slot] += raw[si + 2] as u64;
            sum_x[slot] += x as u64;
            sum_y[slot] += y as u64;
            seen[slot] += 1;
            let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().any(|(dx, dy)| {
                let nx = x + dx;
                let ny = y + dy;
                nx < 0 || ny < 0 || nx >= wi || ny >= hi || ids[(ny as u32 * gw + nx as u32) as usize] != id
            });
            if edge {
                edges[slot].push((x as u16, y as u16));
            }
        }
    }
    let colors = (0..count as usize)
        .map(|i| {
            let c = (seen[i] as u64).max(1);
            [(sum_r[i] / c) as u8, (sum_g[i] / c) as u8, (sum_b[i] / c) as u8]
        })
        .collect();
    let centroids = (0..count as usize)
        .map(|i| {
            let c = (seen[i] as u64).max(1);
            ((sum_x[i] / c) as i32, (sum_y[i] / c) as i32)
        })
        .collect();
    (ids, count, colors, centroids, edges)
}

fn label_land(land: &mut [bool], w: u32, h: u32) -> (Vec<i32>, i32) {
    let n = land.len();
    let mut ids = vec![-1i32; n];
    let mut count = 0i32;
    let mut stack = Vec::new();
    let mut component = Vec::new();
    let wi = w as i32;
    let hi = h as i32;
    for start in 0..n {
        if !land[start] || ids[start] >= 0 {
            continue;
        }
        ids[start] = count;
        stack.clear();
        component.clear();
        stack.push(start);
        component.push(start);
        while let Some(p) = stack.pop() {
            let x = (p as u32 % w) as i32;
            let y = (p as u32 / w) as i32;
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= wi || ny >= hi {
                    continue;
                }
                let ni = (ny as u32 * w + nx as u32) as usize;
                if land[ni] && ids[ni] < 0 {
                    ids[ni] = count;
                    stack.push(ni);
                    component.push(ni);
                }
            }
        }
        if component.len() < SPECK_PX {
            for &p in &component {
                ids[p] = -1;
                land[p] = false;
            }
        } else {
            count += 1;
        }
    }
    (ids, count)
}

fn classify_overlay(raw: &[u8], sw: u32, sh: u32, ow: u32, oh: u32) -> Vec<u8> {
    let xs: Vec<u32> = (0..ow).map(|x| nearest_index(x, ow, sw)).collect();
    let ys: Vec<u32> = (0..oh).map(|y| nearest_index(y, oh, sh)).collect();
    let mut kind = vec![0u8; (ow * oh) as usize];
    for y in 0..oh {
        let src_row = (ys[y as usize] * sw) as usize;
        let dst_row = (y * ow) as usize;
        for x in 0..ow {
            let si = (src_row + xs[x as usize] as usize) * 4;
            kind[dst_row + x as usize] = pixel_class(raw[si], raw[si + 1], raw[si + 2]);
        }
    }
    kind
}

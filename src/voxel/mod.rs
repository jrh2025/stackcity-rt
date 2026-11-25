//! 純 Rust Sprite-Stacking 3D 體素引擎核心。
//!
//! 座標系嚴格遵循精靈堆疊專案約定：
//! - 記憶體配置為平坦陣列，索引順序為 `(z * L + y) * W + x`
//! - `z ∈ [0, H-1]`：`z = 0` 為地面層，`z = H-1` 為頂端
//! - `y ∈ [0, L-1]`：`+y`（較大索引）為物件正面（Front）朝向
//! - `x ∈ [0, W-1]`：左右寬度軸

pub mod ai;
pub mod blueprint;
pub mod carver;
pub mod generators;

use image::{ImageEncoder, Rgba, RgbaImage};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;

pub type Rgb = [u8; 3];
pub type RgbaPx = [u8; 4];

/// 內建像素美術基礎色票庫（與精靈堆疊 Studio 完全相容）
pub fn resolve_palette_color(name: &str) -> Option<Rgb> {
    match name.trim().to_ascii_lowercase().as_str() {
        "red" => Some([208, 48, 56]),
        "vermilion" => Some([218, 62, 44]),
        "blue" => Some([52, 114, 206]),
        "navy" => Some([34, 58, 114]),
        "sky" => Some([104, 178, 228]),
        "green" => Some([58, 154, 78]),
        "emerald" => Some([42, 140, 102]),
        "jade" => Some([78, 162, 142]),
        "yellow" => Some([228, 188, 58]),
        "orange" => Some([226, 124, 42]),
        "purple" => Some([132, 78, 190]),
        "pink" => Some([224, 112, 154]),
        "sakura" => Some([236, 156, 184]),
        "cyan" => Some([58, 192, 212]),
        "white" => Some([224, 228, 236]),
        "black" => Some([32, 36, 46]),
        "gray" | "grey" => Some([136, 144, 158]),
        "dark_gray" => Some([74, 82, 96]),
        "gold" => Some([224, 174, 54]),
        "silver" => Some([184, 194, 210]),
        "brown" => Some([128, 82, 48]),
        "wood" => Some([148, 98, 58]),
        "dark_wood" => Some([94, 58, 36]),
        "sand" => Some([204, 174, 118]),
        "sandstone" => Some([192, 156, 104]),
        "marble" => Some([218, 216, 210]),
        "bronze" => Some([88, 156, 130]),
        "copper" => Some([178, 102, 68]),
        "neon_cyan" => Some([68, 242, 255]),
        "neon_pink" => Some([255, 76, 178]),
        "neon_yellow" => Some([255, 232, 88]),
        "ice" => Some([158, 212, 238]),
        "skin" => Some([224, 178, 144]),
        "stone" => Some([144, 150, 158]),
        "dark_stone" => Some([90, 96, 106]),
        "grass" => Some([76, 156, 68]),
        "water" => Some([44, 106, 178]),
        "lava" => Some([244, 100, 34]),
        _ => None,
    }
}

/// 依色票名稱推斷預設表面材質
pub fn default_material_for_color_name(name: &str) -> &'static str {
    match name.trim().to_ascii_lowercase().as_str() {
        "stone" | "dark_stone" | "sandstone" | "marble" | "sand" => "stone",
        "wood" | "dark_wood" | "brown" => "wood",
        "sky" | "jade" | "ice" | "water" => "glass",
        "gold" | "silver" | "bronze" | "copper" => "metal",
        "grass" | "sakura" => "foliage",
        _ => "flat",
    }
}

/// 5 階 Hue-Shifted 像素色階（deep / dark / mid / light / specular）
#[derive(Debug, Clone, Copy)]
pub struct ColorRamp {
    pub deep: Rgb,
    pub dark: Rgb,
    pub mid: Rgb,
    pub light: Rgb,
    pub specular: Rgb,
}

fn clamp_u8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

pub fn make_ramp(base: Rgb) -> ColorRamp {
    let r = base[0] as f32;
    let g = base[1] as f32;
    let b = base[2] as f32;
    ColorRamp {
        deep: [
            clamp_u8(r * 0.56 - 4.0),
            clamp_u8(g * 0.58 - 2.0),
            clamp_u8(b * 0.68 + 10.0),
        ],
        dark: [
            clamp_u8(r * 0.77 - 2.0),
            clamp_u8(g * 0.78),
            clamp_u8(b * 0.85 + 6.0),
        ],
        mid: base,
        light: [
            clamp_u8(r + (255.0 - r) * 0.22 + 5.0),
            clamp_u8(g + (255.0 - g) * 0.20 + 3.0),
            clamp_u8(b + (255.0 - b) * 0.12 - 3.0),
        ],
        specular: [
            clamp_u8(r + (255.0 - r) * 0.34 + 7.0),
            clamp_u8(g + (255.0 - g) * 0.31 + 5.0),
            clamp_u8(b + (255.0 - b) * 0.20 - 2.0),
        ],
    }
}

/// 烘焙診斷資訊
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackDiagnostics {
    pub slices: usize,
    pub slice_width: usize,
    pub slice_height: usize,
    pub voxels: usize,
    pub emissive_voxels: usize,
    pub thin_runs: Vec<[usize; 2]>,
    pub auto_thickened_voxels: usize,
    pub blink_free: bool,
}

/// 完成烘焙並準備寫入 SQLite 或回傳前端的 Sprite-Stack 資產結果
#[derive(Debug, Clone)]
pub struct BakedStack {
    pub title: String,
    pub category: String,
    pub slices: usize,
    pub slice_w: usize,
    pub slice_h: usize,
    pub slices_png: Vec<u8>,
    pub normals_png: Vec<u8>,
    pub emissive_png: Vec<u8>,
    pub blueprint_json: Option<String>,
    pub default_scale: f32,
    pub anim_preset: String,
    pub sha256: String,
    pub diagnostics: StackDiagnostics,
}

/// 3D 體素建模畫布
#[derive(Debug, Clone)]
pub struct VoxelCanvas {
    pub h: usize,
    pub l: usize,
    pub w: usize,
    pub vol: Vec<RgbaPx>,
    pub emissive: Vec<bool>,
    pub protected: Vec<bool>,
}

impl VoxelCanvas {
    pub fn new(height: usize, length: usize, width: usize) -> Self {
        let h = height.max(8);
        let l = length.max(8);
        let w = width.max(8);
        let len = h * l * w;
        Self {
            h,
            l,
            w,
            vol: vec![[0, 0, 0, 0]; len],
            emissive: vec![false; len],
            protected: vec![false; len],
        }
    }

    #[inline]
    pub fn cx(&self) -> f32 {
        (self.w as f32 - 1.0) * 0.5
    }

    #[inline]
    pub fn cy(&self) -> f32 {
        (self.l as f32 - 1.0) * 0.5
    }

    #[inline]
    pub fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.l + y) * self.w + x
    }

    #[inline]
    pub fn is_solid(&self, x: isize, y: isize, z: isize) -> bool {
        if x < 0 || y < 0 || z < 0 {
            return false;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= self.w || y >= self.l || z >= self.h {
            return false;
        }
        self.vol[self.idx(x, y, z)][3] > 0
    }

    pub fn voxel_count(&self) -> usize {
        self.vol.iter().filter(|px| px[3] > 0).count()
    }

    pub fn set_voxel(
        &mut self,
        x: isize,
        y: isize,
        z: isize,
        color: Rgb,
        emissive: bool,
        protected: Option<bool>,
    ) {
        if x < 0 || y < 0 || z < 0 {
            return;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= self.w || y >= self.l || z >= self.h {
            return;
        }
        let i = self.idx(x, y, z);
        self.vol[i] = [color[0], color[1], color[2], 255];
        self.emissive[i] = emissive;
        self.protected[i] = protected.unwrap_or(emissive);
    }

    pub fn set_voxel_named(
        &mut self,
        x: isize,
        y: isize,
        z: isize,
        color_name: &str,
        emissive: bool,
        protected: Option<bool>,
    ) {
        let rgb = resolve_palette_color(color_name).unwrap_or([136, 144, 158]);
        self.set_voxel(x, y, z, rgb, emissive, protected);
    }

    pub fn clear_voxel(&mut self, x: isize, y: isize, z: isize) {
        if x < 0 || y < 0 || z < 0 {
            return;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= self.w || y >= self.l || z >= self.h {
            return;
        }
        let i = self.idx(x, y, z);
        self.vol[i] = [0, 0, 0, 0];
        self.emissive[i] = false;
        self.protected[i] = false;
    }

    fn clip_bounds(
        &self,
        x0: isize,
        y0: isize,
        z0: isize,
        x1: isize,
        y1: isize,
        z1: isize,
    ) -> Option<(usize, usize, usize, usize, usize, usize)> {
        let xa = x0.min(x1).max(0) as usize;
        let xb = x0.max(x1).min(self.w as isize).max(0) as usize;
        let ya = y0.min(y1).max(0) as usize;
        let yb = y0.max(y1).min(self.l as isize).max(0) as usize;
        let za = z0.min(z1).max(0) as usize;
        let zb = z0.max(z1).min(self.h as isize).max(0) as usize;
        if xa >= xb || ya >= yb || za >= zb {
            None
        } else {
            Some((xa, ya, za, xb, yb, zb))
        }
    }

    /// 根據 3D 布林遮罩著色、計算頂/底/背緣色階並鋪設材質微肌理
    pub fn paint_mask(
        &mut self,
        mask: &[bool],
        color: Rgb,
        shaded: bool,
        emissive: bool,
        material: &str,
        protected: Option<bool>,
    ) {
        if mask.len() != self.vol.len() {
            return;
        }
        let mut z_min = usize::MAX;
        let mut z_max = 0usize;
        let mut y_min = usize::MAX;
        let mut y_max = 0usize;
        let mut any = false;

        for z in 0..self.h {
            for y in 0..self.l {
                let row_off = (z * self.l + y) * self.w;
                for x in 0..self.w {
                    if mask[row_off + x] {
                        any = true;
                        z_min = z_min.min(z);
                        z_max = z_max.max(z);
                        y_min = y_min.min(y);
                        y_max = y_max.max(y);
                    }
                }
            }
        }
        if !any {
            return;
        }

        if !shaded || emissive {
            let prot = protected.unwrap_or(true);
            for (i, &m) in mask.iter().enumerate() {
                if m {
                    self.vol[i] = [color[0], color[1], color[2], 255];
                    self.emissive[i] = emissive;
                    self.protected[i] = prot;
                }
            }
            return;
        }

        let ramp = make_ramp(color);
        let prot = protected.unwrap_or(false);
        let z_band_limit = if z_max > z_min {
            z_min + (z_max - z_min) / 5
        } else {
            0
        };
        let mat = material.trim().to_ascii_lowercase();

        for z in z_min..=z_max {
            for y in y_min..=y_max {
                for x in 0..self.w {
                    let i = self.idx(x, y, z);
                    if !mask[i] {
                        continue;
                    }
                    let top_exposed = z + 1 >= self.h || !mask[self.idx(x, y, z + 1)];
                    let mut c_rgb = if top_exposed {
                        ramp.light
                    } else if (z_max > z_min && z <= z_band_limit) || (y_max > y_min && y == y_min) {
                        ramp.dark
                    } else {
                        ramp.mid
                    };

                    // 套用 3D 程序化材質微肌理
                    if mat != "flat" {
                        let side_area = !top_exposed;
                        let xi = x as isize;
                        let yi = y as isize;
                        let zi = z as isize;
                        let h_hash = ((xi * 37 + yi * 73 + zi * 19)
                            ^ (xi * 13 - yi * 29 + zi * 43))
                            & 15;
                        match mat.as_str() {
                            "stone" => {
                                let stagger = (z / 2) & 1;
                                let mortar_h = z % 3 == 0;
                                let mortar_v = (x + y + stagger * 2) % 4 == 0;
                                if (mortar_h || mortar_v) && h_hash < 10 {
                                    c_rgb = ramp.dark;
                                } else if h_hash == 2 {
                                    c_rgb = ramp.light;
                                }
                            }
                            "wood" => {
                                let grain = (x * 3 + y * 5 + (z / 2)) & 7;
                                if grain == 0 {
                                    c_rgb = ramp.dark;
                                } else if grain == 4 {
                                    c_rgb = ramp.light;
                                }
                            }
                            "glass" => {
                                let mullion = side_area && (z % 3 == 0 || (x + y) % 4 == 0);
                                let diag = (xi - yi + zi * 2).rem_euclid(9);
                                if mullion {
                                    c_rgb = ramp.deep;
                                } else if diag == 0 || diag == 1 {
                                    c_rgb = ramp.specular;
                                }
                            }
                            "metal" => {
                                let sheen = (xi + yi * 2 - zi).rem_euclid(7) == 0;
                                if sheen {
                                    c_rgb = ramp.specular;
                                } else if side_area && h_hash < 3 {
                                    c_rgb = ramp.dark;
                                }
                            }
                            "fur" | "mane" => {
                                let tuft = ((x / 2) * 11 + (y / 2) * 17 + z * 23) & 7;
                                if tuft <= 1 {
                                    c_rgb = ramp.dark;
                                } else if tuft == 6 {
                                    c_rgb = ramp.light;
                                }
                            }
                            "foliage" => {
                                let leaf_cluster =
                                    ((x / 2) * 19 + (y / 2) * 31 + (z / 2) * 13) & 7;
                                if leaf_cluster <= 1 {
                                    c_rgb = ramp.dark;
                                } else if leaf_cluster >= 6 {
                                    c_rgb = ramp.light;
                                }
                            }
                            "fabric" => {
                                if side_area {
                                    let fold = (x + y) % 4;
                                    if fold == 0 {
                                        c_rgb = ramp.dark;
                                    } else if fold == 2 {
                                        c_rgb = ramp.light;
                                    }
                                }
                            }
                            "crystal" => {
                                let facet = (xi - yi + zi) & 3;
                                if facet == 0 {
                                    c_rgb = ramp.deep;
                                } else if facet == 2 {
                                    c_rgb = ramp.specular;
                                }
                            }
                            _ => {}
                        }
                    }

                    self.vol[i] = [c_rgb[0], c_rgb[1], c_rgb[2], 255];
                    self.emissive[i] = false;
                    self.protected[i] = prot;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_box(
        &mut self,
        x0: isize,
        y0: isize,
        z0: isize,
        x1: isize,
        y1: isize,
        z1: isize,
        color: Rgb,
        shaded: bool,
        emissive: bool,
        material: &str,
        protected: Option<bool>,
    ) {
        let Some((xa, ya, za, xb, yb, zb)) = self.clip_bounds(x0, y0, z0, x1, y1, z1) else {
            return;
        };
        let mut mask = vec![false; self.vol.len()];
        for z in za..zb {
            for y in ya..yb {
                let row = (z * self.l + y) * self.w;
                for x in xa..xb {
                    mask[row + x] = true;
                }
            }
        }
        self.paint_mask(&mask, color, shaded, emissive, material, protected);
    }

    pub fn clear_box(
        &mut self,
        x0: isize,
        y0: isize,
        z0: isize,
        x1: isize,
        y1: isize,
        z1: isize,
    ) {
        let Some((xa, ya, za, xb, yb, zb)) = self.clip_bounds(x0, y0, z0, x1, y1, z1) else {
            return;
        };
        for z in za..zb {
            for y in ya..yb {
                let row = (z * self.l + y) * self.w;
                for x in xa..xb {
                    let i = row + x;
                    self.vol[i] = [0, 0, 0, 0];
                    self.emissive[i] = false;
                    self.protected[i] = false;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_cylinder(
        &mut self,
        cx: f32,
        cy: f32,
        z0: isize,
        z1: isize,
        rx: f32,
        ry: f32,
        color: Rgb,
        hollow: bool,
        wall: f32,
        shaded: bool,
        emissive: bool,
        material: &str,
    ) {
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let za = z0.min(z1).max(0) as usize;
        let zb = z0.max(z1).min(self.h as isize).max(0) as usize;
        if za >= zb {
            return;
        }
        let irx = (rx - wall).max(0.5);
        let iry = (ry - wall).max(0.5);
        let mut mask = vec![false; self.vol.len()];
        let mut any = false;
        for y in 0..self.l {
            let dy = (y as f32 - cy) / ry;
            let idy = (y as f32 - cy) / iry;
            for x in 0..self.w {
                let dx = (x as f32 - cx) / rx;
                if dx * dx + dy * dy <= 1.0 {
                    if hollow {
                        let idx = (x as f32 - cx) / irx;
                        if idx * idx + idy * idy < 1.0 {
                            continue;
                        }
                    }
                    any = true;
                    for z in za..zb {
                        mask[(z * self.l + y) * self.w + x] = true;
                    }
                }
            }
        }
        if any {
            self.paint_mask(&mask, color, shaded, emissive, material, None);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_cone(
        &mut self,
        cx: f32,
        cy: f32,
        z0: isize,
        z1: isize,
        rx0: f32,
        ry0: f32,
        rx1: f32,
        ry1: f32,
        color: Rgb,
        hollow: bool,
        wall: f32,
        shaded: bool,
        emissive: bool,
        material: &str,
    ) {
        let za = z0.min(z1).max(0) as usize;
        let zb = z0.max(z1).min(self.h as isize).max(0) as usize;
        if za >= zb {
            return;
        }
        let span = (zb - za).saturating_sub(1).max(1) as f32;
        let mut mask = vec![false; self.vol.len()];
        for z in za..zb {
            let t = (z - za) as f32 / span;
            let rx = (rx0 + (rx1 - rx0) * t).max(0.5);
            let ry = (ry0 + (ry1 - ry0) * t).max(0.5);
            for y in 0..self.l {
                let dy = (y as f32 - cy) / ry;
                for x in 0..self.w {
                    let dx = (x as f32 - cx) / rx;
                    if dx * dx + dy * dy <= 1.0 {
                        if hollow && rx > wall && ry > wall {
                            let idx = (x as f32 - cx) / (rx - wall);
                            let idy = (y as f32 - cy) / (ry - wall);
                            if idx * idx + idy * idy < 1.0 {
                                continue;
                            }
                        }
                        mask[(z * self.l + y) * self.w + x] = true;
                    }
                }
            }
        }
        self.paint_mask(&mask, color, shaded, emissive, material, None);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_sphere(
        &mut self,
        cx: f32,
        cy: f32,
        cz: f32,
        rx: f32,
        ry: f32,
        rz: f32,
        color: Rgb,
        shaded: bool,
        emissive: bool,
        material: &str,
    ) {
        if rx <= 0.0 || ry <= 0.0 || rz <= 0.0 {
            return;
        }
        let mut mask = vec![false; self.vol.len()];
        for z in 0..self.h {
            let dz = (z as f32 - cz) / rz;
            let dz2 = dz * dz;
            if dz2 > 1.0 {
                continue;
            }
            for y in 0..self.l {
                let dy = (y as f32 - cy) / ry;
                let dzy2 = dz2 + dy * dy;
                if dzy2 > 1.0 {
                    continue;
                }
                for x in 0..self.w {
                    let dx = (x as f32 - cx) / rx;
                    if dzy2 + dx * dx <= 1.0 {
                        mask[(z * self.l + y) * self.w + x] = true;
                    }
                }
            }
        }
        self.paint_mask(&mask, color, shaded, emissive, material, None);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_pyramid(
        &mut self,
        cx: f32,
        cy: f32,
        z0: isize,
        z1: isize,
        w0: f32,
        l0: f32,
        w1: f32,
        l1: f32,
        color: Rgb,
        stepped: bool,
        steps: usize,
        shaded: bool,
        emissive: bool,
        material: &str,
    ) {
        let za = z0.min(z1).max(0) as usize;
        let zb = z0.max(z1).min(self.h as isize).max(0) as usize;
        if za >= zb {
            return;
        }
        let span = (zb - za).saturating_sub(1).max(1) as f32;
        let mut mask = vec![false; self.vol.len()];
        for z in za..zb {
            let mut t = (z - za) as f32 / span;
            if stepped && steps > 1 {
                t = (t * steps as f32).floor() / steps as f32;
            }
            let hw = ((w0 + (w1 - w0) * t) * 0.5).max(0.5);
            let hl = ((l0 + (l1 - l0) * t) * 0.5).max(0.5);
            for y in 0..self.l {
                if (y as f32 - cy).abs() > hl {
                    continue;
                }
                for x in 0..self.w {
                    if (x as f32 - cx).abs() <= hw {
                        mask[(z * self.l + y) * self.w + x] = true;
                    }
                }
            }
        }
        self.paint_mask(&mask, color, shaded, emissive, material, None);
    }

    pub fn fill_arch_cutout(
        &mut self,
        cx: f32,
        cy: f32,
        z0: isize,
        z1: isize,
        half_span: f32,
        depth: f32,
        axis: &str,
    ) {
        let za = z0.max(0) as usize;
        let zb = z1.min(self.h as isize).max(0) as usize;
        if za >= zb || half_span <= 0.0 {
            return;
        }
        let arch_z_start = zb.saturating_sub(half_span.round() as usize).max(za);
        for z in za..zb {
            let curr_half = if z < arch_z_start {
                half_span
            } else {
                let dz = (z - arch_z_start) as f32 / (zb - arch_z_start).max(1) as f32;
                half_span * (1.0 - dz * dz).max(0.0).sqrt()
            };
            if curr_half < 0.5 {
                continue;
            }
            for y in 0..self.l {
                for x in 0..self.w {
                    let cut = if axis == "y" {
                        (x as f32 - cx).abs() <= curr_half && (y as f32 - cy).abs() <= depth
                    } else {
                        (y as f32 - cy).abs() <= curr_half && (x as f32 - cx).abs() <= depth
                    };
                    if cut {
                        let i = self.idx(x, y, z);
                        self.vol[i] = [0, 0, 0, 0];
                        self.emissive[i] = false;
                        self.protected[i] = false;
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_line(
        &mut self,
        p0: [f32; 3],
        p1: [f32; 3],
        radius: f32,
        color: Rgb,
        shaded: bool,
        emissive: bool,
        material: &str,
    ) {
        let dist = ((p1[0] - p0[0]).powi(2) + (p1[1] - p0[1]).powi(2) + (p1[2] - p0[2]).powi(2)).sqrt();
        let steps = (dist * 2.2).ceil().max(2.0) as usize;
        let r = radius.max(0.6);
        let r2 = r * r;
        let mut mask = vec![false; self.vol.len()];
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let px = p0[0] + (p1[0] - p0[0]) * t;
            let py = p0[1] + (p1[1] - p0[1]) * t;
            let pz = p0[2] + (p1[2] - p0[2]) * t;
            let x0 = (px - r).floor().max(0.0) as usize;
            let x1 = ((px + r).ceil() as usize + 1).min(self.w);
            let y0 = (py - r).floor().max(0.0) as usize;
            let y1 = ((py + r).ceil() as usize + 1).min(self.l);
            let z0 = (pz - r).floor().max(0.0) as usize;
            let z1 = ((pz + r).ceil() as usize + 1).min(self.h);
            for z in z0..z1 {
                for y in y0..y1 {
                    for x in x0..x1 {
                        let d2 = (x as f32 - px).powi(2)
                            + (y as f32 - py).powi(2)
                            + (z as f32 - pz).powi(2);
                        if d2 <= r2 {
                            mask[self.idx(x, y, z)] = true;
                        }
                    }
                }
            }
        }
        self.paint_mask(&mask, color, shaded, emissive, material, None);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_curved_roof(
        &mut self,
        cx: f32,
        cy: f32,
        z0: isize,
        z_height: usize,
        half_w: f32,
        half_l: f32,
        color: Rgb,
        flare: f32,
        emissive: bool,
        material: &str,
    ) {
        if z_height == 0 {
            return;
        }
        let mut mask = vec![false; self.vol.len()];
        for dz in 0..z_height {
            let zi = z0 + dz as isize;
            if zi < 0 || zi as usize >= self.h {
                continue;
            }
            let z = zi as usize;
            let t = dz as f32 / z_height.saturating_sub(1).max(1) as f32;
            let shrink = 1.0 - t * 0.72;
            let hw = (half_w * shrink).max(1.0);
            let hl = (half_l * shrink).max(1.0);
            for y in 0..self.l {
                let ny = (y as f32 - cy).abs() / hl;
                for x in 0..self.w {
                    let nx = (x as f32 - cx).abs() / hw;
                    let corner = nx.max(ny);
                    let diag_boost = flare * (1.0 - t) * nx.min(ny).powi(2);
                    if corner <= 1.0 + diag_boost {
                        mask[self.idx(x, y, z)] = true;
                    }
                }
            }
        }
        self.paint_mask(&mask, color, true, emissive, material, None);
    }

    /// 於向上的裸露表面鋪設自然積雪
    pub fn apply_snow_cap(&mut self, seed: u64) {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut snow_indices = Vec::new();
        for z in 0..self.h {
            for y in 0..self.l {
                for x in 0..self.w {
                    let i = self.idx(x, y, z);
                    if self.vol[i][3] == 0 || self.emissive[i] {
                        continue;
                    }
                    let above_solid = z + 1 < self.h && self.vol[self.idx(x, y, z + 1)][3] > 0;
                    if above_solid {
                        continue;
                    }
                    let prob: f32 = rng.gen();
                    if prob > 0.12 && (z > 0 || rng.gen::<f32>() > 0.65) {
                        snow_indices.push(i);
                    }
                }
            }
        }
        for i in snow_indices {
            self.vol[i][0] = 232;
            self.vol[i][1] = 240;
            self.vol[i][2] = 248;
            self.protected[i] = true;
        }
    }

    /// 於表面添加自然青苔斑點
    pub fn apply_moss(&mut self, seed: u64) {
        let mut rng = StdRng::seed_from_u64(seed);
        let greens = [[66u8, 146, 68], [52, 126, 58], [86, 164, 74]];
        for i in 0..self.vol.len() {
            if self.vol[i][3] > 0 && !self.emissive[i] && !self.protected[i] && rng.gen::<f32>() > 0.84 {
                let g = greens[rng.gen_range(0..greens.len())];
                self.vol[i][0] = g[0];
                self.vol[i][1] = g[1];
                self.vol[i][2] = g[2];
            }
        }
    }

    /// 3D 環境光遮蔽（AO）、凹槽冷陰影與外凸邊緣暖高光烘焙
    pub fn polish_surface(&mut self, ao_strength: f32, edge_highlight: f32) {
        let mut new_vol = self.vol.clone();
        let occ = |x: isize, y: isize, z: isize| -> f32 {
            if self.is_solid(x, y, z) {
                1.0
            } else {
                0.0
            }
        };

        for z in 0..self.h {
            let z_frac = z as f32 / (self.h.saturating_sub(1).max(1)) as f32;
            let height_mul = 0.94 + 0.08 * z_frac;
            let zi = z as isize;
            for y in 0..self.l {
                let yi = y as isize;
                for x in 0..self.w {
                    let i = self.idx(x, y, z);
                    if self.vol[i][3] == 0 || self.emissive[i] || self.protected[i] {
                        continue;
                    }
                    let xi = x as isize;
                    let up_c = occ(xi, yi, zi + 1);
                    let side_xm = occ(xi - 1, yi, zi);
                    let side_xp = occ(xi + 1, yi, zi);
                    let side_ym = occ(xi, yi - 1, zi);
                    let side_yp = occ(xi, yi + 1, zi);

                    let surface_exposed = up_c < 0.5
                        || side_xm < 0.5
                        || side_xp < 0.5
                        || side_ym < 0.5
                        || side_yp < 0.5;
                    if !surface_exposed {
                        continue;
                    }

                    let upper_occ = up_c * 2.0
                        + occ(xi, yi - 1, zi + 1)
                        + occ(xi, yi + 1, zi + 1)
                        + occ(xi - 1, yi, zi + 1)
                        + occ(xi + 1, yi, zi + 1)
                        + (occ(xi - 1, yi - 1, zi + 1)
                            + occ(xi + 1, yi - 1, zi + 1)
                            + occ(xi - 1, yi + 1, zi + 1)
                            + occ(xi + 1, yi + 1, zi + 1))
                            * 0.5;
                    let side_occ = side_xm + side_xp + side_ym + side_yp;
                    let raw_occ = (upper_occ / 8.0) * 0.72 + (side_occ / 4.0) * 0.28;
                    let ao_term = (raw_occ - 0.18).clamp(0.0, 1.0);
                    let ao_mul = 1.0 - ao_term * ao_strength;
                    let mut mul_r = height_mul * ao_mul;
                    let mut mul_g = height_mul * ao_mul;
                    let mut mul_b = height_mul * (1.0 - ao_term * (ao_strength * 0.65));

                    let open_sides = (1.0 - side_xm)
                        + (1.0 - side_xp)
                        + (1.0 - side_ym)
                        + (1.0 - side_yp);
                    if edge_highlight > 0.0 && up_c < 0.5 && open_sides >= 2.0 {
                        mul_r *= 1.0 + edge_highlight;
                        mul_g *= 1.0 + edge_highlight * 0.9;
                        mul_b *= 1.0 + edge_highlight * 0.55;
                    }

                    new_vol[i][0] = (self.vol[i][0] as f32 * mul_r).round().clamp(0.0, 245.0) as u8;
                    new_vol[i][1] = (self.vol[i][1] as f32 * mul_g).round().clamp(0.0, 245.0) as u8;
                    new_vol[i][2] = (self.vol[i][2] as f32 * mul_b).round().clamp(0.0, 245.0) as u8;
                }
            }
        }
        self.vol = new_vol;
    }

    /// 偵測連續多層包含無 2x2 實心塊支持的孤立 1px 細柱區間
    pub fn thin_runs(&self, min_run: usize) -> Vec<[usize; 2]> {
        let mut thin = vec![false; self.h];
        let lw = self.l * self.w;
        for z in 0..self.h {
            let mut block = vec![false; lw];
            for y in 0..self.l.saturating_sub(1) {
                for x in 0..self.w.saturating_sub(1) {
                    let s00 = self.vol[self.idx(x, y, z)][3] > 0;
                    let s10 = self.vol[self.idx(x + 1, y, z)][3] > 0;
                    let s01 = self.vol[self.idx(x, y + 1, z)][3] > 0;
                    let s11 = self.vol[self.idx(x + 1, y + 1, z)][3] > 0;
                    if s00 && s10 && s01 && s11 {
                        block[y * self.w + x] = true;
                        block[y * self.w + x + 1] = true;
                        block[(y + 1) * self.w + x] = true;
                        block[(y + 1) * self.w + x + 1] = true;
                    }
                }
            }
            let mut seen = vec![false; lw];
            let mut found_thin = false;
            for y0 in 0..self.l {
                if found_thin {
                    break;
                }
                for x0 in 0..self.w {
                    let p0 = y0 * self.w + x0;
                    if seen[p0] || self.vol[self.idx(x0, y0, z)][3] == 0 {
                        continue;
                    }
                    let mut stack = vec![(y0, x0)];
                    seen[p0] = true;
                    let mut has_block = false;
                    while let Some((y, x)) = stack.pop() {
                        if block[y * self.w + x] {
                            has_block = true;
                        }
                        for (ny, nx) in [
                            (y.wrapping_add(1), x),
                            (y.wrapping_sub(1), x),
                            (y, x.wrapping_add(1)),
                            (y, x.wrapping_sub(1)),
                        ] {
                            if ny < self.l && nx < self.w {
                                let np = ny * self.w + nx;
                                if !seen[np] && self.vol[self.idx(nx, ny, z)][3] > 0 {
                                    seen[np] = true;
                                    stack.push((ny, nx));
                                }
                            }
                        }
                    }
                    if !has_block {
                        found_thin = true;
                        break;
                    }
                }
            }
            thin[z] = found_thin;
        }

        let mut runs = Vec::new();
        let mut start: Option<usize> = None;
        for (z, &t) in thin.iter().chain(std::iter::once(&false)).enumerate() {
            if t && start.is_none() {
                start = Some(z);
            } else if !t {
                if let Some(s) = start {
                    if z - s >= min_run {
                        runs.push([s, z - 1]);
                    }
                    start = None;
                }
            }
        }
        runs
    }

    /// 自動將連續 1px 細柱補厚為 2x2 塊，避免旋轉時像素閃爍
    pub fn auto_thicken_thin_runs(&mut self) -> usize {
        let runs = self.thin_runs(2);
        if runs.is_empty() {
            return 0;
        }
        let mut fixed = 0usize;
        let lw = self.l * self.w;
        for [z0, z1] in runs {
            for z in z0..=z1 {
                let mut block = vec![false; lw];
                for y in 0..self.l.saturating_sub(1) {
                    for x in 0..self.w.saturating_sub(1) {
                        let s00 = self.vol[self.idx(x, y, z)][3] > 0;
                        let s10 = self.vol[self.idx(x + 1, y, z)][3] > 0;
                        let s01 = self.vol[self.idx(x, y + 1, z)][3] > 0;
                        let s11 = self.vol[self.idx(x + 1, y + 1, z)][3] > 0;
                        if s00 && s10 && s01 && s11 {
                            block[y * self.w + x] = true;
                            block[y * self.w + x + 1] = true;
                            block[(y + 1) * self.w + x] = true;
                            block[(y + 1) * self.w + x + 1] = true;
                        }
                    }
                }
                let mut iso = Vec::new();
                for y in 0..self.l {
                    for x in 0..self.w {
                        if self.vol[self.idx(x, y, z)][3] > 0 && !block[y * self.w + x] {
                            iso.push((y, x));
                        }
                    }
                }
                for (y, x) in iso {
                    let src_idx = self.idx(x, y, z);
                    let col = self.vol[src_idx];
                    let em = self.emissive[src_idx];
                    let prot = self.protected[src_idx];
                    let nx = if x + 1 < self.w { x + 1 } else { x.saturating_sub(1) };
                    let ny = if y + 1 < self.l { y + 1 } else { y.saturating_sub(1) };
                    for (ty, tx) in [(y, nx), (ny, x), (ny, nx)] {
                        if ty < self.l && tx < self.w {
                            let ti = self.idx(tx, ty, z);
                            if self.vol[ti][3] == 0 {
                                self.vol[ti] = col;
                                self.emissive[ti] = em;
                                self.protected[ti] = prot;
                                fixed += 1;
                            }
                        }
                    }
                }
            }
        }
        fixed
    }

    /// 裁切多餘空白邊界並將物件水平置中於偶數尺寸畫布
    pub fn trim_and_center(&mut self, pad: usize, target_slices: Option<usize>) {
        let mut z_min = usize::MAX;
        let mut z_max = 0usize;
        let mut y_min = usize::MAX;
        let mut y_max = 0usize;
        let mut x_min = usize::MAX;
        let mut x_max = 0usize;
        let mut any = false;

        for z in 0..self.h {
            for y in 0..self.l {
                for x in 0..self.w {
                    if self.vol[self.idx(x, y, z)][3] > 0 {
                        any = true;
                        z_min = z_min.min(z);
                        z_max = z_max.max(z);
                        y_min = y_min.min(y);
                        y_max = y_max.max(y);
                        x_min = x_min.min(x);
                        x_max = x_max.max(x);
                    }
                }
            }
        }
        if !any {
            return;
        }

        let ch = z_max - z_min + 1;
        let cl = y_max - y_min + 1;
        let cw = x_max - x_min + 1;
        let new_h = target_slices.map_or(ch, |t| t.max(ch));
        let mut new_l = cl + pad * 2;
        let mut new_w = cw + pad * 2;
        if new_l % 2 == 1 {
            new_l += 1;
        }
        if new_w % 2 == 1 {
            new_w += 1;
        }
        let oy = (new_l - cl) / 2;
        let ox = (new_w - cw) / 2;

        let mut next = VoxelCanvas::new(new_h, new_l, new_w);
        for z in 0..ch {
            for y in 0..cl {
                for x in 0..cw {
                    let si = self.idx(x_min + x, y_min + y, z_min + z);
                    let di = next.idx(ox + x, oy + y, z);
                    next.vol[di] = self.vol[si];
                    next.emissive[di] = self.emissive[si];
                    next.protected[di] = self.protected[si];
                }
            }
        }
        *self = next;
    }

    /// 估計每個實心體素的 3D 外法線方向，編碼為 RGBA uint8（0..255）
    pub fn compute_normals(&self) -> Vec<RgbaPx> {
        let occ = |x: isize, y: isize, z: isize| -> f32 {
            if self.is_solid(x, y, z) {
                1.0
            } else {
                0.0
            }
        };
        let mut out = vec![[0, 0, 0, 0]; self.vol.len()];
        for z in 0..self.h {
            let zi = z as isize;
            for y in 0..self.l {
                let yi = y as isize;
                for x in 0..self.w {
                    let i = self.idx(x, y, z);
                    if self.vol[i][3] == 0 {
                        continue;
                    }
                    let xi = x as isize;

                    // 3D 3x3x3 Sobel 濾波梯度
                    let mut nx = 0.0f32;
                    let mut ny = 0.0f32;
                    let mut nz = 0.0f32;
                    for dz in -1..=1 {
                        let wz = if dz == 0 { 2.0 } else { 1.0 };
                        for dy in -1..=1 {
                            let wy = if dy == 0 { 2.0 } else { 1.0 };
                            nx += wy * wz * (occ(xi - 1, yi + dy, zi + dz) - occ(xi + 1, yi + dy, zi + dz));
                        }
                    }
                    for dz in -1..=1 {
                        let wz = if dz == 0 { 2.0 } else { 1.0 };
                        for dx in -1..=1 {
                            let wx = if dx == 0 { 2.0 } else { 1.0 };
                            ny += wx * wz * (occ(xi + dx, yi - 1, zi + dz) - occ(xi + dx, yi + 1, zi + dz));
                        }
                    }
                    for dy in -1..=1 {
                        let wy = if dy == 0 { 2.0 } else { 1.0 };
                        for dx in -1..=1 {
                            let wx = if dx == 0 { 2.0 } else { 1.0 };
                            nz += wx * wy * (occ(xi + dx, yi + dy, zi - 1) - occ(xi + dx, yi + dy, zi + 1));
                        }
                    }

                    let mag = (nx * nx + ny * ny + nz * nz).sqrt();
                    if mag < 1e-5 {
                        nx = 0.0;
                        ny = 0.0;
                        nz = 1.0;
                    } else {
                        nx /= mag;
                        ny /= mag;
                        nz /= mag;
                    }
                    out[i] = [
                        ((nx * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8,
                        ((ny * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8,
                        ((nz * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8,
                        255,
                    ];
                }
            }
        }
        out
    }

    /// 偵測夜間自發光體素：整合顯式 `emissive` 標記與小面積高亮度/高飽和表面亮點
    pub fn detect_emissive(&self) -> Vec<RgbaPx> {
        let mut out = vec![[0, 0, 0, 0]; self.vol.len()];
        let mut surf_counts: HashMap<u32, usize> = HashMap::new();
        let mut surf_total = 0usize;
        let mut surface_flags = vec![false; self.vol.len()];

        for z in 0..self.h {
            let zi = z as isize;
            for y in 0..self.l {
                let yi = y as isize;
                for x in 0..self.w {
                    let i = self.idx(x, y, z);
                    let px = self.vol[i];
                    if px[3] == 0 {
                        continue;
                    }
                    if self.emissive[i] {
                        out[i] = px;
                        continue;
                    }
                    let xi = x as isize;
                    let enclosed = self.is_solid(xi, yi, zi - 1)
                        && self.is_solid(xi, yi, zi + 1)
                        && self.is_solid(xi, yi - 1, zi)
                        && self.is_solid(xi, yi + 1, zi)
                        && self.is_solid(xi - 1, yi, zi)
                        && self.is_solid(xi + 1, yi, zi);
                    if !enclosed {
                        surface_flags[i] = true;
                        surf_total += 1;
                        let key = ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | (px[2] as u32);
                        *surf_counts.entry(key).or_insert(0) += 1;
                    }
                }
            }
        }

        if surf_total == 0 {
            return out;
        }
        let dominant: HashSet<u32> = surf_counts
            .into_iter()
            .filter(|&(_, cnt)| (cnt as f32 / surf_total as f32) > 0.14)
            .map(|(k, _)| k)
            .collect();

        for (i, &is_surf) in surface_flags.iter().enumerate() {
            if !is_surf {
                continue;
            }
            let px = self.vol[i];
            let r = px[0] as f32;
            let g = px[1] as f32;
            let b = px[2] as f32;
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            let max_c = r.max(g).max(b);
            let min_c = r.min(g).min(b);
            let sat = (max_c - min_c) / max_c.max(1.0);
            let cand = ((luma >= 180.0) && (sat >= 0.30 || luma >= 232.0))
                || (max_c >= 215.0 && sat >= 0.62 && luma >= 110.0);
            if cand {
                let key = ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | (px[2] as u32);
                if !dominant.contains(&key) {
                    out[i] = px;
                }
            }
        }
        out
    }

    /// 將體素陣列編碼為水平拼接之切片 PNG 圖條（寬 = W * H，高 = L）
    pub fn encode_strip_png(&self, data: &[RgbaPx]) -> Result<Vec<u8>, String> {
        let strip_w = (self.w * self.h) as u32;
        let strip_h = self.l as u32;
        let mut img = RgbaImage::new(strip_w, strip_h);
        for z in 0..self.h {
            let x_off = (z * self.w) as u32;
            for y in 0..self.l {
                for x in 0..self.w {
                    let px = data[self.idx(x, y, z)];
                    img.put_pixel(x_off + x as u32, y as u32, Rgba(px));
                }
            }
        }
        let mut buf = Cursor::new(Vec::new());
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        encoder
            .write_image(img.as_raw(), strip_w, strip_h, image::ExtendedColorType::Rgba8)
            .map_err(|e| format!("PNG 編碼失敗：{e}"))?;
        Ok(buf.into_inner())
    }

    /// 執行完整後處理與貼圖烘焙，產出可直接存入 SQLite 與傳給 WebGL2 的套件
    pub fn bake(
        mut self,
        title: impl Into<String>,
        category: impl Into<String>,
        blueprint_json: Option<String>,
        default_scale: f32,
        anim_preset: impl Into<String>,
        polish: bool,
        auto_thicken: bool,
    ) -> Result<BakedStack, String> {
        if polish {
            self.polish_surface(0.26, 0.14);
        }
        let auto_thickened = if auto_thicken {
            self.auto_thicken_thin_runs()
        } else {
            0
        };
        self.trim_and_center(2, None);

        let normals = self.compute_normals();
        let emissive = self.detect_emissive();
        let em_count = emissive.iter().filter(|px| px[3] > 0).count();
        let runs = self.thin_runs(3);
        let voxels = self.voxel_count();

        let slices_png = self.encode_strip_png(&self.vol)?;
        let normals_png = self.encode_strip_png(&normals)?;
        let emissive_png = self.encode_strip_png(&emissive)?;

        let mut hasher = Sha256::new();
        hasher.update(&slices_png);
        let sha256 = format!("{:x}", hasher.finalize());

        Ok(BakedStack {
            title: title.into(),
            category: category.into(),
            slices: self.h,
            slice_w: self.w,
            slice_h: self.l,
            slices_png,
            normals_png,
            emissive_png,
            blueprint_json,
            default_scale,
            anim_preset: anim_preset.into(),
            sha256,
            diagnostics: StackDiagnostics {
                slices: self.h,
                slice_width: self.w,
                slice_height: self.l,
                voxels,
                emissive_voxels: em_count,
                blink_free: runs.is_empty(),
                thin_runs: runs,
                auto_thickened_voxels: auto_thickened,
            },
        })
    }
}

/// 從匯入的 `slices.png` 與切片數反推法線與自發光遮罩（支援外部匯入 `.zip` / PNG）
pub fn bake_from_strip_bytes(
    title: impl Into<String>,
    category: impl Into<String>,
    slices_png: &[u8],
    slice_count: usize,
    normals_png_opt: Option<&[u8]>,
    emissive_png_opt: Option<&[u8]>,
    blueprint_json: Option<String>,
    default_scale: f32,
    anim_preset: impl Into<String>,
) -> Result<BakedStack, String> {
    let img = image::load_from_memory(slices_png)
        .map_err(|e| format!("無法解析切片 PNG：{e}"))?
        .to_rgba8();
    let total_w = img.width() as usize;
    let slice_h = img.height() as usize;
    let slices = slice_count.max(1);
    if total_w % slices != 0 || total_w == 0 || slice_h == 0 {
        return Err(format!(
            "切片圖寬度 ({total_w}) 無法整除切片數 ({slices})，請檢查 metadata 或切片數設定"
        ));
    }
    let slice_w = total_w / slices;

    let mut canvas = VoxelCanvas::new(slices, slice_h, slice_w);
    for z in 0..slices {
        let x_off = z * slice_w;
        for y in 0..slice_h {
            for x in 0..slice_w {
                let p = img.get_pixel((x_off + x) as u32, y as u32).0;
                let idx = canvas.idx(x, y, z);
                canvas.vol[idx] = p;
            }
        }
    }

    let normals_png = match normals_png_opt {
        Some(b) if !b.is_empty() => b.to_vec(),
        _ => {
            let norms = canvas.compute_normals();
            canvas.encode_strip_png(&norms)?
        }
    };

    let (emissive_png, em_count) = match emissive_png_opt {
        Some(b) if !b.is_empty() => {
            let e_img = image::load_from_memory(b)
                .map_err(|e| format!("無法解析自發光遮罩 PNG：{e}"))?
                .to_rgba8();
            let cnt = e_img.pixels().filter(|p| p.0[3] > 0).count();
            (b.to_vec(), cnt)
        }
        _ => {
            let em = canvas.detect_emissive();
            let cnt = em.iter().filter(|p| p[3] > 0).count();
            (canvas.encode_strip_png(&em)?, cnt)
        }
    };

    let runs = canvas.thin_runs(3);
    let voxels = canvas.voxel_count();
    let re_slices_png = canvas.encode_strip_png(&canvas.vol)?;
    let mut hasher = Sha256::new();
    hasher.update(&re_slices_png);
    let sha256 = format!("{:x}", hasher.finalize());

    Ok(BakedStack {
        title: title.into(),
        category: category.into(),
        slices,
        slice_w,
        slice_h,
        slices_png: re_slices_png,
        normals_png,
        emissive_png,
        blueprint_json,
        default_scale,
        anim_preset: anim_preset.into(),
        sha256,
        diagnostics: StackDiagnostics {
            slices,
            slice_width: slice_w,
            slice_height: slice_h,
            voxels,
            emissive_voxels: em_count,
            blink_free: runs.is_empty(),
            thin_runs: runs,
            auto_thickened_voxels: 0,
        },
    })
}

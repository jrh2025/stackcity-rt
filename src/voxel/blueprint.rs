//! 純資料驅動的 Sprite-Stacking 3D 體素藍圖渲染器（100% 相容既有關鍵 JSON 格式）。
//!
//! 支援 `box`、`clear_box`、`cylinder`、`cone`、`sphere`、`pyramid`、`arch_cut` 與 `line`，
//! 並完整支援 `mirror_x` 對稱、`emissive` 夜間自發光與程序化 `material` 肌理。

use super::{default_material_for_color_name, resolve_palette_color, BakedStack, Rgb, VoxelCanvas};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// 3D 幾何藍圖 JSON 頂層結構
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blueprint {
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_size")]
    pub size: [f32; 3],
    #[serde(default)]
    pub palette: HashMap<String, Value>,
    #[serde(default)]
    pub primitives: Vec<Value>,
}

fn default_title() -> String {
    "自訂 3D 物件".to_string()
}

fn default_size() -> [f32; 3] {
    [42.0, 42.0, 48.0]
}

fn parse_rgb_value(val: &Value, palette: &HashMap<String, Rgb>) -> Rgb {
    match val {
        Value::String(s) => {
            let key = s.trim();
            if let Some(&c) = palette.get(key) {
                return c;
            }
            let lower = key.to_ascii_lowercase();
            if let Some(&c) = palette.get(&lower) {
                return c;
            }
            resolve_palette_color(key).unwrap_or([136, 144, 158])
        }
        Value::Array(arr) if arr.len() >= 3 => {
            let r = arr[0].as_f64().unwrap_or(136.0).round().clamp(0.0, 255.0) as u8;
            let g = arr[1].as_f64().unwrap_or(144.0).round().clamp(0.0, 255.0) as u8;
            let b = arr[2].as_f64().unwrap_or(158.0).round().clamp(0.0, 255.0) as u8;
            [r, g, b]
        }
        _ => [136, 144, 158],
    }
}

fn val_f32(obj: &serde_json::Map<String, Value>, key: &str, default: f32) -> f32 {
    obj.get(key)
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn val_isize(obj: &serde_json::Map<String, Value>, key: &str, default: isize) -> isize {
    val_f32(obj, key, default as f32).round() as isize
}

fn val_bool(obj: &serde_json::Map<String, Value>, key: &str, default: bool) -> bool {
    obj.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

/// 將藍圖 JSON 渲染為 VoxelCanvas
pub fn render_blueprint_canvas(
    bp_value: &Value,
    target_slices: Option<usize>,
    auto_thicken: bool,
    polish: bool,
) -> Result<(String, VoxelCanvas), String> {
    let bp: Blueprint = serde_json::from_value(bp_value.clone())
        .map_err(|e| format!("藍圖 JSON 格式無效：{e}"))?;

    if bp.primitives.is_empty() {
        return Err("藍圖缺少有效的 primitives 幾何原語清單。".to_string());
    }

    let w = (bp.size[0].round() as usize).clamp(16, 96);
    let l = (bp.size[1].round() as usize).clamp(16, 96);
    let mut h = (bp.size[2].round() as usize).clamp(16, 80);

    let mut z_scale = 1.0f32;
    if let Some(ts) = target_slices {
        if ts > 0 && ts != h {
            let target_h = ts.clamp(16, 80);
            z_scale = target_h as f32 / h as f32;
            h = target_h;
        }
    }

    let mut palette: HashMap<String, Rgb> = HashMap::new();
    let empty_pal = HashMap::new();
    for (k, v) in &bp.palette {
        let trimmed = k.trim();
        let rgb = parse_rgb_value(v, &empty_pal);
        palette.insert(trimmed.to_string(), rgb);
        palette.insert(trimmed.to_ascii_lowercase(), rgb);
    }

    let mut c = VoxelCanvas::new(h, l, w);
    let sz = |z: f32| -> isize { (z * z_scale).round() as isize };

    for prim_val in bp.primitives.iter().take(250) {
        let Some(prim) = prim_val.as_object() else {
            continue;
        };
        let ptype = prim
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        let raw_col = prim.get("color").unwrap_or(&Value::Null);
        let col = parse_rgb_value(raw_col, &palette);
        let mat = match prim.get("material").and_then(|v| v.as_str()) {
            Some(m) if !m.trim().is_empty() => m.trim().to_ascii_lowercase(),
            _ => match raw_col.as_str() {
                Some(name) => default_material_for_color_name(name).to_string(),
                None => "flat".to_string(),
            },
        };

        let shaded = val_bool(prim, "shaded", true);
        let emissive = val_bool(prim, "emissive", false);
        let mirror = val_bool(prim, "mirror_x", false);

        match ptype.as_str() {
            "box" => {
                let x0 = val_isize(prim, "x0", 0);
                let y0 = val_isize(prim, "y0", 0);
                let z0 = sz(val_f32(prim, "z0", 0.0));
                let x1 = val_isize(prim, "x1", x0 + 2);
                let y1 = val_isize(prim, "y1", y0 + 2);
                let z1 = sz(val_f32(prim, "z1", 2.0));
                c.fill_box(x0, y0, z0, x1, y1, z1, col, shaded, emissive, &mat, None);
                if mirror {
                    let wi = w as isize;
                    c.fill_box(wi - x1, y0, z0, wi - x0, y1, z1, col, shaded, emissive, &mat, None);
                }
            }
            "clear_box" => {
                let x0 = val_isize(prim, "x0", 0);
                let y0 = val_isize(prim, "y0", 0);
                let z0 = sz(val_f32(prim, "z0", 0.0));
                let x1 = val_isize(prim, "x1", x0 + 2);
                let y1 = val_isize(prim, "y1", y0 + 2);
                let z1 = sz(val_f32(prim, "z1", 2.0));
                c.clear_box(x0, y0, z0, x1, y1, z1);
                if mirror {
                    let wi = w as isize;
                    c.clear_box(wi - x1, y0, z0, wi - x0, y1, z1);
                }
            }
            "cylinder" => {
                let cx_v = val_f32(prim, "cx", c.cx());
                let cy_v = val_f32(prim, "cy", c.cy());
                let z0f = val_f32(prim, "z0", 0.0);
                let z0 = sz(z0f);
                let z1 = sz(val_f32(prim, "z1", z0f + 4.0));
                let rx = val_f32(prim, "rx", 4.0).max(0.8);
                let ry = val_f32(prim, "ry", rx).max(0.8);
                let hollow = val_bool(prim, "hollow", false);
                let wall = val_f32(prim, "wall", 2.0).max(1.0);
                c.fill_cylinder(cx_v, cy_v, z0, z1, rx, ry, col, hollow, wall, shaded, emissive, &mat);
                if mirror {
                    let mcx = (w as f32 - 1.0) - cx_v;
                    c.fill_cylinder(mcx, cy_v, z0, z1, rx, ry, col, hollow, wall, shaded, emissive, &mat);
                }
            }
            "cone" => {
                let cx_v = val_f32(prim, "cx", c.cx());
                let cy_v = val_f32(prim, "cy", c.cy());
                let z0f = val_f32(prim, "z0", 0.0);
                let z0 = sz(z0f);
                let z1 = sz(val_f32(prim, "z1", z0f + 6.0));
                let rx0 = val_f32(prim, "rx0", 5.0).max(0.8);
                let ry0 = val_f32(prim, "ry0", rx0).max(0.8);
                let rx1 = val_f32(prim, "rx1", 1.5).max(0.8);
                let ry1 = val_f32(prim, "ry1", rx1).max(0.8);
                let hollow = val_bool(prim, "hollow", false);
                let wall = val_f32(prim, "wall", 2.0).max(1.0);
                c.fill_cone(
                    cx_v, cy_v, z0, z1, rx0, ry0, rx1, ry1, col, hollow, wall, shaded, emissive, &mat,
                );
                if mirror {
                    let mcx = (w as f32 - 1.0) - cx_v;
                    c.fill_cone(
                        mcx, cy_v, z0, z1, rx0, ry0, rx1, ry1, col, hollow, wall, shaded, emissive, &mat,
                    );
                }
            }
            "sphere" => {
                let cx_v = val_f32(prim, "cx", c.cx());
                let cy_v = val_f32(prim, "cy", c.cy());
                let cz_v = val_f32(prim, "cz", h as f32 * 0.5) * z_scale;
                let rx = val_f32(prim, "rx", 4.0).max(1.0);
                let ry = val_f32(prim, "ry", rx).max(1.0);
                let rz = (val_f32(prim, "rz", rx) * z_scale).max(1.0);
                c.fill_sphere(cx_v, cy_v, cz_v, rx, ry, rz, col, shaded, emissive, &mat);
                if mirror {
                    let mcx = (w as f32 - 1.0) - cx_v;
                    c.fill_sphere(mcx, cy_v, cz_v, rx, ry, rz, col, shaded, emissive, &mat);
                }
            }
            "pyramid" => {
                let cx_v = val_f32(prim, "cx", c.cx());
                let cy_v = val_f32(prim, "cy", c.cy());
                let z0f = val_f32(prim, "z0", 0.0);
                let z0 = sz(z0f);
                let z1 = sz(val_f32(prim, "z1", z0f + 8.0));
                let w0 = val_f32(prim, "w0", 12.0).max(1.5);
                let l0 = val_f32(prim, "l0", w0).max(1.5);
                let w1 = val_f32(prim, "w1", 2.0).max(1.5);
                let l1 = val_f32(prim, "l1", w1).max(1.5);
                let stepped = val_bool(prim, "stepped", false);
                let steps = val_isize(prim, "steps", 8).max(2) as usize;
                c.fill_pyramid(
                    cx_v, cy_v, z0, z1, w0, l0, w1, l1, col, stepped, steps, shaded, emissive, &mat,
                );
                if mirror {
                    let mcx = (w as f32 - 1.0) - cx_v;
                    c.fill_pyramid(
                        mcx, cy_v, z0, z1, w0, l0, w1, l1, col, stepped, steps, shaded, emissive, &mat,
                    );
                }
            }
            "arch_cut" => {
                let cx_v = val_f32(prim, "cx", c.cx());
                let cy_v = val_f32(prim, "cy", c.cy());
                let z0f = val_f32(prim, "z0", 0.0);
                let z0 = sz(z0f);
                let z1 = sz(val_f32(prim, "z1", z0f + 6.0));
                let half_span = val_f32(prim, "half_span", 3.0).max(1.0);
                let depth = val_f32(prim, "depth", 4.0).max(1.0);
                let axis = match prim.get("axis").and_then(|v| v.as_str()) {
                    Some(a) if a.eq_ignore_ascii_case("x") => "x",
                    _ => "y",
                };
                c.fill_arch_cutout(cx_v, cy_v, z0, z1, half_span, depth, axis);
            }
            "line" => {
                let p0_arr = prim.get("p0").and_then(|v| v.as_array());
                let p1_arr = prim.get("p1").and_then(|v| v.as_array());
                if let (Some(a0), Some(a1)) = (p0_arr, p1_arr) {
                    if a0.len() == 3 && a1.len() == 3 {
                        let pt0 = [
                            a0[0].as_f64().unwrap_or(c.cx() as f64) as f32,
                            a0[1].as_f64().unwrap_or(c.cy() as f64) as f32,
                            a0[2].as_f64().unwrap_or(0.0) as f32 * z_scale,
                        ];
                        let pt1 = [
                            a1[0].as_f64().unwrap_or(c.cx() as f64) as f32,
                            a1[1].as_f64().unwrap_or(c.cy() as f64) as f32,
                            a1[2].as_f64().unwrap_or(10.0) as f32 * z_scale,
                        ];
                        let rad = val_f32(prim, "radius", 1.2).max(1.0);
                        c.draw_line(pt0, pt1, rad, col, shaded, emissive, &mat);
                        if mirror {
                            let mpt0 = [(w as f32 - 1.0) - pt0[0], pt0[1], pt0[2]];
                            let mpt1 = [(w as f32 - 1.0) - pt1[0], pt1[1], pt1[2]];
                            c.draw_line(mpt0, mpt1, rad, col, shaded, emissive, &mat);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if c.voxel_count() == 0 {
        return Err("藍圖渲染結果為空，請檢查原語座標是否落在畫布範圍內。".to_string());
    }
    if polish {
        c.polish_surface(0.26, 0.14);
    }
    if auto_thicken {
        c.auto_thicken_thin_runs();
    }
    c.trim_and_center(2, None);

    Ok((bp.title, c))
}

/// 解析並完整烘焙藍圖 JSON 為可用之 Sprite-Stack 資產
pub fn bake_blueprint_json(
    bp_value: &Value,
    category: &str,
    default_scale: f32,
    anim_preset: &str,
    target_slices: Option<usize>,
    auto_thicken: bool,
    polish: bool,
) -> Result<BakedStack, String> {
    let (title, canvas) = render_blueprint_canvas(bp_value, target_slices, auto_thicken, polish)?;
    let bp_str = serde_json::to_string_pretty(bp_value)
        .map_err(|e| format!("藍圖序列化失敗：{e}"))?;
    canvas.bake(
        title,
        category,
        Some(bp_str),
        default_scale,
        anim_preset,
        false, // 已於 render_blueprint_canvas 處理 polish 與 auto_thicken
        false,
    )
}

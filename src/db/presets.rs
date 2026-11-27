//! 四大可玩主題世界範本與空白世界建置器：
//! - `kingdom`：奇幻星光王國（城堡高地、護城河石橋、騎士巡邏、牧場獅羊、慶典煙火）
//! - `city`：CityRT 霓虹都會（十字大道與斑馬線、運河、摩天樓與小店、巴士環線停靠）
//! - `farm`：療癒陽光農莊（風車與紅穀倉、麥田向日葵、綿羊柴犬漫遊、農夫巡視）
//! - `campus`：櫻花青春校園（教學樓、紅跑道操場、櫻花大道、學生作息鐘聲排程）
//! - `blank`：自由空白微縮島嶼

use super::{
    init_world_schema, insert_asset_into_world, now_iso, open_world_conn, sanitize_world_filename,
    set_meta_kv, WORLD_EXT,
};
use crate::db::models::*;
use crate::voxel::generators::generate_from_prompt;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

pub(super) struct WorldBuilder<'a> {
    pub(super) conn: &'a Connection,
    pub(super) data_dir: &'a Path,
    pub(super) w: i32,
    pub(super) h: i32,
    pub(super) tiles: Vec<TerrainTile>,
    pub(super) asset_map: HashMap<String, AssetMeta>,
    pub(super) entities: Vec<WorldEntity>,
    pub(super) paths: Vec<MovementPath>,
    pub(super) zones: Vec<WanderZone>,
    pub(super) events: Vec<EventRule>,
    pub(super) quest_config: Option<Value>,
}

impl<'a> WorldBuilder<'a> {
    pub(super) fn new(conn: &'a Connection, data_dir: &'a Path, w: i32, h: i32, default_ground: &str) -> Self {
        let mut tiles = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                tiles.push(TerrainTile {
                    x,
                    y,
                    elevation: 1,
                    ground_type: default_ground.to_string(),
                    road_type: "none".to_string(),
                    custom_color: None,
                });
            }
        }
        Self {
            conn,
            data_dir,
            w,
            h,
            tiles,
            asset_map: HashMap::new(),
            entities: Vec::new(),
            paths: Vec::new(),
            zones: Vec::new(),
            events: Vec::new(),
            quest_config: None,
        }
    }

    pub(super) fn tile_mut(&mut self, x: i32, y: i32) -> Option<&mut TerrainTile> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        Some(&mut self.tiles[(y * self.w + x) as usize])
    }

    pub(super) fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, elev: i32, ground: &str, road: &str) {
        for y in y0.max(0)..y1.min(self.h) {
            for x in x0.max(0)..x1.min(self.w) {
                if let Some(t) = self.tile_mut(x, y) {
                    t.elevation = elev;
                    t.ground_type = ground.to_string();
                    t.road_type = road.to_string();
                }
            }
        }
    }

    pub(super) fn set_road_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, road: &str) {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=steps {
            let x = x0 + ((x1 - x0) * i) / steps;
            let y = y0 + ((y1 - y0) * i) / steps;
            if let Some(t) = self.tile_mut(x, y) {
                if t.ground_type == "water"
                    && matches!(road, "stone" | "wood" | "asphalt" | "dirt")
                {
                    t.road_type = "bridge".to_string();
                } else {
                    t.road_type = road.to_string();
                }
            }
        }
    }

    pub(super) fn ensure_asset(&mut self, key: &str, slices: usize) -> Result<AssetMeta, String> {
        if let Some(a) = self.asset_map.get(key) {
            return Ok(a.clone());
        }
        let baked = generate_from_prompt(key, slices, 42)?;
        let meta = insert_asset_into_world(self.conn, self.data_dir, &baked, "preset", None)?;
        self.asset_map.insert(key.to_string(), meta.clone());
        Ok(meta)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn spawn(
        &mut self,
        asset_key: &str,
        slices: usize,
        name: &str,
        x: f64,
        y: f64,
        z: f64,
        yaw_deg: f64,
        scale_mul: f64,
        tag: &str,
    ) -> Result<String, String> {
        let asset = self.ensure_asset(asset_key, slices)?;
        let id = Uuid::new_v4().to_string();
        let s = (asset.default_scale as f64) * scale_mul;
        self.entities.push(WorldEntity {
            id: id.clone(),
            asset_id: asset.id.clone(),
            name: name.to_string(),
            x,
            y,
            z,
            yaw: yaw_deg,
            scale: s,
            scale_x: s,
            scale_y: s,
            scale_z: s,
            parent_id: None,
            tint_hex: "#ffffff".to_string(),
            emissive_on: true,
            visible: true,
            anim_mode: "auto".to_string(),
            tag: tag.to_string(),
            meta_json: "{}".to_string(),
        });
        Ok(id)
    }

    pub(super) fn add_path(
        &mut self,
        name: &str,
        entity_id: &str,
        loop_mode: &str,
        speed: f64,
        smooth: bool,
        color: &str,
        wps: Vec<(f64, f64, f64, f64)>,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        self.paths.push(MovementPath {
            id: id.clone(),
            name: name.to_string(),
            entity_id: Some(entity_id.to_string()),
            loop_mode: loop_mode.to_string(),
            speed,
            smooth,
            active: true,
            color: color.to_string(),
            waypoints: wps
                .into_iter()
                .map(|(x, y, z, wait_sec)| Waypoint { x, y, z, wait_sec })
                .collect(),
        });
        id
    }

    pub(super) fn add_wander_zone(
        &mut self,
        name: &str,
        cx: f64,
        cy: f64,
        radius: f64,
        speed: f64,
        entity_ids: Vec<String>,
        color: &str,
    ) {
        self.zones.push(WanderZone {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            cx,
            cy,
            radius,
            speed,
            entity_ids,
            color: color.to_string(),
        });
    }

    pub(super) fn add_event(
        &mut self,
        name: &str,
        trigger_type: &str,
        trigger_config: Value,
        actions: Vec<Value>,
        cooldown_sec: f64,
    ) {
        self.events.push(EventRule {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            enabled: true,
            trigger_type: trigger_type.to_string(),
            trigger_config,
            actions,
            cooldown_sec,
        });
    }

    pub(super) fn set_interaction(&mut self, entity_id: &str, interaction: Value) {
        if let Some(ent) = self.entities.iter_mut().find(|e| e.id == entity_id) {
            let mut root = serde_json::from_str::<Value>(&ent.meta_json).unwrap_or_else(|_| json!({}));
            if let Some(obj) = root.as_object_mut() {
                obj.insert("interaction".to_string(), interaction);
            } else {
                root = json!({ "interaction": interaction });
            }
            ent.meta_json = root.to_string();
        }
    }

    pub(super) fn set_portal(&mut self, entity_id: &str, portal: Value) {
        if let Some(ent) = self.entities.iter_mut().find(|e| e.id == entity_id) {
            let mut root = serde_json::from_str::<Value>(&ent.meta_json).unwrap_or_else(|_| json!({}));
            if let Some(obj) = root.as_object_mut() {
                obj.insert("portal".to_string(), portal);
            } else {
                root = json!({ "portal": portal });
            }
            ent.meta_json = root.to_string();
        }
    }

    pub(super) fn set_vehicle(&mut self, entity_id: &str, vehicle: Value) {
        if let Some(ent) = self.entities.iter_mut().find(|e| e.id == entity_id) {
            let mut root = serde_json::from_str::<Value>(&ent.meta_json).unwrap_or_else(|_| json!({}));
            if let Some(obj) = root.as_object_mut() {
                obj.insert("vehicle".to_string(), vehicle);
            } else {
                root = json!({ "vehicle": vehicle });
            }
            ent.meta_json = root.to_string();
        }
    }

    pub(super) fn set_tint(&mut self, entity_id: &str, hex: &str) {
        if let Some(ent) = self.entities.iter_mut().find(|e| e.id == entity_id) {
            ent.tint_hex = hex.to_string();
        }
    }

    pub(super) fn set_quest(&mut self, cfg: Value) {
        self.quest_config = Some(cfg);
    }

    pub(super) fn flush(self, meta_pairs: Vec<(&str, String)>) -> Result<(), String> {
        let quest_cfg = self.quest_config;
        for (k, v) in meta_pairs {
            set_meta_kv(self.conn, k, &v)?;
        }
        if let Some(qc) = quest_cfg {
            set_meta_kv(self.conn, "quest_config_json", &qc.to_string())?;
        }
        self.conn.execute_batch("BEGIN IMMEDIATE;")
            .map_err(|e| format!("開啟批次寫入失敗：{e}"))?;

        for t in &self.tiles {
            self.conn
                .execute(
                    "INSERT INTO terrain_tiles (x, y, elevation, ground_type, road_type, custom_color)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![t.x, t.y, t.elevation, t.ground_type, t.road_type, t.custom_color],
                )
                .map_err(|e| format!("寫入地形格點失敗：{e}"))?;
        }
        for e in &self.entities {
            self.conn
                .execute(
                    "INSERT INTO world_entities
                     (id, asset_id, name, x, y, z, yaw, scale, scale_x, scale_y, scale_z, parent_id, tint_hex, emissive_on, visible, anim_mode, tag, meta_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                    params![
                        e.id, e.asset_id, e.name, e.x, e.y, e.z, e.yaw, e.scale, e.scale_x, e.scale_y, e.scale_z, e.parent_id,
                        e.tint_hex, e.emissive_on as i64, e.visible as i64, e.anim_mode, e.tag, e.meta_json
                    ],
                )
                .map_err(|err| format!("寫入實體失敗：{err}"))?;
        }
        for p in &self.paths {
            let wp_json = serde_json::to_string(&p.waypoints).unwrap_or_else(|_| "[]".into());
            self.conn
                .execute(
                    "INSERT INTO movement_paths (id, name, entity_id, loop_mode, speed, smooth, active, color, waypoints_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        p.id, p.name, p.entity_id, p.loop_mode, p.speed, p.smooth as i64, p.active as i64,
                        p.color, wp_json
                    ],
                )
                .map_err(|err| format!("寫入移動路徑失敗：{err}"))?;
        }
        for z in &self.zones {
            let ids_json = serde_json::to_string(&z.entity_ids).unwrap_or_else(|_| "[]".into());
            self.conn
                .execute(
                    "INSERT INTO wander_zones (id, name, cx, cy, radius, speed, entity_ids_json, color)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![z.id, z.name, z.cx, z.cy, z.radius, z.speed, ids_json, z.color],
                )
                .map_err(|err| format!("寫入漫遊區失敗：{err}"))?;
        }
        for ev in &self.events {
            let trig_json = serde_json::to_string(&ev.trigger_config).unwrap_or_else(|_| "{}".into());
            let act_json = serde_json::to_string(&ev.actions).unwrap_or_else(|_| "[]".into());
            self.conn
                .execute(
                    "INSERT INTO event_rules (id, name, enabled, trigger_type, trigger_config_json, actions_json, cooldown_sec)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![ev.id, ev.name, ev.enabled as i64, ev.trigger_type, trig_json, act_json, ev.cooldown_sec],
                )
                .map_err(|err| format!("寫入事件規則失敗：{err}"))?;
        }
        self.conn.execute_batch("COMMIT;")
            .map_err(|e| format!("提交世界建置失敗：{e}"))?;
        Ok(())
    }
}

/// 依主題名稱建立全新 `.stackworld.db` 世界檔案
pub fn create_world_from_preset(
    data_dir: &Path,
    theme: &str,
    world_name: &str,
    author: &str,
    explicit_filename: Option<&str>,
) -> Result<WorldSummary, String> {
    let base = explicit_filename.unwrap_or(world_name);
    let mut candidate = sanitize_world_filename(base)?;
    let mut path = data_dir.join(&candidate);
    if path.exists() {
        let stem = candidate.trim_end_matches(WORLD_EXT);
        let short = &Uuid::new_v4().to_string()[..6];
        candidate = format!("{stem}_{short}{WORLD_EXT}");
        path = data_dir.join(&candidate);
    }

    let conn = open_world_conn(&path)?;
    init_world_schema(&conn)?;
    let world_id = Uuid::new_v4().to_string();
    populate_world_by_theme(&conn, data_dir, theme, world_name, author, &world_id)?;

    let summaries = super::list_worlds(data_dir)?;
    summaries
        .into_iter()
        .find(|s| s.file_name == candidate)
        .ok_or_else(|| "建立世界後無法讀取世界摘要。".to_string())
}

/// 原地清空並以該主題完整重建世界初始狀態（保留原檔案路徑、世界 ID 與創作者設定）
pub fn rebuild_world_in_place(
    data_dir: &Path,
    path: &Path,
    theme: &str,
    world_name: &str,
    author: &str,
    world_id: &str,
) -> Result<(), String> {
    let conn = open_world_conn(path)?;
    init_world_schema(&conn)?;
    // 注意：不可包在外部交易內，因 WorldBuilder::flush 會自行啟用 BEGIN IMMEDIATE 交易
    conn.execute_batch(
        "DELETE FROM world_snapshots;
         DELETE FROM movement_paths;
         DELETE FROM wander_zones;
         DELETE FROM event_rules;
         DELETE FROM world_entities;
         DELETE FROM terrain_tiles;
         DELETE FROM asset_catalog;
         DELETE FROM world_bgm;
         DELETE FROM world_meta;",
    )
    .map_err(|e| format!("清空世界舊資料以回復初始值失敗：{e}"))?;

    populate_world_by_theme(&conn, data_dir, theme, world_name, author, world_id)
}

fn populate_world_by_theme(
    conn: &Connection,
    data_dir: &Path,
    theme: &str,
    world_name: &str,
    author: &str,
    world_id: &str,
) -> Result<(), String> {
    let is_indoor = theme.ends_with("_interior");
    let (w, h) = if is_indoor { (24, 24) } else { (40, 40) };
    let created = now_iso();

    let (desc, start_time, start_weather, default_ground, bgm_preset) = match theme {
        "city" => (
            "CityRT 風格霓虹都會：十字柏油大道、運河橋樑、公車停靠站與夜景大樓",
            17.5,
            "clear",
            "grass",
            "city",
        ),
        "farm" => (
            "療癒陽光農莊：風車、紅頂穀倉、向日葵麥田、綿羊與柴犬牧場",
            9.0,
            "clear",
            "grass",
            "farm",
        ),
        "campus" => (
            "櫻花青春校園：教學樓、紅色操場、鐘樓作息排程與櫻花林蔭道",
            7.9,
            "clear",
            "grass",
            "campus",
        ),
        "harbor" => (
            "海港燈塔小鎮：海灣水面、木棧碼頭、燈塔、航海帆船與海鷗飛鳥",
            16.2,
            "clear",
            "sand",
            "farm",
        ),
        "palace_interior" => (
            "皇家宮殿大廳（建築內部）：大理石柱列、紅毯御道、黃金王座、水晶吊燈與騎士晉見",
            13.0,
            "clear",
            "tile_floor",
            "kingdom",
        ),
        "shop_interior" => (
            "精品便利商店（建築內部）：暖白地磚、收銀櫃檯、陳列貨架、甜點展示櫃與自動販賣機",
            15.0,
            "clear",
            "tile_floor",
            "indoor",
        ),
        "factory_interior" => (
            "智慧自動化工廠（建築內部）：工業水泥地、輸送帶、機械手臂、高爐與堆高機巡航",
            11.0,
            "clear",
            "concrete",
            "city",
        ),
        "office_interior" => (
            "現代科技辦公室（建築內部）：商務藍毯、玻璃幕牆、電腦辦公桌、會議長桌與飲水機",
            10.0,
            "clear",
            "carpet_blue",
            "indoor",
        ),
        "classroom_interior" => (
            "陽光活力教室（建築內部）：實木地板、黑板講台、整齊課桌椅與下課學生互動",
            10.2,
            "clear",
            "wood_floor",
            "campus",
        ),
        "home_interior" => (
            "溫馨陽光住家（建築內部）：客廳沙發、發光電視櫃、餐桌廚房、雙人床與看門柴犬",
            19.2,
            "clear",
            "wood_floor",
            "indoor",
        ),
        "blank" => ("自由創作空白微縮沙盒，盡情打造你的專屬世界", 10.5, "clear", "grass", "kingdom"),
        _ => (
            "奇幻星光王國：石崖城堡、護城河石橋、騎士巡邏與夜間慶典煙火",
            10.5,
            "clear",
            "grass",
            "kingdom",
        ),
    };

    let mut b = WorldBuilder::new(conn, data_dir, w, h, default_ground);

    match theme {
        "city" => build_city(&mut b)?,
        "farm" => build_farm(&mut b)?,
        "campus" => build_campus(&mut b)?,
        "harbor" => build_harbor(&mut b)?,
        "palace_interior" => build_palace_interior(&mut b)?,
        "shop_interior" => build_shop_interior(&mut b)?,
        "factory_interior" => build_factory_interior(&mut b)?,
        "office_interior" => build_office_interior(&mut b)?,
        "classroom_interior" => build_classroom_interior(&mut b)?,
        "home_interior" => build_home_interior(&mut b)?,
        "blank" => build_blank(&mut b)?,
        _ => build_kingdom(&mut b)?,
    }

    let meta_pairs = vec![
        ("schema_version", super::SCHEMA_VERSION.to_string()),
        ("world_id", world_id.to_string()),
        ("name", world_name.to_string()),
        ("author", author.to_string()),
        ("description", desc.to_string()),
        ("theme", theme.to_string()),
        ("width", w.to_string()),
        ("height", h.to_string()),
        ("time_of_day", start_time.to_string()),
        ("initial_time_of_day", start_time.to_string()),
        ("time_speed", "1.0".to_string()),
        ("weather", start_weather.to_string()),
        ("bgm_preset", bgm_preset.to_string()),
        ("bgm_volume", "0.45".to_string()),
        ("created_at", created.clone()),
        ("updated_at", created),
        ("thumbnail_b64", String::new()),
    ];
    b.flush(meta_pairs)
}

fn build_kingdom(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    // 城堡高地（北部 x:11..29, y:5..17, elev=2）與山腳花石道
    b.fill_rect(10, 4, 30, 18, 2, "stone", "none");
    b.fill_rect(13, 6, 27, 16, 2, "grass", "none");
    // 護城河橫貫（y: 19..22）與南北石橋
    b.fill_rect(0, 19, 40, 22, 0, "water", "none");
    // 西北小湖泊與海岸
    b.fill_rect(2, 27, 10, 35, 0, "water", "none");
    b.fill_rect(1, 26, 11, 36, 1, "sand", "none");
    b.fill_rect(2, 27, 10, 35, 0, "water", "none");

    // 鋪設王國石板主幹道與 2 格寬護城河石橋（x = 19..20）
    b.set_road_line(19, 14, 19, 36, "stone");
    b.set_road_line(20, 14, 20, 36, "stone");
    b.set_road_line(6, 25, 35, 25, "stone");
    b.set_road_line(28, 25, 28, 35, "dirt");

    // 主要建築與地標
    let castle_id = b.spawn("castle", 44, "星光主城堡", 20.0, 10.5, 0.0, 0.0, 1.35, "landmark")?;
    let tower_id = b.spawn("clock_tower", 44, "皇家鐘樓", 12.5, 11.0, 0.0, 0.0, 1.0, "clock")?;
    let fountain_id = b.spawn("fountain", 32, "王國許願噴泉", 20.0, 27.2, 0.0, 0.0, 1.05, "landmark")?;
    b.spawn("cottage", 40, "麵包師木屋", 14.0, 29.0, 0.0, 15.0, 1.0, "village")?;
    b.spawn("cottage", 40, "花店木屋", 25.5, 29.5, 0.0, -25.0, 1.0, "village")?;
    b.spawn("shop", 40, "王城小酒館", 27.0, 12.0, 0.0, 0.0, 0.95, "village")?;
    b.spawn("lighthouse", 44, "湖畔燈塔", 10.5, 28.0, 0.0, 0.0, 1.0, "beacon")?;
    let crystal_id = b.spawn("crystal", 38, "星辰守護水晶", 13.5, 16.0, 0.0, 0.0, 0.95, "magic")?;

    // 夜間自動亮燈的街燈與營火
    for (lx, ly) in [
        (18.5, 18.2),
        (21.5, 18.2),
        (18.5, 22.8),
        (21.5, 22.8),
        (12.0, 24.0),
        (28.0, 24.0),
    ] {
        b.spawn("lamp", 34, "王國路燈", lx, ly, 0.0, 0.0, 0.9, "night_light")?;
    }
    b.spawn("campfire", 32, "廣場營火", 32.5, 29.0, 0.0, 0.0, 1.0, "night_light")?;

    // 樹林與花草點綴
    for (tx, ty) in [
        (6.0, 8.0),
        (8.0, 13.0),
        (33.0, 8.0),
        (35.0, 14.0),
        (16.0, 15.0),
        (24.0, 15.0),
    ] {
        b.spawn("sakura", 42, "櫻花樹", tx, ty, 0.0, (tx * 29.0) % 360.0, 0.95, "tree")?;
    }
    for (tx, ty) in [(4.0, 6.0), (5.0, 16.0), (36.0, 6.0), (36.5, 18.0), (36.0, 34.0)] {
        b.spawn("pine", 42, "松樹", tx, ty, 0.0, (ty * 41.0) % 360.0, 1.0, "tree")?;
    }
    b.spawn("oak", 42, "廣場大橡樹", 10.5, 33.0, 0.0, 0.0, 1.05, "tree")?;

    // 巡邏騎士＋法師
    let knight_id = b.spawn("knight", 42, "皇家巡邏騎士", 19.8, 22.5, 0.0, 0.0, 1.0, "patrol")?;
    b.add_path(
        "騎士王城巡邏線",
        &knight_id,
        "loop",
        2.3,
        true,
        "#38bdf8",
        vec![
            (19.8, 15.5, 0.0, 1.5),
            (19.8, 25.0, 0.0, 1.0),
            (11.0, 25.0, 0.0, 0.5),
            (29.0, 25.0, 0.0, 0.5),
            (19.8, 25.0, 0.0, 0.5),
        ],
    );

    let mage_id = b.spawn("mage", 42, "星辰大法師", 14.0, 14.0, 0.0, 0.0, 1.0, "mage")?;
    b.add_path(
        "法師高台散步線",
        &mage_id,
        "pingpong",
        1.5,
        true,
        "#c084fc",
        vec![(14.0, 14.0, 0.0, 2.5), (26.0, 14.0, 0.0, 2.5)],
    );

    // 空中盤旋熱氣球與湖面帆船
    let balloon_id = b.spawn("balloon", 44, "王國觀光熱氣球", 10.0, 10.0, 6.5, 0.0, 1.05, "sky")?;
    b.add_path(
        "天空熱氣球航線",
        &balloon_id,
        "loop",
        1.8,
        true,
        "#f472b6",
        vec![
            (9.0, 9.0, 6.5, 0.0),
            (31.0, 9.0, 7.2, 0.0),
            (32.0, 31.0, 6.8, 0.0),
            (9.0, 31.0, 6.2, 0.0),
        ],
    );
    let boat_id = b.spawn("sailboat", 42, "護城河遊船", 5.0, 20.5, 0.2, 90.0, 0.9, "boat")?;
    b.add_path(
        "護城河航行線",
        &boat_id,
        "pingpong",
        1.4,
        true,
        "#22d3ee",
        vec![(3.0, 20.5, 0.2, 1.5), (17.5, 20.5, 0.2, 1.5)],
    );

    // 東南牧場自由漫遊動物（含公獅與綿羊）
    let s1 = b.spawn("sheep", 28, "綿羊小白", 32.0, 32.0, 0.0, 0.0, 1.0, "animal")?;
    let s2 = b.spawn("sheep", 28, "綿羊雲朵", 34.5, 33.5, 0.0, 120.0, 0.95, "animal")?;
    let lion_id = b.spawn("lion", 36, "王國金毛獅王", 31.0, 35.0, 0.0, 45.0, 1.0, "animal")?;
    b.add_wander_zone(
        "東南王家牧場",
        33.0,
        33.0,
        4.5,
        1.1,
        vec![s1, s2, lion_id],
        "#4ade80",
    );

    // 5 條示範活體事件規則
    b.add_event(
        "清晨王城鐘聲",
        "clock",
        json!({"hour": 7, "minute": 0}),
        vec![
            json!({"type": "play_sound", "sound": "bell"}),
            json!({"type": "speech_bubble", "entity_id": tower_id, "text": "早安！王國新的一天開始了！", "duration": 5.0}),
            json!({"type": "emissive_toggle", "tag": "night_light", "on": false}),
        ],
        10.0,
    );
    b.add_event(
        "黃昏點亮街燈與營火",
        "clock",
        json!({"hour": 18, "minute": 30}),
        vec![
            json!({"type": "emissive_toggle", "tag": "night_light", "on": true}),
            json!({"type": "play_sound", "sound": "chime"}),
            json!({"type": "speech_bubble", "entity_id": castle_id, "text": "夜幕降臨，點亮星光燈火！", "duration": 4.5}),
        ],
        10.0,
    );
    b.add_event(
        "夜間九點王城煙火秀",
        "clock",
        json!({"hour": 21, "minute": 0}),
        vec![
            json!({"type": "particle_burst", "effect": "firework", "x": 20.0, "y": 10.5, "z": 8.0, "count": 48}),
            json!({"type": "play_sound", "sound": "firework"}),
            json!({"type": "speech_bubble", "entity_id": castle_id, "text": "王國星光煙火盛典開始！", "duration": 5.0}),
        ],
        10.0,
    );
    b.add_event(
        "點擊城堡國王慶賀",
        "click",
        json!({"entity_id": castle_id}),
        vec![
            json!({"type": "particle_burst", "effect": "sparkle", "x": 20.0, "y": 10.5, "z": 5.0, "count": 28}),
            json!({"type": "scale_tween", "entity_id": castle_id, "scale_mul": 1.12, "duration": 0.6}),
            json!({"type": "play_sound", "sound": "chime"}),
            json!({"type": "speech_bubble", "entity_id": castle_id, "text": "歡迎蒞臨《像素疊界》星光王國！", "duration": 4.0}),
        ],
        1.5,
    );
    b.add_event(
        "騎士經過噴泉向市民致意",
        "proximity",
        json!({"entity_id": knight_id, "x": 20.0, "y": 25.0, "radius": 2.5}),
        vec![
            json!({"type": "speech_bubble", "entity_id": knight_id, "text": "城區一切平安，祝您旅途愉快！", "duration": 3.5}),
        ],
        12.0,
    );
    b.add_event(
        "騎士與法師相遇共護王國",
        "encounter",
        json!({"entity_a": knight_id, "entity_b": mage_id, "radius": 2.2}),
        vec![
            json!({"type": "speech_bubble", "entity_id": mage_id, "text": "願星辰魔法守護騎士的長劍！", "duration": 4.0}),
            json!({"type": "particle_burst", "effect": "sparkle", "x": 18.0, "y": 15.0, "z": 2.5, "count": 22}),
            json!({"type": "play_sound", "sound": "encounter"}),
        ],
        10.0,
    );

    // 示範任務關卡：蒙面山賊（位於南岸東側草地）與五大互動目標
    let bandit_id = b.spawn("knight", 42, "蒙面山賊", 30.0, 26.5, 0.0, 180.0, 1.0, "robber")?;
    b.set_tint(&bandit_id, "#475569");

    b.set_interaction(
        &fountain_id,
        json!({
            "kind": "clue",
            "title": "王國許願噴泉的秘密線索",
            "dialogue": "噴泉水紋顯現出王國啟示：先向「皇家巡邏騎士」領取護盾支援，過橋前往城堡台地找「星辰大法師」解謎，並提防東側草地的「蒙面山賊」！",
            "clue_text": "【噴泉線索】找巡邏騎士拿護盾、過橋找大法師解謎，小心東側營火旁的蒙面山賊！",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &knight_id,
        json!({
            "kind": "support",
            "title": "皇家巡邏騎士的忠義支援",
            "dialogue": "冒險者你好！前方東側草地傳來山賊出沒的消息，這面皇家護盾先借給你防身，遇到搶劫時可直接反制！",
            "clue_text": "【騎士情報】帶著護盾去東南側對付「蒙面山賊」，或協助法師守護星辰水晶！",
            "grant_shield": 1,
            "heal_hp": 2,
            "reward_points": 20,
            "once": true
        }),
    );
    b.set_interaction(
        &mage_id,
        json!({
            "kind": "quiz",
            "title": "星辰大法師的魔法考驗",
            "dialogue": "通過星辰智慧試煉，我便將開啟守護水晶的鑰匙交給你！",
            "question": "在 Sprite-Stacking 體素堆疊技術中，從底層一路往上疊加切片並決定建築高度的是哪一條座標軸？",
            "options": [
                "A. Z 軸（垂直高度層數）",
                "B. 僅存在 2D 畫面寬度的 U 軸",
                "C. 音訊取樣率軸"
            ],
            "correct_index": 0,
            "reward_points": 35,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "星辰寶庫鑰匙",
            "success_text": "答對了！星辰智慧與你同在，這把「星辰寶庫鑰匙」收好吧！",
            "fail_text": "差一點喔！想想我們將每一層切面由下往上疊高，提示是英文字母最後一位！",
            "once": true
        }),
    );
    b.set_interaction(
        &bandit_id,
        json!({
            "kind": "robbery",
            "title": "⚠️ 遭遇蒙面山賊攔路搶劫！",
            "dialogue": "站住！把身上的積分交出來，不然休怪我的長槍不客氣！",
            "auto_trigger_radius": 2.0,
            "demand_points": 20,
            "hp_damage": 1,
            "allow_shield_counter": true,
            "hide_on_defeat": true,
            "question": "山賊朝你左側揮來空隙，想機智反擊必須如何應對？",
            "options": [
                "A. 蹲下並借力絆倒山賊奪回寶石",
                "B. 原地閉眼站著不動"
            ],
            "correct_index": 0,
            "reward_points": 40,
            "reward_item": "失竊王冠寶石",
            "success_text": "漂亮！你成功制伏蒙面山賊，奪回了失竊的王冠寶石並獲得 40 分！",
            "fail_text": "被山賊趁亂撞倒並搶走了 20 分！建議先找巡邏騎士拿護盾再來討伐！",
            "once": true
        }),
    );
    b.set_interaction(
        &crystal_id,
        json!({
            "kind": "treasure",
            "title": "星辰守護水晶寶藏",
            "dialogue": "巨大的魔法水晶裡封存著王國的星光勳章，是否使用鑰匙解封？",
            "required_item": "星辰寶庫鑰匙",
            "locked_text": "水晶被魔法封印鎖住了！需先回答北側高台「星辰大法師」的考題才能獲得「星辰寶庫鑰匙」。",
            "success_text": "水晶共鳴耀眼星光！你獲得珍貴的「星光徽章」與 35 積分！",
            "reward_points": 35,
            "reward_item": "星光徽章",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "守護星光王國：積分大冒險",
        "goal_type": "score",
        "target_count": 100,
        "culprit_entity_id": "",
        "checklist": [],
        "description": "向「王國許願噴泉」與「皇家巡邏騎士」取得線索和護盾，挑戰「星辰大法師」選擇題或擊退「蒙面山賊」，累積滿 100 分即可過關！",
        "victory_title": "🎉 星光王國英雄誕生！",
        "victory_message": "你憑藉智慧、勇氣與騎士的支援，成功累積積分並守護了星光王國的榮耀！"
    }));

    Ok(())
}

fn build_city(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    // 城市運河（y: 26..29）與河岸步道
    b.fill_rect(0, 26, 40, 29, 0, "water", "none");
    // 十字柏油大道＋雙線
    for x in 0..40 {
        b.set_road_line(x, 15, x, 15, "asphalt");
        b.set_road_line(x, 16, x, 16, "asphalt");
    }
    for y in 0..40 {
        b.set_road_line(19, y, 19, y, "asphalt");
        b.set_road_line(20, y, 20, y, "asphalt");
        b.set_road_line(31, y, 31, y, "asphalt");
    }
    // 中央公園綠地
    b.fill_rect(22, 18, 30, 25, 1, "grass", "none");

    let hq_id = b.spawn("skyscraper", 48, "中央星海大廈", 12.0, 9.0, 0.0, 0.0, 1.25, "landmark")?;
    b.spawn("skyscraper", 48, "藍曜商業大樓", 25.5, 9.5, 0.0, 90.0, 1.15, "building")?;
    b.spawn("skyscraper", 48, "東區科技塔", 35.5, 10.0, 0.0, 180.0, 1.05, "building")?;
    let ramen_id = b.spawn("shop", 40, "深夜拉麵屋", 9.0, 21.0, 0.0, 0.0, 1.0, "shop")?;
    b.spawn("shop", 40, "霓虹咖啡廳", 15.0, 21.0, 0.0, 0.0, 1.0, "shop")?;
    let city_tower_id = b.spawn("clock_tower", 44, "市政廣場鐘樓", 26.0, 19.2, 0.0, 0.0, 0.9, "clock")?;
    b.spawn("fountain", 32, "市民噴泉", 26.0, 33.5, 0.0, 0.0, 1.0, "landmark")?;

    for (lx, ly) in [(17.5, 14.0), (22.0, 14.0), (17.5, 17.5), (22.0, 17.5), (10.0, 17.5), (30.0, 17.5)] {
        b.spawn("lamp", 34, "都市街燈", lx, ly, 0.0, 0.0, 0.9, "night_light")?;
    }
    for (tx, ty) in [(23.0, 19.5), (29.0, 19.5), (23.5, 24.0), (28.5, 24.0), (8.0, 33.0)] {
        b.spawn("sakura", 42, "行道櫻花樹", tx, ty, 0.0, 0.0, 0.85, "tree")?;
    }

    let bus_id = b.spawn("bus", 34, "101 號城市巴士", 6.0, 15.5, 0.0, 90.0, 1.0, "vehicle")?;
    b.add_path(
        "市中心環線公車",
        &bus_id,
        "loop",
        3.2,
        false,
        "#facc15",
        vec![
            (6.0, 15.5, 0.0, 3.0),
            (19.5, 15.5, 0.0, 0.5),
            (31.0, 15.5, 0.0, 2.5),
            (31.0, 34.0, 0.0, 2.0),
            (19.5, 34.0, 0.0, 0.5),
            (19.5, 15.5, 0.0, 0.5),
        ],
    );
    let car_id = b.spawn("car", 26, "紅閃電跑車", 19.5, 6.0, 0.0, 0.0, 1.0, "vehicle")?;
    b.add_path(
        "都會高速巡航車",
        &car_id,
        "loop",
        4.6,
        false,
        "#fb7185",
        vec![
            (19.5, 5.0, 0.0, 0.0),
            (19.5, 36.0, 0.0, 0.0),
            (31.0, 36.0, 0.0, 0.0),
            (31.0, 5.0, 0.0, 0.0),
        ],
    );

    let c1 = b.spawn("citizen", 40, "市民小安", 25.0, 22.5, 0.0, 0.0, 1.0, "npc")?;
    let c2 = b.spawn("citizen", 40, "市民阿宇", 27.0, 22.5, 0.0, 90.0, 1.0, "npc")?;
    b.add_wander_zone("中央公園散步區", 26.0, 22.5, 2.2, 1.0, vec![c1.clone(), c2.clone()], "#34d399");

    // 第三位嫌疑犯：神出鬼沒於運河南岸噴泉旁的「神秘墨鏡男」（真兇）
    let culprit_id = b.spawn("citizen", 40, "神秘墨鏡男", 24.5, 32.5, 0.0, 0.0, 1.0, "suspect")?;
    b.set_tint(&culprit_id, "#334155");

    b.set_interaction(
        &ramen_id,
        json!({
            "kind": "clue",
            "title": "拉麵店老闆的目擊證詞",
            "dialogue": "剛才星海珠寶店警報響起！我看到一個穿深灰外套的男人往東南過運河方向跑去，小安和小宇一直待在公園。",
            "clue_text": "【拉麵老闆線索】嫌犯穿深灰深色外套，往運河南岸逃竄！",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &bus_id,
        json!({
            "kind": "clue",
            "title": "101 號巴士司機行車紀錄",
            "dialogue": "我在南側站牌按喇叭時，有個戴墨鏡的傢伙在「市民噴泉」旁鬼鬼祟祟還掉了珠寶收據！",
            "clue_text": "【巴士司機線索】真正歹徒躲在運河南岸「市民噴泉」附近！",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &city_tower_id,
        json!({
            "kind": "support",
            "title": "都會警局偵查支援",
            "dialogue": "已為你同步都市監視器逮捕權限與防身警盾，請小心核對證詞再行逮捕！",
            "clue_text": "【警局支援】切記誤指無辜市民會扣除偵查評價，務必確認位置與衣服特徵！",
            "grant_shield": 1,
            "heal_hp": 1,
            "reward_points": 20,
            "once": true
        }),
    );
    b.set_interaction(
        &c1,
        json!({
            "kind": "suspect",
            "title": "偵訊：市民小安",
            "dialogue": "警官好，我有什麼能幫忙的嗎？",
            "alibi": "我整個下午都在中央公園餵鴿子散步，鐘樓警衛和阿宇都可以作證，我沒離開過公園！",
            "is_culprit": false,
            "wrong_penalty": 20
        }),
    );
    b.set_interaction(
        &c2,
        json!({
            "kind": "suspect",
            "title": "偵訊：市民阿宇",
            "dialogue": "咦？發生珠寶失竊案了嗎？",
            "alibi": "我剛才一直在中央公園長椅區看報紙，還聽到對面咖啡廳的音樂，完全沒過運河橋喔。",
            "is_culprit": false,
            "wrong_penalty": 20
        }),
    );
    b.set_interaction(
        &culprit_id,
        json!({
            "kind": "suspect",
            "title": "偵訊：神秘墨鏡男",
            "dialogue": "哼⋯⋯別擋路，我只是在運河南岸的噴泉邊吹風而已！",
            "alibi": "我⋯⋯我才沒去過珠寶店！腳邊這張珠寶收據只是風吹過來的⋯⋯吧？",
            "is_culprit": true,
            "wrong_penalty": 20
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "都會大偵探：眾多嫌疑犯抓真兇",
        "goal_type": "catch_culprit",
        "target_count": 1,
        "culprit_entity_id": culprit_id,
        "checklist": [],
        "description": "向「深夜拉麵屋」與「101 號城市巴士」蒐集線索、盤查公園與運河南岸的 3 位嫌疑犯並當場指認逮捕真兇！",
        "victory_title": "🚨 神探破案！真兇落網！",
        "victory_message": "經過嚴密線索交叉比對，你成功從嫌疑犯中揪出潛逃至運河南岸的珠寶大盜，都會恢復和平！"
    }));

    b.add_event(
        "夜幕全城霓虹點燈",
        "clock",
        json!({"hour": 18, "minute": 0}),
        vec![
            json!({"type": "emissive_toggle", "tag": "night_light", "on": true}),
            json!({"type": "speech_bubble", "entity_id": hq_id, "text": "都會夜景模式已開啟！", "duration": 4.0}),
        ],
        10.0,
    );
    b.add_event(
        "點擊公車鳴笛",
        "click",
        json!({"entity_id": bus_id}),
        vec![
            json!({"type": "play_sound", "sound": "horn"}),
            json!({"type": "speech_bubble", "entity_id": bus_id, "text": "嘟嘟！下一站：中央公園站", "duration": 3.0}),
        ],
        1.5,
    );
    Ok(())
}

fn build_farm(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    // 農莊小河（y: 30..33）與 2 格寬木橋
    b.fill_rect(0, 30, 40, 33, 0, "water", "none");
    b.fill_rect(26, 4, 38, 15, 2, "grass", "none");
    b.set_road_line(19, 10, 19, 36, "dirt");
    b.set_road_line(20, 10, 20, 36, "dirt");
    b.set_road_line(8, 22, 33, 22, "dirt");

    let barn_id = b.spawn("barn", 42, "紅色大穀倉", 14.0, 11.0, 0.0, 0.0, 1.15, "landmark")?;
    let windmill_id = b.spawn("windmill", 48, "荷蘭風車塔", 31.5, 9.5, 0.0, 0.0, 1.15, "landmark")?;
    b.spawn("cottage", 40, "農莊木屋", 25.0, 20.0, 0.0, -90.0, 0.95, "home")?;
    let fire_id = b.spawn("campfire", 32, "溪畔營火", 11.0, 27.0, 0.0, 0.0, 0.95, "night_light")?;

    // 麥田與向日葵花圃
    for fx in [6.0, 9.0, 12.0, 6.0, 9.0, 12.0] {
        let fy = if fx < 10.0 { 19.0 } else { 20.0 };
        b.spawn("wheat", 24, "黃金稻麥", fx, fy, 0.0, 0.0, 1.0, "crop")?;
    }
    for (sx, sy) in [(27.0, 24.5), (29.5, 24.5), (32.0, 24.5), (28.0, 27.0), (31.0, 27.0)] {
        b.spawn("sunflower", 34, "向日葵", sx, sy, 0.0, 0.0, 0.95, "crop")?;
    }
    for (tx, ty) in [(4.0, 6.0), (8.0, 7.0), (34.0, 18.0), (36.0, 27.0)] {
        b.spawn("oak", 42, "農莊大橡樹", tx, ty, 0.0, 0.0, 1.05, "tree")?;
    }

    let farmer_id = b.spawn("farmer", 40, "農夫伯伯", 19.5, 14.0, 0.0, 0.0, 1.0, "npc")?;
    b.add_path(
        "農夫巡田路線",
        &farmer_id,
        "pingpong",
        1.6,
        true,
        "#fb923c",
        vec![
            (19.5, 14.0, 0.0, 2.0),
            (19.5, 22.0, 0.0, 1.0),
            (10.0, 22.0, 0.0, 3.0),
            (28.0, 22.0, 0.0, 2.0),
        ],
    );

    let sh1 = b.spawn("sheep", 28, "綿羊咩咩", 28.0, 36.0, 0.0, 0.0, 1.0, "animal")?;
    let sh2 = b.spawn("sheep", 28, "綿羊泡泡", 30.5, 36.8, 0.0, 90.0, 0.9, "animal")?;
    let dog = b.spawn("shiba", 30, "看門柴犬", 26.8, 36.2, 0.0, 45.0, 1.0, "animal")?;
    b.add_wander_zone("河岸青草牧場", 29.0, 36.4, 2.4, 1.3, vec![sh1, sh2, dog.clone()], "#4ade80");

    let duck = b.spawn("duck", 24, "小河白鴨", 10.0, 31.5, 0.1, 90.0, 1.0, "animal")?;
    b.add_path(
        "河面戲水路線",
        &duck,
        "pingpong",
        1.1,
        true,
        "#38bdf8",
        vec![(4.0, 31.5, 0.1, 1.5), (17.0, 31.5, 0.1, 1.5)],
    );

    b.add_event(
        "點擊穀倉開啟豐收慶祝",
        "click",
        json!({"entity_id": barn_id}),
        vec![
            json!({"type": "particle_burst", "effect": "sparkle", "x": 14.0, "y": 11.0, "z": 4.0, "count": 24}),
            json!({"type": "play_sound", "sound": "bird"}),
            json!({"type": "speech_bubble", "entity_id": barn_id, "text": "今年稻麥大豐收！", "duration": 4.0}),
        ],
        2.0,
    );

    let thief_id = b.spawn("citizen", 40, "偷菜蒙面賊", 15.5, 23.5, 0.0, 180.0, 1.0, "robber")?;
    b.set_tint(&thief_id, "#475569");

    b.set_interaction(
        &farmer_id,
        json!({
            "kind": "clue",
            "title": "農夫伯伯的尋寶備忘錄",
            "dialogue": "我把 3 份秋收秘寶藏在「紅色大穀倉」、「荷蘭風車塔」與「溪畔營火」，先過河去南岸找看門柴犬幫忙再尋寶吧！",
            "clue_text": "【農夫線索】三樣農莊秘寶分別在紅色大穀倉、荷蘭風車塔、溪畔營火。",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &dog,
        json!({
            "kind": "support",
            "title": "看門柴犬的勇敢護衛",
            "dialogue": "汪！小主人，柴犬幫你準備好護盾！遇到泥土路南側偷菜賊時可以一聲吼退牠！",
            "clue_text": "【柴犬支援】獲得忠犬護盾 +1，可安心擊退農莊偷菜賊。",
            "grant_shield": 1,
            "heal_hp": 2,
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &thief_id,
        json!({
            "kind": "robbery",
            "title": "⚠️ 偷菜蒙面賊攔住農莊小徑！",
            "dialogue": "這些香甜玉米歸我啦！不想挨揍就把積分留下！",
            "auto_trigger_radius": 1.9,
            "demand_points": 15,
            "hp_damage": 1,
            "allow_shield_counter": true,
            "hide_on_defeat": true,
            "question": "面對偷菜賊，該怎麼機智驅離他？",
            "options": ["A. 吹響農莊牧犬哨嚇跑他", "B. 把籃子裡的玉米雙手奉上"],
            "correct_index": 0,
            "reward_points": 35,
            "reward_item": "追回的金玉米",
            "success_text": "好樣的！你成功趕跑偷菜賊並贏得 35 分！",
            "fail_text": "不小心讓他溜走並損失 15 分，快去找柴犬拿護盾再教訓他！",
            "once": true
        }),
    );
    b.set_interaction(
        &barn_id,
        json!({
            "kind": "treasure",
            "title": "尋獲秘寶：黃金小麥禮籃",
            "dialogue": "在紅色大穀倉乾草堆深處發現閃閃發光的「黃金小麥禮籃」！",
            "required_item": "",
            "success_text": "成功收藏「黃金小麥禮籃」，農莊寶藏 +1！",
            "reward_points": 30,
            "reward_item": "黃金小麥禮籃",
            "once": true
        }),
    );
    b.set_interaction(
        &windmill_id,
        json!({
            "kind": "treasure",
            "title": "尋獲秘寶：風車能量徽章",
            "dialogue": "登上東北高地荷蘭風車塔，風車工匠留下一枚「風車能量徽章」！",
            "required_item": "",
            "success_text": "獲得「風車能量徽章」，農莊寶藏 +1！",
            "reward_points": 30,
            "reward_item": "風車能量徽章",
            "once": true
        }),
    );
    b.set_interaction(
        &fire_id,
        json!({
            "kind": "treasure",
            "title": "尋獲秘寶：烤玉米食譜手札",
            "dialogue": "在溫暖溪畔營火旁的木箱裡找到祖傳「烤玉米食譜手札」！",
            "required_item": "",
            "success_text": "獲得「烤玉米食譜手札」，農莊寶藏 +1！",
            "reward_points": 30,
            "reward_item": "烤玉米食譜手札",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "陽光農莊尋寶季：找回 3 份秋收秘寶",
        "goal_type": "find_treasure",
        "target_count": 3,
        "culprit_entity_id": "",
        "checklist": [],
        "description": "向農夫伯伯打聽位置、找柴犬領取護盾支援，找出「紅色大穀倉」、「荷蘭風車塔」、「溪畔營火」共 3 份秋收秘寶！",
        "victory_title": "🌾 秋收大豐收！農莊尋寶達人！",
        "victory_message": "你找齊了農莊 3 份秘寶並守護牧場平安，獲得全村熱烈掌聲！"
    }));
    Ok(())
}

fn build_campus(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    // 操場紅磚跑道外框（x:10..30, y:23..35）
    b.fill_rect(10, 23, 30, 35, 1, "brick", "none");
    b.fill_rect(13, 25, 27, 33, 1, "grass", "none");
    // 櫻花校道
    b.set_road_line(20, 13, 20, 23, "stone");
    b.set_road_line(6, 20, 34, 20, "stone");

    let school_id = b.spawn("school", 44, "希望小學教學樓", 20.0, 9.5, 0.0, 0.0, 1.25, "landmark")?;
    let tower_id = b.spawn("clock_tower", 44, "校園鐘樓", 9.0, 10.5, 0.0, 0.0, 0.95, "clock")?;
    b.spawn("bench", 20, "操場長椅", 14.0, 22.0, 0.0, 0.0, 1.0, "prop")?;
    b.spawn("bench", 20, "操場長椅", 26.0, 22.0, 0.0, 0.0, 1.0, "prop")?;

    for lx in [17.0, 23.0, 10.0, 30.0] {
        b.spawn("lamp", 34, "校園路燈", lx, 19.0, 0.0, 0.0, 0.9, "night_light")?;
    }
    for (sx, sy) in [
        (16.5, 15.0),
        (23.5, 15.0),
        (16.5, 18.0),
        (23.5, 18.0),
        (6.5, 25.0),
        (33.5, 25.0),
    ] {
        b.spawn("sakura", 42, "校園櫻花樹", sx, sy, 0.0, (sx * 17.0) % 360.0, 0.95, "tree")?;
    }

    let st1 = b.spawn("student", 40, "學生小晴", 20.0, 21.0, 0.0, 180.0, 1.0, "student")?;
    b.add_path(
        "上學走進教室路徑",
        &st1,
        "pingpong",
        1.7,
        true,
        "#f472b6",
        vec![(20.0, 23.0, 0.0, 2.0), (20.0, 13.5, 0.0, 3.0)],
    );
    let st2 = b.spawn("student", 40, "學生阿哲", 17.0, 28.0, 0.0, 0.0, 1.0, "student")?;
    let st3 = b.spawn("student", 40, "學生小美", 23.0, 29.0, 0.0, 90.0, 1.0, "student")?;
    b.add_wander_zone("操場下課自由活動", 20.0, 29.0, 4.5, 1.5, vec![st2.clone(), st3], "#38bdf8");

    b.add_event(
        "早上八點上課鐘響",
        "clock",
        json!({"hour": 8, "minute": 0}),
        vec![
            json!({"type": "play_sound", "sound": "bell"}),
            json!({"type": "speech_bubble", "entity_id": tower_id, "text": "叮咚！早上八點上課時間到囉！", "duration": 5.0}),
        ],
        10.0,
    );
    b.add_event(
        "點擊教學樓觸發櫻花飄落",
        "click",
        json!({"entity_id": school_id}),
        vec![
            json!({"type": "particle_burst", "effect": "sakura", "x": 20.0, "y": 15.0, "z": 5.0, "count": 36}),
            json!({"type": "play_sound", "sound": "chime"}),
        ],
        2.0,
    );

    b.set_interaction(
        &tower_id,
        json!({
            "kind": "support",
            "title": "校園鐘樓的考前加油站",
            "dialogue": "叮咚！這裡有整理好的數理與資訊小筆記，幫你補充精神與滿分信心！",
            "clue_text": "【筆記支援】正多邊形外角和必為 360°、跑道面積要先算長度，加油！",
            "grant_shield": 1,
            "heal_hp": 2,
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &st1,
        json!({
            "kind": "quiz",
            "title": "學生小晴的數學挑戰題",
            "dialogue": "同學，幫我看一下這題操場圍欄計算題對不對吧！",
            "question": "長方形操場長 20 格、寬 12 格，請問沿著操場外圍繞一圈總共有幾格長度？",
            "options": ["A. 64 格", "B. 32 格", "C. 240 格"],
            "correct_index": 0,
            "reward_points": 30,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "數學學霸貼紙",
            "success_text": "太強了！(20 + 12) × 2 = 64 格完全正確！",
            "fail_text": "再試一次喔～周長是兩條長加上兩條寬！",
            "once": true
        }),
    );
    b.set_interaction(
        &st2,
        json!({
            "kind": "quiz",
            "title": "學生阿哲的自然科學問答",
            "dialogue": "櫻花季到了，來考考你對植物與天氣的認識！",
            "question": "植物進行光合作用製造養分時，最主要需要吸收陽光、水分以及哪一種氣體？",
            "options": ["A. 二氧化碳", "B. 氮氣", "C. 氦氣"],
            "correct_index": 0,
            "reward_points": 30,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "自然探索徽章",
            "success_text": "答對了！植物會吸收二氧化碳並釋放清新的氧氣！",
            "fail_text": "不對喔，提示：我們呼出來的氣體就是植物最喜歡的原料！",
            "once": true
        }),
    );
    b.set_interaction(
        &school_id,
        json!({
            "kind": "quiz",
            "title": "教學樓資優闖關：SQLite 檔案",
            "dialogue": "最後一關！解開這座微縮校園世界底層的資料密碼：",
            "question": "《StackCity RT》是用哪一種單檔關聯式資料庫把貼圖、場景與任務存進同一包分享給好友？",
            "options": ["A. SQLite (.stackworld.db)", "B. 純記事本 .txt", "C. 只存在記憶體裡不存檔"],
            "correct_index": 0,
            "reward_points": 40,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "校園榮譽獎章",
            "success_text": "滿分過關！SQLite 單檔自包含設計讓分享變得超可靠！",
            "fail_text": "再想想看，提示字首是 SQL！",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "青春校園大挑戰：解開 3 道學科難題",
        "goal_type": "solve_puzzles",
        "target_count": 3,
        "culprit_entity_id": "",
        "checklist": [],
        "description": "向「校園鐘樓」領取筆記加油，並完成「學生小晴」、「學生阿哲」與「希望小學教學樓」共 3 道知識考驗！",
        "victory_title": "🎓 全能資優生！校園三關全破！",
        "victory_message": "數學、自然與資訊挑戰通通難不倒你，全校櫻花為你齊聲喝采！"
    }));
    Ok(())
}

fn build_blank(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    b.spawn("oak", 42, "起始大樹", 16.0, 18.0, 0.0, 0.0, 1.0, "tree")?;
    b.spawn("lamp", 34, "起始路燈", 22.0, 20.0, 0.0, 0.0, 0.9, "night_light")?;
    b.set_quest(json!({
        "enabled": false,
        "title": "自創關卡目標",
        "goal_type": "score",
        "target_count": 100,
        "culprit_entity_id": "",
        "checklist": [],
        "description": "在右欄「🏆 任務」開啟目標設計，並點選任意物件指派線索、考驗、搶劫或寶藏！",
        "victory_title": "🎉 恭喜完成關卡目標！",
        "victory_message": "太厲害了！你成功通關這座由創作者親手設計的世界！"
    }));
    Ok(())
}

/// 輔助建構建築內部 L 型立體剖面牆體（北牆 y:0..2、西牆 x:0..2）
pub(super) fn build_interior_walls(b: &mut WorldBuilder<'_>, wall_type: &str) {
    b.fill_rect(0, 0, b.w, 2, 4, wall_type, "none");
    b.fill_rect(0, 0, 2, b.h, 4, wall_type, "none");
}

fn build_harbor(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    b.fill_rect(0, 0, 40, 20, 1, "grass", "none");
    b.fill_rect(0, 18, 40, 22, 1, "sand", "none");
    b.fill_rect(0, 22, 40, 40, 0, "water", "none");
    // 木棧碼頭伸入海灣
    b.fill_rect(18, 21, 22, 31, 1, "sand", "bridge");
    b.set_road_line(8, 15, 32, 15, "stone");
    b.set_road_line(20, 6, 20, 21, "stone");

    let lh_id = b.spawn("lighthouse", 44, "海港引航燈塔", 8.0, 20.0, 0.0, 0.0, 1.15, "beacon")?;
    b.spawn("shop", 40, "港邊海鮮小店", 13.0, 13.0, 0.0, 0.0, 1.0, "shop")?;
    let hut_id = b.spawn("cottage", 40, "漁夫木屋", 27.0, 12.5, 0.0, 0.0, 1.0, "village")?;
    b.spawn("oak", 42, "海風大樹", 33.0, 10.0, 0.0, 0.0, 1.0, "tree")?;

    let ship_id = b.spawn("sailboat", 42, "遠洋貿易帆船", 8.0, 32.0, 0.2, 90.0, 1.1, "boat")?;
    b.add_path(
        "海灣進港航線",
        &ship_id,
        "loop",
        2.0,
        true,
        "#38bdf8",
        vec![(6.0, 32.0, 0.2, 1.0), (20.0, 35.5, 0.2, 2.5), (34.0, 32.0, 0.2, 1.0), (20.0, 33.2, 0.2, 1.0)],
    );
    let gull = b.spawn("bird", 24, "海鷗小藍", 16.0, 24.0, 4.0, 0.0, 1.0, "sky")?;
    b.add_wander_zone("海灣飛翔區", 20.0, 26.0, 8.0, 2.1, vec![gull], "#facc15");

    let c1 = b.spawn("citizen", 40, "港口旅人", 17.0, 16.0, 0.0, 90.0, 1.0, "npc")?;
    let c2 = b.spawn("farmer", 40, "老漁夫", 23.0, 16.0, 0.0, 270.0, 1.0, "npc")?;
    b.add_wander_zone("碼頭廣場散步", 20.0, 16.2, 3.2, 1.2, vec![c1.clone(), c2.clone()], "#4ade80");

    b.add_event(
        "點擊燈塔鳴笛放光",
        "click",
        json!({"entity_id": lh_id}),
        vec![
            json!({"type": "play_sound", "sound": "horn"}),
            json!({"type": "particle_burst", "effect": "sparkle", "x": 8.0, "y": 20.0, "z": 4.5, "count": 26}),
        ],
        2.0,
    );
    b.add_event(
        "旅人與漁夫相遇問好",
        "encounter",
        json!({"entity_a": c1, "entity_b": c2, "radius": 1.8}),
        vec![
            json!({"type": "speech_bubble", "entity_id": c2, "text": "今天海象真好，剛捕到滿滿漁獲！", "duration": 4.0}),
            json!({"type": "play_sound", "sound": "bird"}),
        ],
        8.0,
    );

    let pirate_id = b.spawn("knight", 42, "海盜水手", 24.0, 19.0, 0.0, 180.0, 1.0, "robber")?;
    b.set_tint(&pirate_id, "#7f1d1d");

    b.set_interaction(
        &c2,
        json!({
            "kind": "clue",
            "title": "老漁夫的航海秘圖",
            "dialogue": "海灣流傳著兩大秘寶！第一份藏在西側「海港引航燈塔」裡；而「漁夫木屋」的寶箱鑰匙被東邊沙灘的海盜水手搶走了！",
            "clue_text": "【漁夫線索】燈塔內藏羅盤；擊退沙灘海盜水手拿「沉船秘鑰」即可開木屋寶箱。",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &c1,
        json!({
            "kind": "support",
            "title": "港口旅人的海巡支援",
            "dialogue": "沙灘上有海盜水手出沒，這面橡木海巡盾牌借你防身！",
            "clue_text": "【旅人支援】獲得海巡護盾 +1，可安全奪回沉船秘鑰。",
            "grant_shield": 1,
            "heal_hp": 1,
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &pirate_id,
        json!({
            "kind": "robbery",
            "title": "⚠️ 海盜水手攔住海灣沙灘！",
            "dialogue": "哈哈！想拿走「沉船秘鑰」？先交出你口袋的積分再說！",
            "auto_trigger_radius": 2.0,
            "demand_points": 15,
            "hp_damage": 1,
            "allow_shield_counter": true,
            "hide_on_defeat": true,
            "question": "海盜揮舞繩索衝過來，該如何利用潮汐濕沙反制？",
            "options": ["A. 拉動碼頭纜繩絆住他雙腳", "B. 原地交出積分認輸"],
            "correct_index": 0,
            "reward_points": 35,
            "reward_item": "沉船秘鑰",
            "success_text": "海盜摔個四腳朝天！你順利奪回「沉船秘鑰」！",
            "fail_text": "被海盜撞倒並失血與扣分，快找港口旅人拿護盾再來討回鑰匙！",
            "once": true
        }),
    );
    b.set_interaction(
        &lh_id,
        json!({
            "kind": "treasure",
            "title": "尋獲航海秘寶：星光羅盤",
            "dialogue": "在海港引航燈塔基座暗格發現了傳說中的「星光羅盤」！",
            "required_item": "",
            "success_text": "入手「星光羅盤」與 35 積分！",
            "reward_points": 35,
            "reward_item": "星光羅盤",
            "once": true
        }),
    );
    b.set_interaction(
        &hut_id,
        json!({
            "kind": "treasure",
            "title": "尋獲航海秘寶：沉船珍珠寶箱",
            "dialogue": "漁夫木屋裡鎖著一只厚重的海底珍珠寶箱！",
            "required_item": "沉船秘鑰",
            "locked_text": "寶箱鎖住了！先擊退沙灘東側的「海盜水手」奪回「沉船秘鑰」再來開啟。",
            "success_text": "秘鑰喀噠開啟寶箱！閃耀的「海底珍珠箱」到手囉！",
            "reward_points": 45,
            "reward_item": "海底珍珠箱",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "海港尋寶傳說：尋回 2 件航海秘寶",
        "goal_type": "find_treasure",
        "target_count": 2,
        "culprit_entity_id": "",
        "checklist": [],
        "description": "向「老漁夫」取得線索、找「港口旅人」領取護盾，探索燈塔並從海盜手上奪回「沉船秘鑰」開啟漁夫木屋寶箱！",
        "victory_title": "⚓ 偉大的航海探險家！",
        "victory_message": "你找回兩大航海秘寶並擊退海盜，整個海港燈塔小鎮為你鳴笛祝賀！"
    }));
    Ok(())
}

fn build_palace_interior(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    build_interior_walls(b, "wall_stone");
    // 中央皇家紅毯御道
    b.fill_rect(10, 3, 15, 23, 1, "carpet_red", "none");

    let throne_id = b.spawn("throne", 42, "黃金國王寶座", 12.5, 5.5, 0.0, 0.0, 1.25, "landmark")?;
    b.spawn("chandelier", 34, "皇家水晶吊燈", 12.5, 12.0, 3.2, 0.0, 1.1, "night_light")?;
    b.spawn("chandelier", 34, "大廳水晶吊燈", 12.5, 18.0, 3.2, 0.0, 1.1, "night_light")?;
    for py in [6.0, 11.5, 17.0] {
        b.spawn("pillar", 46, "大理石巨柱", 6.5, py, 0.0, 0.0, 1.05, "pillar")?;
        b.spawn("pillar", 46, "大理石巨柱", 18.5, py, 0.0, 0.0, 1.05, "pillar")?;
    }
    b.spawn("statue", 42, "殿堂騎士雕像", 4.0, 5.0, 0.0, 0.0, 1.0, "prop")?;
    b.spawn("statue", 42, "殿堂守護雕像", 21.0, 5.0, 0.0, 0.0, 1.0, "prop")?;
    let chest_id = b.spawn("chest", 26, "皇家珍寶箱", 4.5, 14.5, 0.0, 90.0, 1.0, "prop")?;

    let knight_id = b.spawn("knight", 42, "御前侍衛長", 9.0, 13.0, 0.0, 0.0, 1.05, "npc")?;
    let mage_id = b.spawn("mage", 42, "宮廷大法師", 16.0, 13.0, 0.0, 180.0, 1.05, "npc")?;
    b.add_path(
        "大殿巡視路線",
        &knight_id,
        "pingpong",
        1.5,
        true,
        "#38bdf8",
        vec![(9.0, 9.0, 0.0, 1.5), (9.0, 19.0, 0.0, 1.5), (15.5, 19.0, 0.0, 1.0)],
    );
    b.add_wander_zone("殿堂法師漫步", 15.0, 15.0, 4.0, 1.0, vec![mage_id.clone()], "#c084fc");

    b.add_event(
        "侍衛長與大法師相遇行禮",
        "encounter",
        json!({"entity_a": knight_id, "entity_b": mage_id, "radius": 1.8}),
        vec![
            json!({"type": "speech_bubble", "entity_id": knight_id, "text": "法師大人，宮殿大廳今日一切安好！", "duration": 4.0}),
            json!({"type": "particle_burst", "effect": "sparkle", "x": 12.5, "y": 14.0, "z": 2.0, "count": 20}),
            json!({"type": "play_sound", "sound": "chime"}),
        ],
        8.0,
    );
    b.add_event(
        "點擊黃金王座賜福",
        "click",
        json!({"entity_id": throne_id}),
        vec![
            json!({"type": "particle_burst", "effect": "firework", "x": 12.5, "y": 5.5, "z": 3.5, "count": 32}),
            json!({"type": "play_sound", "sound": "bell"}),
            json!({"type": "speech_bubble", "entity_id": throne_id, "text": "願星光祝福這座繁榮的王國！", "duration": 4.0}),
        ],
        2.0,
    );

    let envoy_id = b.spawn("citizen", 40, "可疑宮廷使者", 6.0, 19.5, 0.0, 0.0, 1.0, "suspect")?;
    b.set_tint(&envoy_id, "#312e81");

    b.set_interaction(
        &throne_id,
        json!({
            "kind": "clue",
            "title": "王座禦印的現場線索",
            "dialogue": "皇家紅寶石不翼而飛！大殿侍衛長與法師全程在大廳中央公開值勤，反而是西南角出現一位神色慌張的紫衣人物。",
            "clue_text": "【王座線索】侍衛長與大法師有互相見證，請留意殿廳西南角的深色外套訪客。",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &chest_id,
        json!({
            "kind": "clue",
            "title": "珍寶箱撬痕檢驗",
            "dialogue": "珍寶箱鎖孔旁留下紫色披風纖維與泥土腳印，通往西南方 x=6, y=19 一帶！",
            "clue_text": "【寶箱物證】鎖孔旁的深紫布料與可疑宮廷使者衣袖完全吻合！",
            "reward_points": 20,
            "once": true
        }),
    );
    b.set_interaction(
        &knight_id,
        json!({
            "kind": "suspect",
            "title": "詢問：御前侍衛長",
            "dialogue": "大人明察！我一直忠心守護御前走廊。",
            "alibi": "我在紅毯通道巡邏時始終有宮廷大法師與水晶吊燈照看著，絕不可能偷竊！",
            "is_culprit": false,
            "wrong_penalty": 20
        }),
    );
    b.set_interaction(
        &mage_id,
        json!({
            "kind": "suspect",
            "title": "詢問：宮廷大法師",
            "dialogue": "願星辰見證我的清白，寶石的魔力正從西南角落傳來。",
            "alibi": "我一直在東側維護魔法水晶結界，西側珍寶箱那邊靠近不得呀。",
            "is_culprit": false,
            "wrong_penalty": 20
        }),
    );
    b.set_interaction(
        &envoy_id,
        json!({
            "kind": "suspect",
            "title": "盤查：可疑宮廷使者",
            "dialogue": "沒、沒事！我只是迷路走到這裡欣賞大理石柱罷了⋯⋯",
            "alibi": "袖子上的缺角？那是不小心被門口樹枝勾到的，才不是被珍寶箱鎖頭夾破的！",
            "is_culprit": true,
            "wrong_penalty": 20
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "宮殿失竊案：揪出竊走王冠寶石的內賊",
        "goal_type": "catch_culprit",
        "target_count": 1,
        "culprit_entity_id": envoy_id,
        "checklist": [],
        "description": "調查「黃金國王寶座」與「皇家珍寶箱」物證，盤查侍衛長、大法師與宮廷使者後當場逮捕真兇！",
        "victory_title": "👑 皇家密探破案成功！",
        "victory_message": "火眼金睛抓獲偽裝成使者的王冠竊賊，國王親自授予你最高榮譽！"
    }));
    Ok(())
}

fn build_shop_interior(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    build_interior_walls(b, "wall_cream");
    b.fill_rect(2, 2, 10, 8, 1, "wood_floor", "none");

    let counter_id = b.spawn("counter", 28, "店鋪收銀櫃檯", 6.5, 5.5, 0.0, 0.0, 1.15, "shop")?;
    let display_id = b.spawn("display_case", 28, "手工甜點展示櫃", 11.5, 5.5, 0.0, 0.0, 1.1, "shop")?;
    b.spawn("vending", 42, "冰飲自動販賣機", 19.5, 4.0, 0.0, 0.0, 1.05, "night_light")?;
    b.spawn("vending", 42, "零食自動販賣機", 21.5, 4.0, 0.0, 0.0, 1.05, "night_light")?;

    for sx in [10.0, 16.0] {
        for sy in [11.5, 16.5] {
            b.spawn("shelf", 36, "商品陳列貨架", sx, sy, 0.0, 0.0, 1.0, "shop")?;
        }
    }
    b.spawn("potted_plant", 34, "店門口盆栽", 4.5, 20.5, 0.0, 0.0, 1.0, "plant")?;

    let manager_id = b.spawn("citizen", 40, "店長小雅", 6.5, 3.8, 0.0, 0.0, 1.0, "npc")?;
    let c1 = b.spawn("citizen", 40, "顧客阿明", 13.0, 14.0, 0.0, 90.0, 1.0, "npc")?;
    let c2 = b.spawn("student", 40, "放學買零食學生", 18.0, 18.0, 0.0, 0.0, 1.0, "npc")?;
    b.add_wander_zone("貨架走道購物區", 14.5, 14.5, 5.5, 1.1, vec![c1.clone(), c2.clone()], "#38bdf8");

    b.add_event(
        "顧客在店內相遇問候",
        "encounter",
        json!({"entity_a": c1, "entity_b": c2, "radius": 1.6}),
        vec![
            json!({"type": "speech_bubble", "entity_id": c1, "text": "這家店的蛋糕跟飲料超讚！", "duration": 3.5}),
            json!({"type": "play_sound", "sound": "chime"}),
        ],
        8.0,
    );
    b.add_event(
        "點擊收銀台結帳歡迎",
        "click",
        json!({"entity_id": counter_id}),
        vec![
            json!({"type": "play_sound", "sound": "chime"}),
            json!({"type": "speech_bubble", "entity_id": counter_id, "text": "歡迎光臨，請問需要結帳嗎？", "duration": 3.5}),
        ],
        1.5,
    );

    let shoplifter_id = b.spawn("citizen", 40, "超商蒙面搶匪", 19.5, 19.5, 0.0, 180.0, 1.0, "robber")?;
    b.set_tint(&shoplifter_id, "#475569");

    b.set_interaction(
        &manager_id,
        json!({
            "kind": "support",
            "title": "店長小雅的防盜警鈴支援",
            "dialogue": "歡迎成為今日超商值班守護員！先收下防盜警鈴護盾，幫忙櫃檯結帳算數，並提防東南角的蒙面搶匪！",
            "clue_text": "【店長支援】領取警鈴護盾 +1 與業績加分 20 分！",
            "grant_shield": 1,
            "heal_hp": 1,
            "reward_points": 20,
            "once": true
        }),
    );
    b.set_interaction(
        &display_id,
        json!({
            "kind": "clue",
            "title": "甜點展示櫃特價表",
            "dialogue": "今日草莓蛋糕 45 元、宇治抹茶布丁 35 元、冰拿鐵 40 元，記得算對客人找零金額喔！",
            "clue_text": "【價目表】草莓蛋糕 45 元、抹茶布丁 35 元、冰拿鐵 40 元。",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &counter_id,
        json!({
            "kind": "quiz",
            "title": "收銀櫃檯急智找零考驗",
            "dialogue": "有顧客買了草莓蛋糕 (45元) 和抹茶布丁 (35元)，拿一張 100 元鈔票結帳！",
            "question": "請問總共 80 元的商品，收 100 元鈔票應該找給客人多少元？",
            "options": ["A. 找零 20 元", "B. 找零 30 元", "C. 找零 15 元"],
            "correct_index": 0,
            "reward_points": 35,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "金牌收銀徽章",
            "success_text": "叮咚！結帳零失誤，顧客超滿意給了五星好評 (+35分)！",
            "fail_text": "哎呀算錯了！45 + 35 = 80，再重新算算 100 - 80！",
            "once": true
        }),
    );
    b.set_interaction(
        &shoplifter_id,
        json!({
            "kind": "robbery",
            "title": "⚠️ 超商角落冒出蒙面搶匪！",
            "dialogue": "把收銀機裡的積分交出來！別逼我拆貨架！",
            "auto_trigger_radius": 1.9,
            "demand_points": 15,
            "hp_damage": 1,
            "allow_shield_counter": true,
            "hide_on_defeat": true,
            "question": "搶匪背對自動門威嚇，按下哪個按鈕可以瞬間把他鎖住報警？",
            "options": ["A. 按下防盜警鈴並關上電控大門", "B. 請他再多拿幾瓶飲料"],
            "correct_index": 0,
            "reward_points": 40,
            "reward_item": "見義勇為錦旗",
            "success_text": "神勇表現！順利制伏超商搶匪並獲得 40 分！",
            "fail_text": "一時緊張沒按到警鈴被搶走 15 分，快找店長小雅拿護盾再來！",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "便利商店一日店長：累積 80 績分",
        "goal_type": "score",
        "target_count": 80,
        "culprit_entity_id": "",
        "checklist": [],
        "description": "向「店長小雅」領防盜支援、查看「甜點展示櫃」價目、通過「收銀櫃檯」找零考驗並擊退超商搶匪，累積滿 80 分即完成任務！",
        "victory_title": "🛍️ 模範五星超商店長！",
        "victory_message": "顧客滿意結帳加上成功防盜，你是這條街最可靠的明星店長！"
    }));
    Ok(())
}

fn build_factory_interior(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    build_interior_walls(b, "wall_stone");
    b.fill_rect(14, 15, 22, 22, 1, "tile_floor", "none");

    let furn_id = b.spawn("furnace", 42, "核心動力高爐", 6.0, 6.0, 0.0, 0.0, 1.15, "night_light")?;
    for cx in [10.0, 13.0, 16.0] {
        b.spawn("conveyor", 22, "自動輸送帶", cx, 8.0, 0.0, 0.0, 1.05, "machine")?;
    }
    let arm1_id = b.spawn("robot_arm", 42, "1號機械手臂", 11.0, 11.0, 0.0, 180.0, 1.0, "machine")?;
    b.spawn("robot_arm", 42, "2號機械手臂", 15.5, 11.0, 0.0, 180.0, 1.0, "machine")?;
    b.spawn("crate", 28, "原料貨箱堆", 5.0, 16.0, 0.0, 0.0, 1.05, "prop")?;
    let crate2_id = b.spawn("crate", 28, "成品貨箱堆", 20.0, 8.0, 0.0, 45.0, 1.05, "prop")?;

    let fork_id = b.spawn("forklift", 34, "智慧堆高機", 10.0, 18.0, 0.0, 90.0, 1.0, "vehicle")?;
    b.add_path(
        "堆高機搬運路線",
        &fork_id,
        "loop",
        2.0,
        false,
        "#facc15",
        vec![(7.5, 18.0, 0.0, 1.5), (18.5, 18.0, 0.0, 1.5), (18.5, 12.5, 0.0, 1.0), (7.5, 12.5, 0.0, 1.0)],
    );
    let worker_id = b.spawn("citizen", 40, "工程師阿傑", 16.0, 16.0, 0.0, 0.0, 1.0, "npc")?;
    b.add_wander_zone("廠區巡檢範圍", 15.0, 16.0, 4.0, 1.1, vec![worker_id.clone()], "#fb923c");

    b.add_event(
        "堆高機遇見工程師鳴笛",
        "encounter",
        json!({"entity_a": fork_id, "entity_b": worker_id, "radius": 2.0}),
        vec![
            json!({"type": "play_sound", "sound": "horn"}),
            json!({"type": "speech_bubble", "entity_id": fork_id, "text": "嗶嗶！堆高機通行中，請注意安全！", "duration": 3.5}),
        ],
        8.0,
    );
    b.add_event(
        "點擊高爐檢視運轉",
        "click",
        json!({"entity_id": furn_id}),
        vec![
            json!({"type": "particle_burst", "effect": "sparkle", "x": 6.0, "y": 6.0, "z": 3.0, "count": 24}),
            json!({"type": "speech_bubble", "entity_id": furn_id, "text": "高爐火力 100% 穩定運轉中！", "duration": 3.5}),
        ],
        1.5,
    );

    b.set_interaction(
        &worker_id,
        json!({
            "kind": "support",
            "title": "工程師阿傑的巡檢手冊",
            "dialogue": "產線安全第一！高爐標準溫度為 800°C 內，輸送帶必須先放原料再加工包裝。這套安全護具請穿好！",
            "clue_text": "【工安提示】高爐警報優先洩壓檢查；流程順序：上料→焊接→包裝。",
            "grant_shield": 1,
            "heal_hp": 1,
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &crate2_id,
        json!({
            "kind": "clue",
            "title": "成品貨箱出貨標籤",
            "dialogue": "箱子上印著本週產線 KPI：所有安全檢查通過即可即時啟動全自動化出貨！",
            "clue_text": "【出貨紀錄】待校驗「核心動力高爐」與「1號機械手臂」兩項排程設定。",
            "reward_points": 10,
            "once": true
        }),
    );
    b.set_interaction(
        &furn_id,
        json!({
            "kind": "quiz",
            "title": "高爐壓力異常排除考驗",
            "dialogue": "核心動力高爐錶頭跳出黃色警示，該如何處置才安全？",
            "question": "偵測到高爐爐壓略微偏高時，標準安全操作應該怎麼做？",
            "options": ["A. 調整調壓洩壓閥並監測溫度", "B. 直接把安全閥焊死不理它"],
            "correct_index": 0,
            "reward_points": 40,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "高爐合格認證章",
            "success_text": "完美排除異常！核心高爐恢復綠色滿載運轉！",
            "fail_text": "危險操作！提示：應該先適度洩壓降溫喔！",
            "once": true
        }),
    );
    b.set_interaction(
        &arm1_id,
        json!({
            "kind": "quiz",
            "title": "機械手臂夾爪流程校正",
            "dialogue": "1號機械手臂需要你核對自動化執行順序：",
            "question": "自動化生產線的正確作業步驟順序為何？",
            "options": [
                "A. 原料進料 → 機械加工組裝 → 成品裝箱",
                "B. 先密封紙箱 → 再塞進還沒加工的原料"
            ],
            "correct_index": 0,
            "reward_points": 40,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "自動產線通行證",
            "success_text": "手臂同步率達 100%！精準度無懈可擊！",
            "fail_text": "順序顛倒了喔，再看一次生產線方向吧！",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "智慧工廠巡檢員：排除 2 項產線難題",
        "goal_type": "solve_puzzles",
        "target_count": 2,
        "culprit_entity_id": "",
        "checklist": [],
        "description": "向「工程師阿傑」取得巡檢支援，完成「核心動力高爐」與「1號機械手臂」共 2 項技術校驗！",
        "victory_title": "🏭 產線全自動化啟動成功！",
        "victory_message": "工廠產線零故障高效運作，你成為智慧製造工廠的黃金總監！"
    }));
    Ok(())
}

fn build_office_interior(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    build_interior_walls(b, "wall_glass");
    b.fill_rect(14, 13, 22, 22, 1, "tile_floor", "none");

    for (dx, dy) in [(6.0, 7.0), (11.0, 7.0), (6.0, 12.5), (11.0, 12.5)] {
        b.spawn("office_desk", 30, "電腦辦公桌", dx, dy, 0.0, 0.0, 1.0, "night_light")?;
        b.spawn("office_chair", 32, "辦公椅", dx, dy - 1.5, 0.0, 0.0, 1.0, "prop")?;
    }
    let mt_id = b.spawn("meeting_table", 22, "中央會議桌", 18.0, 17.5, 0.0, 0.0, 1.15, "prop")?;
    b.spawn("bookshelf", 40, "檔案資料書櫃", 5.5, 3.2, 0.0, 0.0, 1.0, "prop")?;
    b.spawn("bookshelf", 40, "技術圖書櫃", 9.5, 3.2, 0.0, 0.0, 1.0, "prop")?;
    let cooler_id = b.spawn("water_cooler", 36, "茶水間飲水機", 20.5, 4.5, 0.0, 0.0, 1.0, "prop")?;
    b.spawn("potted_plant", 34, "辦公室盆栽", 21.0, 11.0, 0.0, 0.0, 1.0, "plant")?;

    let w1 = b.spawn("citizen", 40, "設計師小凱", 15.0, 10.0, 0.0, 0.0, 1.0, "npc")?;
    let w2 = b.spawn("citizen", 40, "工程師莉莉", 19.0, 10.0, 0.0, 180.0, 1.0, "npc")?;
    b.add_wander_zone("辦公與茶水區", 16.5, 11.0, 4.5, 1.0, vec![w1.clone(), w2.clone()], "#38bdf8");

    b.add_event(
        "同事相遇討論企劃",
        "encounter",
        json!({"entity_a": w1, "entity_b": w2, "radius": 1.6}),
        vec![
            json!({"type": "speech_bubble", "entity_id": w1, "text": "等一下三點我們一起在會議桌對進度吧！", "duration": 4.0}),
            json!({"type": "play_sound", "sound": "chime"}),
        ],
        8.0,
    );
    b.add_event(
        "點擊會議桌展開腦力激盪",
        "click",
        json!({"entity_id": mt_id}),
        vec![
            json!({"type": "particle_burst", "effect": "sparkle", "x": 18.0, "y": 17.5, "z": 2.0, "count": 20}),
            json!({"type": "speech_bubble", "entity_id": mt_id, "text": "靈感大爆發！專案順利推進！", "duration": 3.5}),
        ],
        1.5,
    );

    b.set_interaction(
        &w1,
        json!({
            "kind": "clue",
            "title": "設計師小凱的設計稿簽核",
            "dialogue": "介面視覺規範都定案囉！拿著這份定稿卡去找「工程師莉莉」測試效能，再到「中央會議桌」簡報即可結案！",
            "clue_text": "【設計師進度】UI 設計稿已完成定案 (ui_ready ✓)。",
            "reward_points": 25,
            "reward_item": "UI 定案藍圖",
            "reward_flag": "ui_ready",
            "once": true
        }),
    );
    b.set_interaction(
        &cooler_id,
        json!({
            "kind": "support",
            "title": "茶水間能量咖啡補給",
            "dialogue": "來杯現磨手沖冰咖啡提振專注力吧！滿血繼續攻克技術難關！",
            "clue_text": "【茶水間補給】工程師最愛 WebGL2 實例化渲染批次繪製效能優化。",
            "grant_shield": 1,
            "heal_hp": 2,
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &w2,
        json!({
            "kind": "quiz",
            "title": "工程師莉莉的渲染架構抽考",
            "dialogue": "要順利通過技術驗收，來回答這道圖學關鍵題吧：",
            "question": "為何 WebGL2 使用 Instancing（實例化繪製）能大幅提升大量微縮物件的幀率？",
            "options": [
                "A. 一次繪圖呼叫就能批次渲染成百上千相同造型實例",
                "B. 把電腦螢幕解析度強制縮小到 1x1"
            ],
            "correct_index": 0,
            "reward_points": 35,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "效能測試合格章",
            "reward_flag": "perf_ok",
            "success_text": "答對！60fps 穩定輸出認證通過！(perf_ok ✓)",
            "fail_text": "不對喔，提示：重點在於減少 CPU 對 GPU 的 Draw Call 次數！",
            "once": true
        }),
    );
    b.set_interaction(
        &mt_id,
        json!({
            "kind": "treasure",
            "title": "中央會議桌最終專案簡報",
            "dialogue": "所有設計稿與效能報告都準備齊全了，確定展開最終簡報驗收嗎？",
            "required_item": "效能測試合格章",
            "locked_text": "簡報前必須先通過「工程師莉莉」的渲染技術考驗取得「效能測試合格章」喔！",
            "success_text": "簡報大獲好評！專案順利上線並達成結案清單！(demo_done ✓)",
            "reward_points": 45,
            "reward_item": "上線慶功獎盃",
            "reward_flag": "demo_done",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "科技辦公室：完成 3 項專案上線清單",
        "goal_type": "custom_checklist",
        "target_count": 3,
        "culprit_entity_id": "",
        "checklist": [
            { "key": "ui_ready", "label": "找「設計師小凱」確認設計定稿" },
            { "key": "perf_ok", "label": "通過「工程師莉莉」渲染技術測驗" },
            { "key": "demo_done", "label": "於「中央會議桌」完成最終上線驗收" }
        ],
        "description": "依序完成右上方待辦清單中的 3 項任務指標，順利推動專案準時上線！",
        "victory_title": "💼 專案完美上線！金牌產品經理！",
        "victory_message": "團隊合作無間、設計與效能雙雙滿分，整個辦公室響起歡呼聲！"
    }));
    Ok(())
}

fn build_classroom_interior(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    build_interior_walls(b, "wall_cream");

    let bb_id = b.spawn("blackboard", 36, "教室黑板與講台", 12.5, 4.2, 0.0, 0.0, 1.2, "landmark")?;
    b.spawn("bookshelf", 40, "班級圖書角", 4.5, 4.2, 0.0, 0.0, 1.0, "prop")?;
    b.spawn("potted_plant", 34, "窗邊綠植盆栽", 20.5, 4.5, 0.0, 0.0, 1.0, "plant")?;

    for row_y in [9.5, 14.0, 18.5] {
        for col_x in [7.5, 12.5, 17.5] {
            b.spawn("school_desk", 24, "學生課桌椅", col_x, row_y, 0.0, 180.0, 1.0, "desk")?;
        }
    }

    let s1 = b.spawn("student", 40, "班長小晴", 10.0, 12.0, 0.0, 0.0, 1.0, "student")?;
    let s2 = b.spawn("student", 40, "同學阿哲", 15.0, 16.0, 0.0, 90.0, 1.0, "student")?;
    b.add_wander_zone("教室下課走廊", 12.5, 14.0, 5.0, 1.1, vec![s1.clone(), s2.clone()], "#f472b6");

    b.add_event(
        "同學下課相遇聊天",
        "encounter",
        json!({"entity_a": s1, "entity_b": s2, "radius": 1.6}),
        vec![
            json!({"type": "speech_bubble", "entity_id": s1, "text": "這題數學解法好有趣，我們一起討論！", "duration": 4.0}),
            json!({"type": "play_sound", "sound": "bell"}),
        ],
        8.0,
    );
    b.add_event(
        "點擊黑板複習重點",
        "click",
        json!({"entity_id": bb_id}),
        vec![
            json!({"type": "particle_burst", "effect": "sparkle", "x": 12.5, "y": 4.2, "z": 2.5, "count": 20}),
            json!({"type": "speech_bubble", "entity_id": bb_id, "text": "今日課程主題：3D 精靈堆疊與空間座標！", "duration": 4.0}),
        ],
        1.5,
    );

    b.set_interaction(
        &s1,
        json!({
            "kind": "support",
            "title": "班長小晴的課堂重點分享",
            "dialogue": "快來看講台黑板！三角形內角和 180°，正方體共有 6 個面，加油拿滿分！",
            "clue_text": "【班長筆記】三角形內角和 180 度；長方體有 6 個面。",
            "grant_shield": 1,
            "heal_hp": 1,
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &bb_id,
        json!({
            "kind": "quiz",
            "title": "黑板隨堂幾何小考",
            "dialogue": "站在講台前回答這題空間幾何題吧！",
            "question": "一個標準正立方體積木共有幾個平坦正方形面？",
            "options": ["A. 6 個面", "B. 4 個面", "C. 8 個面"],
            "correct_index": 0,
            "reward_points": 35,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "幾何滿分貼紙",
            "success_text": "完全正確！正方體上下前後左右共 6 個面！",
            "fail_text": "再數一次看看喔，像骰子一樣有 1 到 6 點！",
            "once": true
        }),
    );
    b.set_interaction(
        &s2,
        json!({
            "kind": "quiz",
            "title": "同學阿哲的國語成語挑戰",
            "dialogue": "換我出題考考你，答對就可以準時下課囉！",
            "question": "用來形容兩位夥伴互相合作、搭配得非常默契，最適合用哪一個成語？",
            "options": ["A. 相輔相成", "B. 緣木求魚", "C. 畫蛇添足"],
            "correct_index": 0,
            "reward_points": 35,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "成語小達人獎狀",
            "success_text": "太厲害了！答對 100 分，下課鐘聲響起來！",
            "fail_text": "差一點，提示是彼此幫助、共同成就！",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "陽光教室挑戰賽：通過 2 道隨堂測驗",
        "goal_type": "solve_puzzles",
        "target_count": 2,
        "culprit_entity_id": "",
        "checklist": [],
        "description": "向「班長小晴」借筆記預習，並完成講台「教室黑板」與「同學阿哲」共 2 題課堂隨堂測驗！",
        "victory_title": "📚 今日課堂第一名！準時下課歡呼！",
        "victory_message": "幾何與成語測驗全都一次高分通關，老師給了你全班最大顆的榮譽星星！"
    }));
    Ok(())
}

fn build_home_interior(b: &mut WorldBuilder<'_>) -> Result<(), String> {
    build_interior_walls(b, "wall_cream");
    // 廚房地磚區與客廳地毯
    b.fill_rect(14, 2, 23, 10, 1, "tile_floor", "none");
    b.fill_rect(4, 13, 12, 20, 1, "carpet_red", "none");

    let tv_id = b.spawn("tv_stand", 32, "客廳大電視", 8.0, 12.0, 0.0, 180.0, 1.05, "night_light")?;
    b.spawn("sofa", 26, "柔軟布藝沙發", 8.0, 18.0, 0.0, 0.0, 1.05, "prop")?;
    b.spawn("dining_table", 22, "餐廳圓桌", 18.0, 15.0, 0.0, 0.0, 1.05, "prop")?;
    b.spawn("kitchen_stove", 28, "廚房流理台", 18.5, 4.5, 0.0, 0.0, 1.1, "night_light")?;
    b.spawn("bed", 24, "舒適雙人床", 6.5, 5.5, 0.0, 0.0, 1.1, "prop")?;
    let shelf_id = b.spawn("bookshelf", 40, "臥室書櫃", 11.5, 3.5, 0.0, 0.0, 0.95, "prop")?;
    b.spawn("potted_plant", 34, "客廳琴葉榕", 4.0, 17.5, 0.0, 0.0, 1.0, "plant")?;

    let dog = b.spawn("shiba", 30, "愛犬柴柴", 11.0, 16.0, 0.0, 90.0, 1.0, "animal")?;
    let host = b.spawn("citizen", 40, "屋主小安", 15.0, 12.0, 0.0, 270.0, 1.0, "npc")?;
    b.add_wander_zone("客廳活動空間", 12.5, 14.5, 4.5, 1.0, vec![dog.clone(), host.clone()], "#4ade80");

    b.add_event(
        "屋主與柴柴開心撒嬌",
        "encounter",
        json!({"entity_a": host, "entity_b": dog, "radius": 1.6}),
        vec![
            json!({"type": "speech_bubble", "entity_id": dog, "text": "汪汪！歡迎回家～", "duration": 3.5}),
            json!({"type": "particle_burst", "effect": "sparkle", "x": 12.0, "y": 14.5, "z": 1.6, "count": 18}),
            json!({"type": "play_sound", "sound": "chime"}),
        ],
        8.0,
    );
    b.add_event(
        "點擊電視切換頻道",
        "click",
        json!({"entity_id": tv_id}),
        vec![
            json!({"type": "play_sound", "sound": "chime"}),
            json!({"type": "speech_bubble", "entity_id": tv_id, "text": "正在播放：《像素疊界》建築大師專題！", "duration": 3.5}),
        ],
        1.5,
    );

    b.set_interaction(
        &host,
        json!({
            "kind": "clue",
            "title": "屋主小安的驚喜尋寶提示",
            "dialogue": "歡迎回家！我在家裡藏了 2 份紀念禮物：一份夾在北側「臥室書櫃」，另一份就在客廳的「客廳大電視」後面喔！",
            "clue_text": "【小安提示】兩份家中驚喜禮物分別在「臥室書櫃」與「客廳大電視」附近。",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &dog,
        json!({
            "kind": "support",
            "title": "愛犬柴柴的溫暖陪伴",
            "dialogue": "汪嗚～柴柴蹭了蹭你並幫你指路！",
            "clue_text": "【柴柴領路】先去北邊臥室書櫃看看吧！",
            "grant_shield": 1,
            "heal_hp": 2,
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &shelf_id,
        json!({
            "kind": "treasure",
            "title": "尋獲家中驚喜：像素建築相冊",
            "dialogue": "在臥室書櫃頂層翻開一本精緻的「像素建築相冊」！",
            "required_item": "",
            "success_text": "珍藏了第一份禮物「像素建築相冊」！",
            "reward_points": 35,
            "reward_item": "像素建築相冊",
            "once": true
        }),
    );
    b.set_interaction(
        &tv_id,
        json!({
            "kind": "treasure",
            "title": "尋獲家中驚喜：黃金遊戲手把",
            "dialogue": "在客廳大電視底座抽屜裡發現閃亮的「黃金遊戲手把」！",
            "required_item": "",
            "success_text": "找到第二份禮物「黃金遊戲手把」，今晚來開同樂派對吧！",
            "reward_points": 35,
            "reward_item": "黃金遊戲手把",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "溫馨住家同樂夜：尋找 2 樣家中驚喜",
        "goal_type": "find_treasure",
        "target_count": 2,
        "culprit_entity_id": "",
        "checklist": [],
        "description": "向「屋主小安」和「愛犬柴柴」拿線索，找出藏在「臥室書櫃」與「客廳大電視」的 2 樣紀念禮物！",
        "victory_title": "🛋️ 家庭派對狂歡夜圓滿達成！",
        "victory_message": "相冊與黃金手把通通集齊，屋主小安與柴柴在客廳歡呼開派對啦！"
    }));
    Ok(())
}

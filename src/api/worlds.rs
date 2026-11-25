//! 世界清單、地形、實體、路徑、漫遊區、事件規則與快照 CRUD API。

use super::{ApiError, AppState};
use crate::db::{
    self,
    models::{EventRule, MovementPath, TerrainTile, WanderZone, Waypoint, WorldEntity},
    presets::create_world_from_preset,
};
use axum::{
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use uuid::Uuid;

/// 區分 JSON 欄位「未提供 (`None`)」與「明確傳入 `null` (`Some(None)`)」，確保解除附掛與清除進度正確寫入
fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

pub async fn list_worlds(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let worlds = db::list_worlds(&st.data_dir).map_err(ApiError::internal)?;
    Ok(Json(json!({ "worlds": worlds })))
}

#[derive(Deserialize)]
pub struct CreateWorldReq {
    pub name: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_author")]
    pub author: String,
}
fn default_theme() -> String {
    "kingdom".to_string()
}
fn default_author() -> String {
    "像素建築師".to_string()
}

pub async fn create_world(
    State(st): State<AppState>,
    Json(req): Json<CreateWorldReq>,
) -> Result<Json<Value>, ApiError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_req("請輸入世界名稱。"));
    }
    let summary = create_world_from_preset(&st.data_dir, &req.theme, name, &req.author, None)
        .map_err(ApiError::bad_req)?;
    Ok(Json(json!({ "world": summary })))
}

pub async fn delete_world(
    State(st): State<AppState>,
    Path(file): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    if !path.exists() {
        return Err(ApiError::not_found("找不到指定的世界檔案。"));
    }
    fs::remove_file(&path).map_err(|e| ApiError::internal(format!("刪除世界失敗：{e}")))?;
    // 若殘留 WAL/SHM 暫存檔一併清理
    for ext in ["-wal", "-shm"] {
        let aux = format!("{}{ext}", path.to_string_lossy());
        let _ = fs::remove_file(aux);
    }

    // 若這是最後一個世界，自動重建預設示範世界以維持系統可用狀態
    let mut worlds = db::list_worlds(&st.data_dir).map_err(ApiError::internal)?;
    let mut recreated_default = false;
    if worlds.is_empty() {
        db::ensure_data_dir_initialized(&st.data_dir).map_err(ApiError::internal)?;
        worlds = db::list_worlds(&st.data_dir).map_err(ApiError::internal)?;
        recreated_default = true;
    }

    Ok(Json(json!({
        "ok": true,
        "recreated_default": recreated_default,
        "worlds": worlds
    })))
}

pub async fn get_world_state(
    State(st): State<AppState>,
    Path(file): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    if !path.exists() {
        return Err(ApiError::not_found("找不到指定的世界檔案。"));
    }
    let safe_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&file)
        .to_string();
    let full = db::load_full_world_state(&path, &safe_name).map_err(ApiError::internal)?;
    Ok(Json(serde_json::to_value(full).map_err(|e| ApiError::internal(e.to_string()))?))
}

#[derive(Deserialize)]
pub struct UpdateMetaReq {
    pub name: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub theme: Option<String>,
    pub time_of_day: Option<f64>,
    pub time_speed: Option<f64>,
    pub weather: Option<String>,
    pub bgm_preset: Option<String>,
    pub bgm_volume: Option<f64>,
    pub player_asset_id: Option<String>,
    pub player_name: Option<String>,
    pub player_scale: Option<f64>,
    pub player_x: Option<f64>,
    pub player_y: Option<f64>,
    pub quest_config: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub quest_progress: Option<Option<Value>>,
    pub thumbnail_b64: Option<String>,
}

pub async fn update_world_meta(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<UpdateMetaReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;

    if let Some(v) = req.name {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err(ApiError::bad_req("世界名稱不可空白。"));
        }
        db::set_meta_kv(&conn, "name", trimmed).map_err(ApiError::internal)?;
    }
    if let Some(v) = req.author {
        db::set_meta_kv(&conn, "author", v.trim()).map_err(ApiError::internal)?;
    }
    if let Some(v) = req.description {
        db::set_meta_kv(&conn, "description", v.trim()).map_err(ApiError::internal)?;
    }
    if let Some(v) = req.theme {
        db::set_meta_kv(&conn, "theme", v.trim()).map_err(ApiError::internal)?;
    }
    if let Some(v) = req.time_of_day {
        db::set_meta_kv(&conn, "time_of_day", &v.rem_euclid(24.0).to_string())
            .map_err(ApiError::internal)?;
    }
    if let Some(v) = req.time_speed {
        db::set_meta_kv(&conn, "time_speed", &v.clamp(0.0, 20.0).to_string())
            .map_err(ApiError::internal)?;
    }
    if let Some(v) = req.weather {
        db::set_meta_kv(&conn, "weather", v.trim()).map_err(ApiError::internal)?;
    }
    if let Some(v) = req.bgm_preset {
        db::set_meta_kv(&conn, "bgm_preset", v.trim()).map_err(ApiError::internal)?;
    }
    if let Some(v) = req.bgm_volume {
        db::set_meta_kv(&conn, "bgm_volume", &v.clamp(0.0, 1.0).to_string())
            .map_err(ApiError::internal)?;
    }
    if let Some(v) = req.player_asset_id {
        db::set_meta_kv(&conn, "player_asset_id", v.trim()).map_err(ApiError::internal)?;
    }
    if let Some(v) = req.player_name {
        let trimmed = v.trim();
        let final_name = if trimmed.is_empty() { "冒險玩家" } else { trimmed };
        db::set_meta_kv(&conn, "player_name", final_name).map_err(ApiError::internal)?;
    }
    if let Some(v) = req.player_scale {
        db::set_meta_kv(&conn, "player_scale", &v.clamp(0.3, 3.0).to_string())
            .map_err(ApiError::internal)?;
    }
    if let Some(v) = req.player_x {
        db::set_meta_kv(&conn, "player_x", &v.to_string()).map_err(ApiError::internal)?;
    }
    if let Some(v) = req.player_y {
        db::set_meta_kv(&conn, "player_y", &v.to_string()).map_err(ApiError::internal)?;
    }
    if let Some(qc) = req.quest_config {
        db::set_meta_kv(&conn, "quest_config_json", &qc.to_string()).map_err(ApiError::internal)?;
    }
    if let Some(qp_opt) = req.quest_progress {
        match qp_opt {
            Some(qp) => {
                db::set_meta_kv(&conn, "quest_progress_json", &qp.to_string())
                    .map_err(ApiError::internal)?;
            }
            None => {
                conn.execute("DELETE FROM world_meta WHERE key = 'quest_progress_json'", [])
                    .map_err(|e| ApiError::internal(format!("清除任務進度失敗：{e}")))?;
            }
        }
    }
    if let Some(v) = req.thumbnail_b64 {
        db::set_meta_kv(&conn, "thumbnail_b64", &v).map_err(ApiError::internal)?;
    }
    db::touch_world_updated(&conn);
    let meta = db::read_world_meta(&conn).map_err(ApiError::internal)?;
    Ok(Json(json!({ "meta": meta })))
}

#[derive(Deserialize)]
pub struct ResizeWorldReq {
    #[serde(default)]
    pub add_north: i32,
    #[serde(default)]
    pub add_south: i32,
    #[serde(default)]
    pub add_west: i32,
    #[serde(default)]
    pub add_east: i32,
    #[serde(default = "default_fill_ground")]
    pub fill_ground: String,
    #[serde(default = "default_fill_elev")]
    pub fill_elevation: i32,
}
fn default_fill_ground() -> String {
    "grass".to_string()
}
fn default_fill_elev() -> i32 {
    1
}

pub async fn resize_world_handler(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<ResizeWorldReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let meta = db::resize_world(
        &path,
        req.add_north,
        req.add_south,
        req.add_west,
        req.add_east,
        &req.fill_ground,
        req.fill_elevation,
    )
    .map_err(ApiError::bad_req)?;
    Ok(Json(json!({ "meta": meta })))
}

pub async fn upload_world_bgm(
    State(st): State<AppState>,
    Path(file): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut name = "custom_bgm.mp3".to_string();
    let mut mime = "audio/mpeg".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_req(format!("讀取音檔表單失敗：{e}")))?
    {
        if field.name() == Some("file") {
            if let Some(fn_str) = field.file_name() {
                name = fn_str.to_string();
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".wav") {
                    mime = "audio/wav".to_string();
                } else if lower.ends_with(".ogg") {
                    mime = "audio/ogg".to_string();
                } else if lower.ends_with(".m4a") {
                    mime = "audio/mp4".to_string();
                }
            }
            if let Some(ct) = field.content_type() {
                if ct.starts_with("audio/") {
                    mime = ct.to_string();
                }
            }
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad_req(format!("讀取音檔位元失敗：{e}")))?
                    .to_vec(),
            );
        }
    }

    let bytes = file_bytes.ok_or_else(|| ApiError::bad_req("請選擇要上傳的背景音樂檔案（.mp3 / .wav / .ogg / .m4a）。"))?;
    if bytes.is_empty() || bytes.len() > 14 * 1024 * 1024 {
        return Err(ApiError::bad_req("音檔大小不可為空或超過 14 MB。"));
    }

    db::save_world_bgm(&conn, &name, &mime, &bytes).map_err(ApiError::internal)?;
    let meta = db::read_world_meta(&conn).map_err(ApiError::internal)?;
    Ok(Json(json!({ "meta": meta })))
}

pub async fn get_world_bgm(
    State(st): State<AppState>,
    Path(file): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let Some((_name, mime, bytes)) = db::read_world_bgm(&conn).map_err(ApiError::internal)? else {
        return Err(ApiError::not_found("此世界尚未上傳自訂背景音樂。"));
    };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        bytes,
    ))
}

pub async fn delete_world_bgm_handler(
    State(st): State<AppState>,
    Path(file): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    db::delete_world_bgm(&conn).map_err(ApiError::internal)?;
    let meta = db::read_world_meta(&conn).map_err(ApiError::internal)?;
    Ok(Json(json!({ "meta": meta })))
}

#[derive(Deserialize)]
pub struct BatchTerrainReq {
    pub tiles: Vec<TerrainTile>,
}

pub async fn batch_update_terrain(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<BatchTerrainReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let mut conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let tx = conn
        .transaction()
        .map_err(|e| ApiError::internal(format!("開啟地形交易失敗：{e}")))?;

    for t in &req.tiles {
        let elev = t.elevation.clamp(0, 6);
        tx.execute(
            "INSERT INTO terrain_tiles (x, y, elevation, ground_type, road_type, custom_color)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(x, y) DO UPDATE SET
               elevation = excluded.elevation,
               ground_type = excluded.ground_type,
               road_type = excluded.road_type,
               custom_color = excluded.custom_color",
            params![t.x, t.y, elev, t.ground_type, t.road_type, t.custom_color],
        )
        .map_err(|e| ApiError::internal(format!("更新地形格點失敗：{e}")))?;
    }
    tx.commit()
        .map_err(|e| ApiError::internal(format!("提交地形更新失敗：{e}")))?;
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "updated": req.tiles.len() })))
}

#[derive(Deserialize)]
pub struct CreateEntityItem {
    pub asset_id: String,
    pub name: Option<String>,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub z: f64,
    #[serde(default)]
    pub yaw: f64,
    #[serde(default = "default_one")]
    pub scale: f64,
    pub scale_x: Option<f64>,
    pub scale_y: Option<f64>,
    pub scale_z: Option<f64>,
    pub parent_id: Option<String>,
    #[serde(default = "default_tint")]
    pub tint_hex: String,
    #[serde(default = "default_true")]
    pub emissive_on: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_anim")]
    pub anim_mode: String,
    #[serde(default)]
    pub tag: String,
    #[serde(default = "default_meta_json")]
    pub meta_json: String,
}
fn default_one() -> f64 {
    1.0
}
fn default_tint() -> String {
    "#ffffff".to_string()
}
fn default_true() -> bool {
    true
}
fn default_anim() -> String {
    "auto".to_string()
}
fn default_meta_json() -> String {
    "{}".to_string()
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum CreateEntitiesPayload {
    Many { entities: Vec<CreateEntityItem> },
    One(CreateEntityItem),
}

pub async fn create_entities(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(payload): Json<CreateEntitiesPayload>,
) -> Result<Json<Value>, ApiError> {
    let items = match payload {
        CreateEntitiesPayload::Many { entities } => entities,
        CreateEntitiesPayload::One(one) => vec![one],
    };
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let mut conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let tx = conn
        .transaction()
        .map_err(|e| ApiError::internal(format!("開啟實體交易失敗：{e}")))?;

    let mut created = Vec::with_capacity(items.len());
    for item in items {
        let asset_name: String = tx
            .query_row(
                "SELECT name FROM asset_catalog WHERE id = ?1",
                params![item.asset_id],
                |r| r.get(0),
            )
            .map_err(|_| ApiError::bad_req(format!("找不到資產 ID：{}", item.asset_id)))?;

        let id = Uuid::new_v4().to_string();
        let sx = item.scale_x.unwrap_or(item.scale).clamp(0.15, 8.0);
        let sy = item.scale_y.unwrap_or(item.scale).clamp(0.15, 8.0);
        let sz = item.scale_z.unwrap_or(item.scale).clamp(0.15, 8.0);
        let avg_scale = ((sx + sy + sz) / 3.0).clamp(0.15, 8.0);
        let entity = WorldEntity {
            id: id.clone(),
            asset_id: item.asset_id,
            name: item.name.unwrap_or(asset_name),
            x: item.x,
            y: item.y,
            z: item.z,
            yaw: item.yaw,
            scale: avg_scale,
            scale_x: sx,
            scale_y: sy,
            scale_z: sz,
            parent_id: item.parent_id,
            tint_hex: item.tint_hex,
            emissive_on: item.emissive_on,
            visible: item.visible,
            anim_mode: item.anim_mode,
            tag: item.tag,
            meta_json: item.meta_json,
        };
        tx.execute(
            "INSERT INTO world_entities
             (id, asset_id, name, x, y, z, yaw, scale, scale_x, scale_y, scale_z, parent_id, tint_hex, emissive_on, visible, anim_mode, tag, meta_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                entity.id,
                entity.asset_id,
                entity.name,
                entity.x,
                entity.y,
                entity.z,
                entity.yaw,
                entity.scale,
                entity.scale_x,
                entity.scale_y,
                entity.scale_z,
                entity.parent_id,
                entity.tint_hex,
                entity.emissive_on as i64,
                entity.visible as i64,
                entity.anim_mode,
                entity.tag,
                entity.meta_json
            ],
        )
        .map_err(|e| ApiError::internal(format!("新增物件失敗：{e}")))?;
        created.push(entity);
    }
    tx.commit()
        .map_err(|e| ApiError::internal(format!("提交物件新增失敗：{e}")))?;
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "entities": created })))
}

#[derive(Deserialize)]
pub struct UpdateEntityReq {
    pub name: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub yaw: Option<f64>,
    pub scale: Option<f64>,
    pub scale_x: Option<f64>,
    pub scale_y: Option<f64>,
    pub scale_z: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub parent_id: Option<Option<String>>,
    pub tint_hex: Option<String>,
    pub emissive_on: Option<bool>,
    pub visible: Option<bool>,
    pub anim_mode: Option<String>,
    pub tag: Option<String>,
    pub meta_json: Option<String>,
}

pub async fn update_entity(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
    Json(req): Json<UpdateEntityReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;

    let mut cur: WorldEntity = conn
        .query_row(
            "SELECT id, asset_id, name, x, y, z, yaw, scale,
                    COALESCE(scale_x, scale), COALESCE(scale_y, scale), COALESCE(scale_z, scale),
                    parent_id, tint_hex, emissive_on, visible, anim_mode, tag, meta_json
             FROM world_entities WHERE id = ?1",
            params![id],
            |r| {
                Ok(WorldEntity {
                    id: r.get(0)?,
                    asset_id: r.get(1)?,
                    name: r.get(2)?,
                    x: r.get(3)?,
                    y: r.get(4)?,
                    z: r.get(5)?,
                    yaw: r.get(6)?,
                    scale: r.get(7)?,
                    scale_x: r.get(8)?,
                    scale_y: r.get(9)?,
                    scale_z: r.get(10)?,
                    parent_id: r.get(11)?,
                    tint_hex: r.get(12)?,
                    emissive_on: r.get::<_, i64>(13)? != 0,
                    visible: r.get::<_, i64>(14)? != 0,
                    anim_mode: r.get(15)?,
                    tag: r.get(16)?,
                    meta_json: r.get(17)?,
                })
            },
        )
        .map_err(|_| ApiError::not_found("找不到指定的場景物件。"))?;

    if let Some(v) = req.name {
        cur.name = v;
    }
    if let Some(v) = req.x {
        cur.x = v;
    }
    if let Some(v) = req.y {
        cur.y = v;
    }
    if let Some(v) = req.z {
        cur.z = v;
    }
    if let Some(v) = req.yaw {
        cur.yaw = v;
    }
    let has_axis_update = req.scale_x.is_some() || req.scale_y.is_some() || req.scale_z.is_some();
    if let Some(v) = req.scale {
        let clamped = v.clamp(0.15, 8.0);
        cur.scale = clamped;
        if !has_axis_update {
            cur.scale_x = clamped;
            cur.scale_y = clamped;
            cur.scale_z = clamped;
        }
    }
    if let Some(v) = req.scale_x {
        cur.scale_x = v.clamp(0.15, 8.0);
    }
    if let Some(v) = req.scale_y {
        cur.scale_y = v.clamp(0.15, 8.0);
    }
    if let Some(v) = req.scale_z {
        cur.scale_z = v.clamp(0.15, 8.0);
    }
    if has_axis_update {
        cur.scale = ((cur.scale_x + cur.scale_y + cur.scale_z) / 3.0).clamp(0.15, 8.0);
    }
    if let Some(v) = req.parent_id {
        let cleaned = v
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != &cur.id);
        if let Some(ref pid) = cleaned {
            // 檢查新指定之載體是否存在且不會構成循環附掛鏈
            let mut visited = HashSet::new();
            visited.insert(cur.id.clone());
            let mut cursor: Option<String> = Some(pid.clone());
            for _ in 0..64 {
                let Some(ref cid) = cursor else { break };
                if !visited.insert(cid.clone()) {
                    return Err(ApiError::bad_req("不可建立循環附掛搭乘關係。"));
                }
                let next_parent: Option<Option<String>> = conn
                    .query_row(
                        "SELECT parent_id FROM world_entities WHERE id = ?1",
                        params![cid],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(|e| ApiError::internal(format!("檢查附掛關係失敗：{e}")))?;
                match next_parent {
                    Some(p) => cursor = p.filter(|s| !s.trim().is_empty()),
                    None => {
                        if cid == pid {
                            return Err(ApiError::bad_req("找不到指定的附掛載體物件。"));
                        }
                        break;
                    }
                }
            }
        }
        cur.parent_id = cleaned;
    }
    if let Some(v) = req.tint_hex {
        cur.tint_hex = v;
    }
    if let Some(v) = req.emissive_on {
        cur.emissive_on = v;
    }
    if let Some(v) = req.visible {
        cur.visible = v;
    }
    if let Some(v) = req.anim_mode {
        cur.anim_mode = v;
    }
    if let Some(v) = req.tag {
        cur.tag = v;
    }
    if let Some(v) = req.meta_json {
        cur.meta_json = v;
    }

    conn.execute(
        "UPDATE world_entities SET
          name=?2, x=?3, y=?4, z=?5, yaw=?6, scale=?7, scale_x=?8, scale_y=?9, scale_z=?10,
          parent_id=?11, tint_hex=?12, emissive_on=?13, visible=?14, anim_mode=?15, tag=?16, meta_json=?17
         WHERE id=?1",
        params![
            cur.id,
            cur.name,
            cur.x,
            cur.y,
            cur.z,
            cur.yaw,
            cur.scale,
            cur.scale_x,
            cur.scale_y,
            cur.scale_z,
            cur.parent_id,
            cur.tint_hex,
            cur.emissive_on as i64,
            cur.visible as i64,
            cur.anim_mode,
            cur.tag,
            cur.meta_json
        ],
    )
    .map_err(|e| ApiError::internal(format!("更新物件失敗：{e}")))?;

    db::touch_world_updated(&conn);
    Ok(Json(json!({ "entity": cur })))
}

pub async fn delete_entity(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    // 先解除附掛於此物件的所有子物件，避免殘留無效 parent_id
    conn.execute(
        "UPDATE world_entities SET parent_id = NULL WHERE parent_id = ?1",
        params![id],
    )
    .map_err(|e| ApiError::internal(format!("解除子物件附掛失敗：{e}")))?;
    let changed = conn
        .execute("DELETE FROM world_entities WHERE id = ?1", params![id])
        .map_err(|e| ApiError::internal(format!("刪除物件失敗：{e}")))?;
    if changed == 0 {
        return Err(ApiError::not_found("找不到欲刪除的物件。"));
    }
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct PathReq {
    pub name: String,
    pub entity_id: Option<String>,
    #[serde(default = "default_loop")]
    pub loop_mode: String,
    #[serde(default = "default_speed")]
    pub speed: f64,
    #[serde(default = "default_true")]
    pub smooth: bool,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default = "default_path_color")]
    pub color: String,
    pub waypoints: Vec<Waypoint>,
}
fn default_loop() -> String {
    "loop".to_string()
}
fn default_speed() -> f64 {
    2.0
}
fn default_path_color() -> String {
    "#38bdf8".to_string()
}

pub async fn create_path(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<PathReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let mp = MovementPath {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        entity_id: req.entity_id,
        loop_mode: req.loop_mode,
        speed: req.speed.clamp(0.1, 20.0),
        smooth: req.smooth,
        active: req.active,
        color: req.color,
        waypoints: req.waypoints,
    };
    let wp_json = serde_json::to_string(&mp.waypoints).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO movement_paths (id, name, entity_id, loop_mode, speed, smooth, active, color, waypoints_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            mp.id,
            mp.name,
            mp.entity_id,
            mp.loop_mode,
            mp.speed,
            mp.smooth as i64,
            mp.active as i64,
            mp.color,
            wp_json
        ],
    )
    .map_err(|e| ApiError::internal(format!("新增移動路徑失敗：{e}")))?;
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "path": mp })))
}

pub async fn update_path(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
    Json(req): Json<PathReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let mp = MovementPath {
        id: id.clone(),
        name: req.name,
        entity_id: req.entity_id,
        loop_mode: req.loop_mode,
        speed: req.speed.clamp(0.1, 20.0),
        smooth: req.smooth,
        active: req.active,
        color: req.color,
        waypoints: req.waypoints,
    };
    let wp_json = serde_json::to_string(&mp.waypoints).unwrap_or_else(|_| "[]".into());
    let changed = conn
        .execute(
            "UPDATE movement_paths SET name=?2, entity_id=?3, loop_mode=?4, speed=?5, smooth=?6, active=?7, color=?8, waypoints_json=?9 WHERE id=?1",
            params![
                mp.id,
                mp.name,
                mp.entity_id,
                mp.loop_mode,
                mp.speed,
                mp.smooth as i64,
                mp.active as i64,
                mp.color,
                wp_json
            ],
        )
        .map_err(|e| ApiError::internal(format!("更新移動路徑失敗：{e}")))?;
    if changed == 0 {
        return Err(ApiError::not_found("找不到指定的移動路徑。"));
    }
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "path": mp })))
}

pub async fn delete_path(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    conn.execute("DELETE FROM movement_paths WHERE id = ?1", params![id])
        .map_err(|e| ApiError::internal(format!("刪除移動路徑失敗：{e}")))?;
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct WanderZoneReq {
    pub name: String,
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    #[serde(default = "default_wander_speed")]
    pub speed: f64,
    #[serde(default)]
    pub entity_ids: Vec<String>,
    #[serde(default = "default_zone_color")]
    pub color: String,
}
fn default_wander_speed() -> f64 {
    1.2
}
fn default_zone_color() -> String {
    "#4ade80".to_string()
}

pub async fn create_wander_zone(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<WanderZoneReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let wz = WanderZone {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        cx: req.cx,
        cy: req.cy,
        radius: req.radius.clamp(1.0, 30.0),
        speed: req.speed.clamp(0.1, 10.0),
        entity_ids: req.entity_ids,
        color: req.color,
    };
    let ids_json = serde_json::to_string(&wz.entity_ids).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO wander_zones (id, name, cx, cy, radius, speed, entity_ids_json, color)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![wz.id, wz.name, wz.cx, wz.cy, wz.radius, wz.speed, ids_json, wz.color],
    )
    .map_err(|e| ApiError::internal(format!("新增漫遊區域失敗：{e}")))?;
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "wander_zone": wz })))
}

pub async fn update_wander_zone(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
    Json(req): Json<WanderZoneReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let wz = WanderZone {
        id: id.clone(),
        name: req.name,
        cx: req.cx,
        cy: req.cy,
        radius: req.radius.clamp(1.0, 30.0),
        speed: req.speed.clamp(0.1, 10.0),
        entity_ids: req.entity_ids,
        color: req.color,
    };
    let ids_json = serde_json::to_string(&wz.entity_ids).unwrap_or_else(|_| "[]".into());
    let changed = conn
        .execute(
            "UPDATE wander_zones SET name=?2, cx=?3, cy=?4, radius=?5, speed=?6, entity_ids_json=?7, color=?8 WHERE id=?1",
            params![wz.id, wz.name, wz.cx, wz.cy, wz.radius, wz.speed, ids_json, wz.color],
        )
        .map_err(|e| ApiError::internal(format!("更新漫遊區域失敗：{e}")))?;
    if changed == 0 {
        return Err(ApiError::not_found("找不到指定的漫遊區域。"));
    }
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "wander_zone": wz })))
}

pub async fn delete_wander_zone(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    conn.execute("DELETE FROM wander_zones WHERE id = ?1", params![id])
        .map_err(|e| ApiError::internal(format!("刪除漫遊區域失敗：{e}")))?;
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct EventRuleReq {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub trigger_type: String,
    #[serde(default)]
    pub trigger_config: Value,
    #[serde(default)]
    pub actions: Vec<Value>,
    #[serde(default = "default_cooldown")]
    pub cooldown_sec: f64,
}
fn default_cooldown() -> f64 {
    2.0
}

pub async fn create_event(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<EventRuleReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let ev = EventRule {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        enabled: req.enabled,
        trigger_type: req.trigger_type,
        trigger_config: req.trigger_config,
        actions: req.actions,
        cooldown_sec: req.cooldown_sec.max(0.0),
    };
    let trig_json = serde_json::to_string(&ev.trigger_config).unwrap_or_else(|_| "{}".into());
    let act_json = serde_json::to_string(&ev.actions).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO event_rules (id, name, enabled, trigger_type, trigger_config_json, actions_json, cooldown_sec)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            ev.id,
            ev.name,
            ev.enabled as i64,
            ev.trigger_type,
            trig_json,
            act_json,
            ev.cooldown_sec
        ],
    )
    .map_err(|e| ApiError::internal(format!("新增事件規則失敗：{e}")))?;
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "event": ev })))
}

pub async fn update_event(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
    Json(req): Json<EventRuleReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let ev = EventRule {
        id: id.clone(),
        name: req.name,
        enabled: req.enabled,
        trigger_type: req.trigger_type,
        trigger_config: req.trigger_config,
        actions: req.actions,
        cooldown_sec: req.cooldown_sec.max(0.0),
    };
    let trig_json = serde_json::to_string(&ev.trigger_config).unwrap_or_else(|_| "{}".into());
    let act_json = serde_json::to_string(&ev.actions).unwrap_or_else(|_| "[]".into());
    let changed = conn
        .execute(
            "UPDATE event_rules SET name=?2, enabled=?3, trigger_type=?4, trigger_config_json=?5, actions_json=?6, cooldown_sec=?7 WHERE id=?1",
            params![
                ev.id,
                ev.name,
                ev.enabled as i64,
                ev.trigger_type,
                trig_json,
                act_json,
                ev.cooldown_sec
            ],
        )
        .map_err(|e| ApiError::internal(format!("更新事件規則失敗：{e}")))?;
    if changed == 0 {
        return Err(ApiError::not_found("找不到指定的事件規則。"));
    }
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "event": ev })))
}

pub async fn delete_event(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    conn.execute("DELETE FROM event_rules WHERE id = ?1", params![id])
        .map_err(|e| ApiError::internal(format!("刪除事件規則失敗：{e}")))?;
    db::touch_world_updated(&conn);
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct CreateSnapshotReq {
    pub label: String,
}

pub async fn create_snapshot(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<CreateSnapshotReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let safe_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&file)
        .to_string();
    let label = if req.label.trim().is_empty() {
        "手動存檔快照"
    } else {
        req.label.trim()
    };
    let snap = db::create_snapshot(&path, &safe_name, label).map_err(ApiError::internal)?;
    Ok(Json(json!({ "snapshot": snap })))
}

pub async fn restore_snapshot(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    db::restore_snapshot(&path, &id).map_err(ApiError::bad_req)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ResetWorldReq {
    #[serde(default = "default_reset_mode")]
    pub mode: String,
}
fn default_reset_mode() -> String {
    "progress_only".to_string()
}

pub async fn reset_world_handler(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<ResetWorldReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let safe_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&file)
        .to_string();
    let full = db::reset_world_state(&st.data_dir, &path, req.mode.trim(), &safe_name)
        .map_err(ApiError::bad_req)?;
    Ok(Json(serde_json::to_value(full).map_err(|e| ApiError::internal(e.to_string()))?))
}

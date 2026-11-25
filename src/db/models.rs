//! 《StackCity RT（像素疊界）》資料庫與前後端共用資料模型。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 世界清單摘要卡片
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSummary {
    pub file_name: String,
    pub world_id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub theme: String,
    pub width: i32,
    pub height: i32,
    pub entity_count: i64,
    pub asset_count: i64,
    pub updated_at: String,
    pub thumbnail_b64: String,
}

/// 世界核心中繼設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldMeta {
    pub schema_version: String,
    pub world_id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub theme: String,
    pub width: i32,
    pub height: i32,
    pub time_of_day: f64,
    pub time_speed: f64,
    pub weather: String,
    #[serde(default = "default_bgm_preset")]
    pub bgm_preset: String,
    #[serde(default = "default_bgm_volume")]
    pub bgm_volume: f64,
    #[serde(default)]
    pub has_custom_bgm: bool,
    #[serde(default)]
    pub custom_bgm_name: Option<String>,
    #[serde(default)]
    pub player_asset_id: Option<String>,
    #[serde(default = "default_player_name")]
    pub player_name: String,
    #[serde(default = "default_one_f64")]
    pub player_scale: f64,
    #[serde(default)]
    pub player_x: Option<f64>,
    #[serde(default)]
    pub player_y: Option<f64>,
    #[serde(default)]
    pub quest_config: Option<Value>,
    #[serde(default)]
    pub quest_progress: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
    pub thumbnail_b64: String,
}

fn default_bgm_preset() -> String {
    "kingdom".to_string()
}

fn default_bgm_volume() -> f64 {
    0.45
}

fn default_player_name() -> String {
    "冒險玩家".to_string()
}

fn default_one_f64() -> f64 {
    1.0
}

/// 單一地形格點
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainTile {
    pub x: i32,
    pub y: i32,
    pub elevation: i32,
    pub ground_type: String,
    pub road_type: String,
    pub custom_color: Option<String>,
}

/// 資產目錄中繼資料（不含大體積 PNG BLOB，貼圖由獨立端點按需快取載入）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetMeta {
    pub id: String,
    pub name: String,
    pub category: String,
    pub source_type: String,
    pub blueprint_json: Option<String>,
    pub slice_count: i32,
    pub slice_w: i32,
    pub slice_h: i32,
    pub default_scale: f32,
    pub anim_preset: String,
    pub sha256: String,
    pub created_at: String,
}

/// 場景內投放的實體物件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEntity {
    pub id: String,
    pub asset_id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f64,
    pub scale: f64,
    #[serde(default = "default_one_f64")]
    pub scale_x: f64,
    #[serde(default = "default_one_f64")]
    pub scale_y: f64,
    #[serde(default = "default_one_f64")]
    pub scale_z: f64,
    pub parent_id: Option<String>,
    pub tint_hex: String,
    pub emissive_on: bool,
    pub visible: bool,
    pub anim_mode: String,
    pub tag: String,
    pub meta_json: String,
}

/// 移動路徑節點
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Waypoint {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    #[serde(default)]
    pub wait_sec: f64,
}

/// 移動路徑設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovementPath {
    pub id: String,
    pub name: String,
    pub entity_id: Option<String>,
    pub loop_mode: String,
    pub speed: f64,
    pub smooth: bool,
    pub active: bool,
    pub color: String,
    pub waypoints: Vec<Waypoint>,
}

/// 自由漫遊區域設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WanderZone {
    pub id: String,
    pub name: String,
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    pub speed: f64,
    pub entity_ids: Vec<String>,
    pub color: String,
}

/// 無程式碼事件觸發規則
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger_type: String,
    pub trigger_config: Value,
    pub actions: Vec<Value>,
    pub cooldown_sec: f64,
}

/// 世界版本快照資訊
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSnapshotInfo {
    pub id: String,
    pub label: String,
    pub created_at: String,
}

/// 前端載入世界的完整狀態負載
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldFullState {
    pub file_name: String,
    pub meta: WorldMeta,
    pub terrain: Vec<TerrainTile>,
    pub assets: Vec<AssetMeta>,
    pub entities: Vec<WorldEntity>,
    pub paths: Vec<MovementPath>,
    pub wander_zones: Vec<WanderZone>,
    pub events: Vec<EventRule>,
    pub snapshots: Vec<WorldSnapshotInfo>,
}

/// 個人跨世界資產庫項目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryAssetItem {
    pub id: String,
    pub name: String,
    pub category: String,
    pub source_type: String,
    pub blueprint_json: Option<String>,
    pub slice_count: i32,
    pub slice_w: i32,
    pub slice_h: i32,
    pub default_scale: f32,
    pub anim_preset: String,
    pub sha256: String,
    pub created_at: String,
}

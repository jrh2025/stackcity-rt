//! SQLite 單檔自包含世界資料庫（`.stackworld.db`）與個人跨世界資產庫（`_library.db`）。
//!
//! 確保所有自創精靈堆疊貼圖（slices / normals / emissive PNG）、3D 藍圖 JSON、地形、
//! 場景實例、移動路徑、漫遊區域與事件規則皆封裝於單一 SQLite 檔案內，
//! 支援 `VACUUM INTO` 原子匯出與嚴格結構校驗匯入。

pub mod models;
pub mod office_day;
pub mod presets;

use crate::voxel::{bake_from_strip_bytes, blueprint::bake_blueprint_json, BakedStack};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use models::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const SCHEMA_VERSION: &str = "1";
pub const WORLD_EXT: &str = ".stackworld.db";
const LIBRARY_DB_NAME: &str = "_library.db";

const WORLD_SCHEMA_SQL: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS world_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS terrain_tiles (
    x INTEGER NOT NULL,
    y INTEGER NOT NULL,
    elevation INTEGER NOT NULL DEFAULT 1,
    ground_type TEXT NOT NULL DEFAULT 'grass',
    road_type TEXT NOT NULL DEFAULT 'none',
    custom_color TEXT,
    PRIMARY KEY (x, y)
);

CREATE TABLE IF NOT EXISTS asset_catalog (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    source_type TEXT NOT NULL,
    blueprint_json TEXT,
    slices_png BLOB NOT NULL,
    normals_png BLOB NOT NULL,
    emissive_png BLOB NOT NULL,
    slice_count INTEGER NOT NULL,
    slice_w INTEGER NOT NULL,
    slice_h INTEGER NOT NULL,
    default_scale REAL NOT NULL DEFAULT 1.0,
    anim_preset TEXT NOT NULL DEFAULT 'static',
    sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS world_entities (
    id TEXT PRIMARY KEY,
    asset_id TEXT NOT NULL REFERENCES asset_catalog(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    x REAL NOT NULL,
    y REAL NOT NULL,
    z REAL NOT NULL DEFAULT 0.0,
    yaw REAL NOT NULL DEFAULT 0.0,
    scale REAL NOT NULL DEFAULT 1.0,
    scale_x REAL NOT NULL DEFAULT 1.0,
    scale_y REAL NOT NULL DEFAULT 1.0,
    scale_z REAL NOT NULL DEFAULT 1.0,
    parent_id TEXT,
    tint_hex TEXT NOT NULL DEFAULT '#ffffff',
    emissive_on INTEGER NOT NULL DEFAULT 1,
    visible INTEGER NOT NULL DEFAULT 1,
    anim_mode TEXT NOT NULL DEFAULT 'auto',
    tag TEXT NOT NULL DEFAULT '',
    meta_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS movement_paths (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    entity_id TEXT REFERENCES world_entities(id) ON DELETE SET NULL,
    loop_mode TEXT NOT NULL DEFAULT 'loop',
    speed REAL NOT NULL DEFAULT 2.0,
    smooth INTEGER NOT NULL DEFAULT 1,
    active INTEGER NOT NULL DEFAULT 1,
    color TEXT NOT NULL DEFAULT '#38bdf8',
    waypoints_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS wander_zones (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    cx REAL NOT NULL,
    cy REAL NOT NULL,
    radius REAL NOT NULL,
    speed REAL NOT NULL DEFAULT 1.2,
    entity_ids_json TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#4ade80'
);

CREATE TABLE IF NOT EXISTS event_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    trigger_type TEXT NOT NULL,
    trigger_config_json TEXT NOT NULL,
    actions_json TEXT NOT NULL,
    cooldown_sec REAL NOT NULL DEFAULT 2.0
);

CREATE TABLE IF NOT EXISTS world_snapshots (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    created_at TEXT NOT NULL,
    state_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS world_bgm (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    data BLOB NOT NULL,
    updated_at TEXT NOT NULL
);
"#;

const LIBRARY_SCHEMA_SQL: &str = r#"
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS asset_catalog (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    source_type TEXT NOT NULL,
    blueprint_json TEXT,
    slices_png BLOB NOT NULL,
    normals_png BLOB NOT NULL,
    emissive_png BLOB NOT NULL,
    slice_count INTEGER NOT NULL,
    slice_w INTEGER NOT NULL,
    slice_h INTEGER NOT NULL,
    default_scale REAL NOT NULL DEFAULT 1.0,
    anim_preset TEXT NOT NULL DEFAULT 'static',
    sha256 TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);
"#;

/// 清理檔案名稱，防止路徑遍歷並確保 `.stackworld.db` 副檔名
pub fn sanitize_world_filename(raw: &str) -> Result<String, String> {
    let base = Path::new(raw)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .trim();
    if base.is_empty() || base.contains("..") || base.contains('/') || base.contains('\\') {
        return Err("無效的世界檔案名稱。".to_string());
    }
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '（' | '）') || c as u32 > 127 {
                c
            } else {
                '_'
            }
        })
        .collect();
    let final_name = if clean.ends_with(WORLD_EXT) {
        clean
    } else {
        format!("{}{}", clean.trim_end_matches(".db"), WORLD_EXT)
    };
    Ok(final_name)
}

pub fn world_path(data_dir: &Path, file_name: &str) -> Result<PathBuf, String> {
    let safe = sanitize_world_filename(file_name)?;
    Ok(data_dir.join(safe))
}

pub fn open_world_conn(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| format!("開啟世界資料庫失敗：{e}"))?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS world_bgm (
             id INTEGER PRIMARY KEY CHECK (id = 1),
             name TEXT NOT NULL,
             mime_type TEXT NOT NULL,
             data BLOB NOT NULL,
             updated_at TEXT NOT NULL
         );",
    )
    .map_err(|e| format!("設定 SQLite WAL 模式失敗：{e}"))?;

    // 向下相容舊版 .stackworld.db：僅當 world_entities 表已存在時，補齊三軸獨立縮放欄位
    let has_entities_tbl: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='world_entities'",
            [],
            |_| Ok(true),
        )
        .optional()
        .unwrap_or(None)
        .unwrap_or(false);
    if has_entities_tbl {
        let mut cols: HashSet<String> = HashSet::new();
        if let Ok(mut st) = conn.prepare("PRAGMA table_info(world_entities)") {
            if let Ok(rows) = st.query_map([], |r| r.get::<_, String>(1)) {
                for name in rows.flatten() {
                    cols.insert(name);
                }
            }
        }
        let mut added = false;
        for col in ["scale_x", "scale_y", "scale_z"] {
            if !cols.contains(col) {
                let sql = format!("ALTER TABLE world_entities ADD COLUMN {col} REAL");
                conn.execute(&sql, [])
                    .map_err(|e| format!("遷移舊版世界物件縮放欄位失敗 ({col})：{e}"))?;
                added = true;
            }
        }
        if added {
            conn.execute(
                "UPDATE world_entities SET
                   scale_x = COALESCE(scale_x, scale),
                   scale_y = COALESCE(scale_y, scale),
                   scale_z = COALESCE(scale_z, scale)",
                [],
            )
            .map_err(|e| format!("初始化舊版世界物件三軸縮放值失敗：{e}"))?;
        }
    }

    Ok(conn)
}

pub fn init_world_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(WORLD_SCHEMA_SQL)
        .map_err(|e| format!("初始化世界資料表失敗：{e}"))
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn set_meta_kv(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO world_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| format!("寫入世界設定失敗 ({key})：{e}"))?;
    Ok(())
}

pub fn touch_world_updated(conn: &Connection) {
    let _ = set_meta_kv(conn, "updated_at", &now_iso());
}

pub fn read_world_meta(conn: &Connection) -> Result<WorldMeta, String> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM world_meta")
        .map_err(|e| format!("查詢世界設定失敗：{e}"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| format!("讀取世界設定失敗：{e}"))?;
    let mut map: HashMap<String, String> = HashMap::new();
    for row in rows {
        let (k, v) = row.map_err(|e| format!("解析設定失敗：{e}"))?;
        map.insert(k, v);
    }

    let custom_bgm_name: Option<String> = conn
        .query_row("SELECT name FROM world_bgm WHERE id = 1", [], |r| r.get(0))
        .optional()
        .unwrap_or(None);

    let theme_val = map
        .get("theme")
        .cloned()
        .unwrap_or_else(|| "kingdom".to_string());

    Ok(WorldMeta {
        schema_version: map
            .get("schema_version")
            .cloned()
            .unwrap_or_else(|| SCHEMA_VERSION.to_string()),
        world_id: map
            .get("world_id")
            .cloned()
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: map
            .get("name")
            .cloned()
            .unwrap_or_else(|| "未命名像素世界".to_string()),
        author: map
            .get("author")
            .cloned()
            .unwrap_or_else(|| "像素建築師".to_string()),
        description: map.get("description").cloned().unwrap_or_default(),
        theme: theme_val.clone(),
        width: map
            .get("width")
            .and_then(|s| s.parse().ok())
            .unwrap_or(40),
        height: map
            .get("height")
            .and_then(|s| s.parse().ok())
            .unwrap_or(40),
        time_of_day: map
            .get("time_of_day")
            .and_then(|s| s.parse().ok())
            .unwrap_or(10.5),
        time_speed: map
            .get("time_speed")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        weather: map
            .get("weather")
            .cloned()
            .unwrap_or_else(|| "clear".to_string()),
        bgm_preset: map.get("bgm_preset").cloned().unwrap_or_else(|| {
            if theme_val.ends_with("_interior") {
                "indoor".to_string()
            } else if matches!(theme_val.as_str(), "city" | "farm" | "campus") {
                theme_val.clone()
            } else {
                "kingdom".to_string()
            }
        }),
        bgm_volume: map
            .get("bgm_volume")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.45),
        has_custom_bgm: custom_bgm_name.is_some(),
        custom_bgm_name,
        player_asset_id: map.get("player_asset_id").cloned().filter(|s| !s.trim().is_empty()),
        player_name: map
            .get("player_name")
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "冒險玩家".to_string()),
        player_scale: map
            .get("player_scale")
            .and_then(|s| s.parse::<f64>().ok())
            .map(|v| v.clamp(0.3, 3.0))
            .unwrap_or(1.0),
        player_x: map.get("player_x").and_then(|s| s.parse::<f64>().ok()),
        player_y: map.get("player_y").and_then(|s| s.parse::<f64>().ok()),
        quest_config: map
            .get("quest_config_json")
            .and_then(|s| serde_json::from_str::<Value>(s).ok()),
        quest_progress: map
            .get("quest_progress_json")
            .and_then(|s| serde_json::from_str::<Value>(s).ok()),
        created_at: map.get("created_at").cloned().unwrap_or_else(now_iso),
        updated_at: map.get("updated_at").cloned().unwrap_or_else(now_iso),
        thumbnail_b64: map.get("thumbnail_b64").cloned().unwrap_or_default(),
    })
}

/// 初始化資料目錄、個人跨世界資產庫及確保預設開局世界存在
pub fn ensure_data_dir_initialized(data_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| format!("建立世界目錄失敗：{e}"))?;
    init_library_db(data_dir)?;

    let existing = list_worlds(data_dir)?;
    if existing.is_empty() {
        presets::create_world_from_preset(
            data_dir,
            "kingdom",
            "奇幻星光王國",
            "像素國王",
            Some("kingdom_dawn.stackworld.db"),
        )?;
    }
    Ok(())
}

/// 初始化個人跨世界資產庫（並自動匯入工作區既有 outputs 示範資產）
pub fn init_library_db(data_dir: &Path) -> Result<(), String> {
    let lib_path = data_dir.join(LIBRARY_DB_NAME);
    let conn = Connection::open(&lib_path).map_err(|e| format!("開啟個人資產庫失敗：{e}"))?;
    conn.execute_batch(LIBRARY_SCHEMA_SQL)
        .map_err(|e| format!("建立個人資產庫資料表失敗：{e}"))?;

    // 自動收錄工作區既有 outputs/adult_male_lion 與 outputs/bulbasaur 藍圖（若存在且尚未收錄）
    let candidate_dirs = [
        PathBuf::from("../outputs/adult_male_lion"),
        PathBuf::from("../outputs/bulbasaur"),
        PathBuf::from("outputs/adult_male_lion"),
        PathBuf::from("outputs/bulbasaur"),
    ];
    for dir in candidate_dirs {
        let bp_path = dir.join("blueprint.json");
        if bp_path.exists() {
            if let Ok(raw) = fs::read_to_string(&bp_path) {
                if let Ok(bp_val) = serde_json::from_str::<Value>(&raw) {
                    let cat = if dir.to_string_lossy().contains("lion") {
                        "animal"
                    } else {
                        "character"
                    };
                    if let Ok(baked) =
                        bake_blueprint_json(&bp_val, cat, 0.78, "walk", None, true, true)
                    {
                        let _ = upsert_library_asset(&conn, &baked, "imported_blueprint");
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn upsert_library_asset(
    lib_conn: &Connection,
    baked: &BakedStack,
    source_type: &str,
) -> Result<String, String> {
    let existing: Option<String> = lib_conn
        .query_row(
            "SELECT id FROM asset_catalog WHERE sha256 = ?1",
            params![baked.sha256],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("查詢資產庫失敗：{e}"))?;
    if let Some(id) = existing {
        // 即使幾何 sha256 相同，仍須同步更新最新名稱、分類與藍圖 JSON，避免改名無法儲存
        lib_conn
            .execute(
                "UPDATE asset_catalog
                 SET name = ?2, category = ?3, blueprint_json = COALESCE(?4, blueprint_json),
                     default_scale = ?5, anim_preset = ?6
                 WHERE id = ?1",
                params![
                    id,
                    baked.title,
                    baked.category,
                    baked.blueprint_json,
                    baked.default_scale,
                    baked.anim_preset,
                ],
            )
            .map_err(|e| format!("更新既有個人資產庫名稱失敗：{e}"))?;
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    lib_conn
        .execute(
            "INSERT INTO asset_catalog
             (id, name, category, source_type, blueprint_json, slices_png, normals_png, emissive_png,
              slice_count, slice_w, slice_h, default_scale, anim_preset, sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                id,
                baked.title,
                baked.category,
                source_type,
                baked.blueprint_json,
                baked.slices_png,
                baked.normals_png,
                baked.emissive_png,
                baked.slices as i32,
                baked.slice_w as i32,
                baked.slice_h as i32,
                baked.default_scale,
                baked.anim_preset,
                baked.sha256,
                now_iso(),
            ],
        )
        .map_err(|e| format!("寫入個人資產庫失敗：{e}"))?;
    Ok(id)
}

pub fn list_library_assets(data_dir: &Path) -> Result<Vec<LibraryAssetItem>, String> {
    let lib_path = data_dir.join(LIBRARY_DB_NAME);
    let conn = Connection::open(&lib_path).map_err(|e| format!("開啟個人資產庫失敗：{e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, category, source_type, blueprint_json, slice_count, slice_w, slice_h,
                    default_scale, anim_preset, sha256, created_at
             FROM asset_catalog ORDER BY created_at DESC",
        )
        .map_err(|e| format!("查詢個人資產庫失敗：{e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(LibraryAssetItem {
                id: r.get(0)?,
                name: r.get(1)?,
                category: r.get(2)?,
                source_type: r.get(3)?,
                blueprint_json: r.get(4)?,
                slice_count: r.get(5)?,
                slice_w: r.get(6)?,
                slice_h: r.get(7)?,
                default_scale: r.get(8)?,
                anim_preset: r.get(9)?,
                sha256: r.get(10)?,
                created_at: r.get(11)?,
            })
        })
        .map_err(|e| format!("讀取個人資產庫失敗：{e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("解析個人資產庫失敗：{e}"))
}

pub fn get_library_asset_blobs(
    data_dir: &Path,
    asset_id: &str,
) -> Result<Option<(LibraryAssetItem, Vec<u8>, Vec<u8>, Vec<u8>)>, String> {
    let lib_path = data_dir.join(LIBRARY_DB_NAME);
    let conn = Connection::open(&lib_path).map_err(|e| format!("開啟個人資產庫失敗：{e}"))?;
    conn.query_row(
        "SELECT id, name, category, source_type, blueprint_json, slice_count, slice_w, slice_h,
                default_scale, anim_preset, sha256, created_at, slices_png, normals_png, emissive_png
         FROM asset_catalog WHERE id = ?1",
        params![asset_id],
        |r| {
            let item = LibraryAssetItem {
                id: r.get(0)?,
                name: r.get(1)?,
                category: r.get(2)?,
                source_type: r.get(3)?,
                blueprint_json: r.get(4)?,
                slice_count: r.get(5)?,
                slice_w: r.get(6)?,
                slice_h: r.get(7)?,
                default_scale: r.get(8)?,
                anim_preset: r.get(9)?,
                sha256: r.get(10)?,
                created_at: r.get(11)?,
            };
            Ok((item, r.get(12)?, r.get(13)?, r.get(14)?))
        },
    )
    .optional()
    .map_err(|e| format!("讀取資產庫貼圖失敗：{e}"))
}

/// 將 BakedStack 插入或去重寫入指定世界 DB，同時同步至個人資產庫
pub fn insert_asset_into_world(
    conn: &Connection,
    data_dir: &Path,
    baked: &BakedStack,
    source_type: &str,
    preferred_id: Option<&str>,
) -> Result<AssetMeta, String> {
    // 同步寫入個人跨世界資產庫
    if let Ok(lib_conn) = Connection::open(data_dir.join(LIBRARY_DB_NAME)) {
        let _ = upsert_library_asset(&lib_conn, baked, source_type);
    }

    if preferred_id.is_none() {
        if let Some(mut existing) = conn
            .query_row(
                "SELECT id, name, category, source_type, blueprint_json, slice_count, slice_w, slice_h,
                        default_scale, anim_preset, sha256, created_at
                 FROM asset_catalog WHERE sha256 = ?1",
                params![baked.sha256],
                |r| {
                    Ok(AssetMeta {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        category: r.get(2)?,
                        source_type: r.get(3)?,
                        blueprint_json: r.get(4)?,
                        slice_count: r.get(5)?,
                        slice_w: r.get(6)?,
                        slice_h: r.get(7)?,
                        default_scale: r.get(8)?,
                        anim_preset: r.get(9)?,
                        sha256: r.get(10)?,
                        created_at: r.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("查詢世界資產去重失敗：{e}"))?
        {
            if existing.name != baked.title
                || existing.category != baked.category
                || existing.blueprint_json != baked.blueprint_json
            {
                conn.execute(
                    "UPDATE asset_catalog
                     SET name = ?2, category = ?3, blueprint_json = COALESCE(?4, blueprint_json),
                         default_scale = ?5, anim_preset = ?6
                     WHERE id = ?1",
                    params![
                        existing.id,
                        baked.title,
                        baked.category,
                        baked.blueprint_json,
                        baked.default_scale,
                        baked.anim_preset,
                    ],
                )
                .map_err(|e| format!("更新既有世界資產名稱失敗：{e}"))?;
                touch_world_updated(conn);
                existing.name = baked.title.clone();
                existing.category = baked.category.clone();
                if baked.blueprint_json.is_some() {
                    existing.blueprint_json = baked.blueprint_json.clone();
                }
                existing.default_scale = baked.default_scale;
                existing.anim_preset = baked.anim_preset.clone();
            }
            return Ok(existing);
        }
    }

    let id = preferred_id
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let created_at = now_iso();
    conn.execute(
        "INSERT INTO asset_catalog
         (id, name, category, source_type, blueprint_json, slices_png, normals_png, emissive_png,
          slice_count, slice_w, slice_h, default_scale, anim_preset, sha256, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
          name=excluded.name, category=excluded.category, source_type=excluded.source_type,
          blueprint_json=excluded.blueprint_json, slices_png=excluded.slices_png,
          normals_png=excluded.normals_png, emissive_png=excluded.emissive_png,
          slice_count=excluded.slice_count, slice_w=excluded.slice_w, slice_h=excluded.slice_h,
          default_scale=excluded.default_scale, anim_preset=excluded.anim_preset, sha256=excluded.sha256",
        params![
            id,
            baked.title,
            baked.category,
            source_type,
            baked.blueprint_json,
            baked.slices_png,
            baked.normals_png,
            baked.emissive_png,
            baked.slices as i32,
            baked.slice_w as i32,
            baked.slice_h as i32,
            baked.default_scale,
            baked.anim_preset,
            baked.sha256,
            created_at,
        ],
    )
    .map_err(|e| format!("儲存資產至世界失敗：{e}"))?;

    touch_world_updated(conn);
    Ok(AssetMeta {
        id,
        name: baked.title.clone(),
        category: baked.category.clone(),
        source_type: source_type.to_string(),
        blueprint_json: baked.blueprint_json.clone(),
        slice_count: baked.slices as i32,
        slice_w: baked.slice_w as i32,
        slice_h: baked.slice_h as i32,
        default_scale: baked.default_scale,
        anim_preset: baked.anim_preset.clone(),
        sha256: baked.sha256.clone(),
        created_at,
    })
}

/// 列出目錄下所有 `.stackworld.db` 世界摘要
pub fn list_worlds(data_dir: &Path) -> Result<Vec<WorldSummary>, String> {
    let entries = fs::read_dir(data_dir).map_err(|e| format!("掃描世界目錄失敗：{e}"))?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        let Some(fname) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !fname.ends_with(WORLD_EXT) {
            continue;
        }
        if let Ok(conn) = open_world_conn(&p) {
            if let Ok(meta) = read_world_meta(&conn) {
                let entity_count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM world_entities", [], |r| r.get(0))
                    .unwrap_or(0);
                let asset_count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM asset_catalog", [], |r| r.get(0))
                    .unwrap_or(0);
                out.push(WorldSummary {
                    file_name: fname.to_string(),
                    world_id: meta.world_id,
                    name: meta.name,
                    author: meta.author,
                    description: meta.description,
                    theme: meta.theme,
                    width: meta.width,
                    height: meta.height,
                    entity_count,
                    asset_count,
                    updated_at: meta.updated_at,
                    thumbnail_b64: meta.thumbnail_b64,
                });
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

/// 讀取單一世界的完整狀態
pub fn load_full_world_state(path: &Path, file_name: &str) -> Result<WorldFullState, String> {
    let conn = open_world_conn(path)?;
    let meta = read_world_meta(&conn)?;

    let terrain = {
        let mut st = conn
            .prepare("SELECT x, y, elevation, ground_type, road_type, custom_color FROM terrain_tiles ORDER BY y, x")
            .map_err(|e| format!("讀取地形失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
                Ok(TerrainTile {
                    x: r.get(0)?,
                    y: r.get(1)?,
                    elevation: r.get(2)?,
                    ground_type: r.get(3)?,
                    road_type: r.get(4)?,
                    custom_color: r.get(5)?,
                })
            })
            .map_err(|e| format!("讀取地形列失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析地形列失敗：{e}"))?
    };

    let assets = {
        let mut st = conn
            .prepare(
                "SELECT id, name, category, source_type, blueprint_json, slice_count, slice_w, slice_h,
                        default_scale, anim_preset, sha256, created_at
                 FROM asset_catalog ORDER BY category, name",
            )
            .map_err(|e| format!("讀取資產清單失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
                Ok(AssetMeta {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    category: r.get(2)?,
                    source_type: r.get(3)?,
                    blueprint_json: r.get(4)?,
                    slice_count: r.get(5)?,
                    slice_w: r.get(6)?,
                    slice_h: r.get(7)?,
                    default_scale: r.get(8)?,
                    anim_preset: r.get(9)?,
                    sha256: r.get(10)?,
                    created_at: r.get(11)?,
                })
            })
            .map_err(|e| format!("查詢資產清單失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析資產清單失敗：{e}"))?
    };

    let entities = {
        let mut st = conn
            .prepare(
                "SELECT id, asset_id, name, x, y, z, yaw, scale,
                        COALESCE(scale_x, scale), COALESCE(scale_y, scale), COALESCE(scale_z, scale),
                        parent_id, tint_hex, emissive_on, visible, anim_mode, tag, meta_json
                 FROM world_entities",
            )
            .map_err(|e| format!("讀取實體清單失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
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
            })
            .map_err(|e| format!("查詢實體失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析實體失敗：{e}"))?
    };

    let paths = {
        let mut st = conn
            .prepare("SELECT id, name, entity_id, loop_mode, speed, smooth, active, color, waypoints_json FROM movement_paths")
            .map_err(|e| format!("讀取路徑失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
                let wp_str: String = r.get(8)?;
                let waypoints: Vec<Waypoint> = serde_json::from_str(&wp_str).unwrap_or_default();
                Ok(MovementPath {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    entity_id: r.get(2)?,
                    loop_mode: r.get(3)?,
                    speed: r.get(4)?,
                    smooth: r.get::<_, i64>(5)? != 0,
                    active: r.get::<_, i64>(6)? != 0,
                    color: r.get(7)?,
                    waypoints,
                })
            })
            .map_err(|e| format!("查詢路徑失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析路徑失敗：{e}"))?
    };

    let wander_zones = {
        let mut st = conn
            .prepare("SELECT id, name, cx, cy, radius, speed, entity_ids_json, color FROM wander_zones")
            .map_err(|e| format!("讀取漫遊區失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
                let ids_str: String = r.get(6)?;
                let entity_ids: Vec<String> = serde_json::from_str(&ids_str).unwrap_or_default();
                Ok(WanderZone {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    cx: r.get(2)?,
                    cy: r.get(3)?,
                    radius: r.get(4)?,
                    speed: r.get(5)?,
                    entity_ids,
                    color: r.get(7)?,
                })
            })
            .map_err(|e| format!("查詢漫遊區失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析漫遊區失敗：{e}"))?
    };

    let events = {
        let mut st = conn
            .prepare("SELECT id, name, enabled, trigger_type, trigger_config_json, actions_json, cooldown_sec FROM event_rules")
            .map_err(|e| format!("讀取事件規則失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
                let trig_str: String = r.get(4)?;
                let act_str: String = r.get(5)?;
                Ok(EventRule {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    enabled: r.get::<_, i64>(2)? != 0,
                    trigger_type: r.get(3)?,
                    trigger_config: serde_json::from_str(&trig_str).unwrap_or(Value::Null),
                    actions: serde_json::from_str(&act_str).unwrap_or_default(),
                    cooldown_sec: r.get(6)?,
                })
            })
            .map_err(|e| format!("查詢事件規則失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析事件規則失敗：{e}"))?
    };

    let snapshots = {
        let mut st = conn
            .prepare("SELECT id, label, created_at FROM world_snapshots ORDER BY created_at DESC LIMIT 30")
            .map_err(|e| format!("讀取快照清單失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
                Ok(WorldSnapshotInfo {
                    id: r.get(0)?,
                    label: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })
            .map_err(|e| format!("查詢快照清單失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析快照清單失敗：{e}"))?
    };

    Ok(WorldFullState {
        file_name: file_name.to_string(),
        meta,
        terrain,
        assets,
        entities,
        paths,
        wander_zones,
        events,
        snapshots,
    })
}

/// 讀取單一資產的指定貼圖（slices / normals / emissive）與 sha256
pub fn read_asset_texture(
    path: &Path,
    asset_id: &str,
    kind: &str,
) -> Result<Option<(Vec<u8>, String)>, String> {
    let col = match kind {
        "slices" => "slices_png",
        "normals" => "normals_png",
        "emissive" => "emissive_png",
        _ => return Err("未知的貼圖類型。".to_string()),
    };
    let conn = open_world_conn(path)?;
    let sql = format!("SELECT {col}, sha256 FROM asset_catalog WHERE id = ?1");
    conn.query_row(&sql, params![asset_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .map_err(|e| format!("讀取資產貼圖失敗：{e}"))
}

#[derive(Serialize, Deserialize)]
struct SnapshotPayload {
    meta: WorldMeta,
    terrain: Vec<TerrainTile>,
    entities: Vec<WorldEntity>,
    paths: Vec<MovementPath>,
    wander_zones: Vec<WanderZone>,
    events: Vec<EventRule>,
}

/// 建立世界快照（供隨時一鍵回溯還原）
pub fn create_snapshot(path: &Path, file_name: &str, label: &str) -> Result<WorldSnapshotInfo, String> {
    let full = load_full_world_state(path, file_name)?;
    let payload = SnapshotPayload {
        meta: full.meta,
        terrain: full.terrain,
        entities: full.entities,
        paths: full.paths,
        wander_zones: full.wander_zones,
        events: full.events,
    };
    let state_json = serde_json::to_string(&payload).map_err(|e| format!("序列化快照失敗：{e}"))?;
    let id = Uuid::new_v4().to_string();
    let created_at = now_iso();
    let conn = open_world_conn(path)?;
    conn.execute(
        "INSERT INTO world_snapshots (id, label, created_at, state_json) VALUES (?1, ?2, ?3, ?4)",
        params![id, label, created_at, state_json],
    )
    .map_err(|e| format!("寫入快照失敗：{e}"))?;
    Ok(WorldSnapshotInfo {
        id,
        label: label.to_string(),
        created_at,
    })
}

/// 從快照還原世界狀態（單一交易內原子還原）
pub fn restore_snapshot(path: &Path, snap_id: &str) -> Result<(), String> {
    let mut conn = open_world_conn(path)?;
    let raw: String = conn
        .query_row(
            "SELECT state_json FROM world_snapshots WHERE id = ?1",
            params![snap_id],
            |r| r.get(0),
        )
        .map_err(|_| "找不到指定的世界快照。".to_string())?;
    let payload: SnapshotPayload =
        serde_json::from_str(&raw).map_err(|e| format!("快照資料損毀：{e}"))?;

    let tx = conn.transaction().map_err(|e| format!("開啟交易失敗：{e}"))?;
    tx.execute(
        "INSERT INTO world_meta (key, value) VALUES ('time_of_day', ?1), ('weather', ?2), ('updated_at', ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![
            payload.meta.time_of_day.to_string(),
            payload.meta.weather,
            now_iso()
        ],
    )
    .map_err(|e| format!("還原世界設定失敗：{e}"))?;
    if let Some(pid) = &payload.meta.player_asset_id {
        tx.execute(
            "INSERT INTO world_meta (key, value) VALUES ('player_asset_id', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![pid],
        )
        .map_err(|e| format!("還原玩家外觀失敗：{e}"))?;
    }
    tx.execute(
        "INSERT INTO world_meta (key, value) VALUES ('player_name', ?1), ('player_scale', ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![payload.meta.player_name, payload.meta.player_scale.to_string()],
    )
    .map_err(|e| format!("還原玩家屬性失敗：{e}"))?;
    if let (Some(px), Some(py)) = (payload.meta.player_x, payload.meta.player_y) {
        tx.execute(
            "INSERT INTO world_meta (key, value) VALUES ('player_x', ?1), ('player_y', ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![px.to_string(), py.to_string()],
        )
        .map_err(|e| format!("還原玩家座標失敗：{e}"))?;
    } else {
        tx.execute("DELETE FROM world_meta WHERE key IN ('player_x', 'player_y')", [])
            .map_err(|e| format!("重設玩家預設座標失敗：{e}"))?;
    }
    if let Some(qc) = &payload.meta.quest_config {
        tx.execute(
            "INSERT INTO world_meta (key, value) VALUES ('quest_config_json', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![qc.to_string()],
        )
        .map_err(|e| format!("還原任務目標設定失敗：{e}"))?;
    } else {
        tx.execute("DELETE FROM world_meta WHERE key = 'quest_config_json'", [])
            .map_err(|e| format!("清除任務目標設定失敗：{e}"))?;
    }
    if let Some(qp) = &payload.meta.quest_progress {
        tx.execute(
            "INSERT INTO world_meta (key, value) VALUES ('quest_progress_json', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![qp.to_string()],
        )
        .map_err(|e| format!("還原任務遊玩進度失敗：{e}"))?;
    } else {
        tx.execute("DELETE FROM world_meta WHERE key = 'quest_progress_json'", [])
            .map_err(|e| format!("清除任務遊玩進度失敗：{e}"))?;
    }

    tx.execute("DELETE FROM terrain_tiles", [])
        .map_err(|e| format!("清除地形失敗：{e}"))?;
    for t in &payload.terrain {
        tx.execute(
            "INSERT INTO terrain_tiles (x, y, elevation, ground_type, road_type, custom_color)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![t.x, t.y, t.elevation, t.ground_type, t.road_type, t.custom_color],
        )
        .map_err(|e| format!("還原地形失敗：{e}"))?;
    }

    tx.execute("DELETE FROM movement_paths", [])
        .map_err(|e| format!("清除路徑失敗：{e}"))?;
    tx.execute("DELETE FROM wander_zones", [])
        .map_err(|e| format!("清除漫遊區失敗：{e}"))?;
    tx.execute("DELETE FROM event_rules", [])
        .map_err(|e| format!("清除事件失敗：{e}"))?;
    tx.execute("DELETE FROM world_entities", [])
        .map_err(|e| format!("清除實體失敗：{e}"))?;

    for e in &payload.entities {
        let sx = if e.scale_x > 0.01 { e.scale_x } else { e.scale };
        let sy = if e.scale_y > 0.01 { e.scale_y } else { e.scale };
        let sz = if e.scale_z > 0.01 { e.scale_z } else { e.scale };
        tx.execute(
            "INSERT INTO world_entities
             (id, asset_id, name, x, y, z, yaw, scale, scale_x, scale_y, scale_z, parent_id, tint_hex, emissive_on, visible, anim_mode, tag, meta_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                e.id, e.asset_id, e.name, e.x, e.y, e.z, e.yaw, e.scale, sx, sy, sz, e.parent_id,
                e.tint_hex, e.emissive_on as i64, e.visible as i64, e.anim_mode, e.tag, e.meta_json
            ],
        )
        .map_err(|err| format!("還原實體失敗：{err}"))?;
    }
    for p in &payload.paths {
        let wp_json = serde_json::to_string(&p.waypoints).unwrap_or_else(|_| "[]".into());
        tx.execute(
            "INSERT INTO movement_paths (id, name, entity_id, loop_mode, speed, smooth, active, color, waypoints_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                p.id, p.name, p.entity_id, p.loop_mode, p.speed, p.smooth as i64, p.active as i64,
                p.color, wp_json
            ],
        )
        .map_err(|err| format!("還原路徑失敗：{err}"))?;
    }
    for z in &payload.wander_zones {
        let ids_json = serde_json::to_string(&z.entity_ids).unwrap_or_else(|_| "[]".into());
        tx.execute(
            "INSERT INTO wander_zones (id, name, cx, cy, radius, speed, entity_ids_json, color)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![z.id, z.name, z.cx, z.cy, z.radius, z.speed, ids_json, z.color],
        )
        .map_err(|err| format!("還原漫遊區失敗：{err}"))?;
    }
    for ev in &payload.events {
        let trig_json = serde_json::to_string(&ev.trigger_config).unwrap_or_else(|_| "{}".into());
        let act_json = serde_json::to_string(&ev.actions).unwrap_or_else(|_| "[]".into());
        tx.execute(
            "INSERT INTO event_rules (id, name, enabled, trigger_type, trigger_config_json, actions_json, cooldown_sec)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![ev.id, ev.name, ev.enabled as i64, ev.trigger_type, trig_json, act_json, ev.cooldown_sec],
        )
        .map_err(|err| format!("還原事件規則失敗：{err}"))?;
    }
    tx.commit().map_err(|e| format!("提交快照還原交易失敗：{e}"))?;
    Ok(())
}

/// 原子性匯出單檔自包含 `.stackworld.db`（透過 SQLite `VACUUM INTO`）
pub fn export_world_db_bytes(path: &Path) -> Result<Vec<u8>, String> {
    if !path.exists() {
        return Err("指定的世界檔案不存在。".to_string());
    }
    let tmp_dir = tempfile::tempdir().map_err(|e| format!("建立暫存目錄失敗：{e}"))?;
    let out_path = tmp_dir.path().join("export.stackworld.db");
    let conn = open_world_conn(path)?;
    let out_str = out_path
        .to_str()
        .ok_or_else(|| "暫存路徑無效。".to_string())?;
    conn.execute("VACUUM INTO ?1", params![out_str])
        .map_err(|e| format!("打包匯出世界檔失敗：{e}"))?;
    fs::read(&out_path).map_err(|e| format!("讀取匯出暫存檔失敗：{e}"))
}

/// 驗證並匯入好友分享的 `.stackworld.db` 單檔世界包
pub fn import_world_db_bytes(
    bytes: &[u8],
    data_dir: &Path,
    preferred_name: Option<&str>,
) -> Result<WorldSummary, String> {
    if bytes.len() < 100 || !bytes.starts_with(b"SQLite format 3\0") {
        return Err("檔案不是合法的 SQLite 世界資料庫（缺少 SQLite 檔頭）。".to_string());
    }

    let tmp_dir = tempfile::tempdir().map_err(|e| format!("建立暫存驗證目錄失敗：{e}"))?;
    let tmp_path = tmp_dir.path().join("incoming.stackworld.db");
    fs::write(&tmp_path, bytes).map_err(|e| format!("寫入驗證暫存檔失敗：{e}"))?;

    let conn = Connection::open(&tmp_path).map_err(|e| format!("無法開啟匯入資料庫：{e}"))?;
    let integrity: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|e| format!("資料庫完整性檢查失敗：{e}"))?;
    if !integrity.eq_ignore_ascii_case("ok") {
        return Err(format!("世界檔案損毀：{integrity}"));
    }

    // 檢查必要資料表
    let required_tables: HashSet<&str> = [
        "world_meta",
        "terrain_tiles",
        "asset_catalog",
        "world_entities",
        "movement_paths",
        "wander_zones",
        "event_rules",
    ]
    .into_iter()
    .collect();
    let mut found_tables = HashSet::new();
    {
        let mut st = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .map_err(|e| format!("讀取資料表結構失敗：{e}"))?;
        let rows = st
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| format!("列舉資料表失敗：{e}"))?;
        for r in rows {
            if let Ok(name) = r {
                found_tables.insert(name);
            }
        }
    }
    for req in required_tables {
        if !found_tables.contains(req) {
            return Err(format!("匯入失敗：世界檔案缺少 `{req}` 資料表。"));
        }
    }

    let meta = read_world_meta(&conn)?;
    if meta.schema_version != SCHEMA_VERSION {
        return Err(format!("不支援的世界版本：{}", meta.schema_version));
    }

    // 逐筆檢驗內嵌 PNG BLOB 合法性並同步收錄進個人跨世界資產庫
    {
        let lib_conn = Connection::open(data_dir.join(LIBRARY_DB_NAME)).ok();
        let mut st = conn
            .prepare(
                "SELECT name, category, source_type, blueprint_json, slices_png, normals_png, emissive_png,
                        slice_count, default_scale, anim_preset
                 FROM asset_catalog",
            )
            .map_err(|e| format!("檢查內嵌資產失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Vec<u8>>(4)?,
                    r.get::<_, Vec<u8>>(5)?,
                    r.get::<_, Vec<u8>>(6)?,
                    r.get::<_, i32>(7)?,
                    r.get::<_, f32>(8)?,
                    r.get::<_, String>(9)?,
                ))
            })
            .map_err(|e| format!("遍歷內嵌資產失敗：{e}"))?;

        for row in rows {
            let (name, cat, src, bp, slices, norms, emis, cnt, scale, anim) =
                row.map_err(|e| format!("讀取資產 BLOB 失敗：{e}"))?;
            let baked = bake_from_strip_bytes(
                &name,
                &cat,
                &slices,
                cnt.max(1) as usize,
                Some(&norms),
                Some(&emis),
                bp,
                scale,
                &anim,
            )?;
            if let Some(ref lc) = lib_conn {
                let _ = upsert_library_asset(lc, &baked, &src);
            }
        }
    }
    drop(conn);

    // 產生不衝突的目標檔名並寫入 worlds/
    let base_label = preferred_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&meta.name);
    let mut candidate = sanitize_world_filename(base_label)?;
    let mut dest = data_dir.join(&candidate);
    if dest.exists() {
        let stem = candidate.trim_end_matches(WORLD_EXT);
        let short = &Uuid::new_v4().to_string()[..6];
        candidate = format!("{stem}_{short}{WORLD_EXT}");
        dest = data_dir.join(&candidate);
    }
    fs::write(&dest, bytes).map_err(|e| format!("寫入匯入世界檔失敗：{e}"))?;

    let final_conn = open_world_conn(&dest)?;
    let _ = set_meta_kv(&final_conn, "imported_world", "1");
    let has_initial_snap: bool = final_conn
        .query_row(
            "SELECT 1 FROM world_snapshots WHERE label = '__initial__' LIMIT 1",
            [],
            |_| Ok(true),
        )
        .optional()
        .unwrap_or(None)
        .unwrap_or(false);
    let final_meta = read_world_meta(&final_conn)?;
    let entity_count: i64 = final_conn
        .query_row("SELECT COUNT(*) FROM world_entities", [], |r| r.get(0))
        .unwrap_or(0);
    let asset_count: i64 = final_conn
        .query_row("SELECT COUNT(*) FROM asset_catalog", [], |r| r.get(0))
        .unwrap_or(0);
    drop(final_conn);

    if !has_initial_snap {
        let _ = create_snapshot(&dest, &candidate, "__initial__");
    }

    Ok(WorldSummary {
        file_name: candidate,
        world_id: final_meta.world_id,
        name: final_meta.name,
        author: final_meta.author,
        description: final_meta.description,
        theme: final_meta.theme,
        width: final_meta.width,
        height: final_meta.height,
        entity_count,
        asset_count,
        updated_at: final_meta.updated_at,
        thumbnail_b64: final_meta.thumbnail_b64,
    })
}

/// 儲存自訂背景音樂至單檔 SQLite `world_bgm` 表
pub fn save_world_bgm(
    conn: &Connection,
    name: &str,
    mime_type: &str,
    data: &[u8],
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO world_bgm (id, name, mime_type, data, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           mime_type = excluded.mime_type,
           data = excluded.data,
           updated_at = excluded.updated_at",
        params![name, mime_type, data, now_iso()],
    )
    .map_err(|e| format!("儲存自訂背景音樂失敗：{e}"))?;
    touch_world_updated(conn);
    Ok(())
}

/// 讀取世界內建自訂背景音樂
pub fn read_world_bgm(conn: &Connection) -> Result<Option<(String, String, Vec<u8>)>, String> {
    conn.query_row(
        "SELECT name, mime_type, data FROM world_bgm WHERE id = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
    .map_err(|e| format!("讀取自訂背景音樂失敗：{e}"))
}

/// 移除世界自訂背景音樂
pub fn delete_world_bgm(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM world_bgm WHERE id = 1", [])
        .map_err(|e| format!("刪除自訂背景音樂失敗：{e}"))?;
    touch_world_updated(conn);
    Ok(())
}

/// 擴展或裁切世界地圖尺寸（可對北／南／西／東四側增減格數），並自動平移全部實體、路徑、漫遊區與事件座標
pub fn resize_world(
    path: &Path,
    add_north: i32,
    add_south: i32,
    add_west: i32,
    add_east: i32,
    fill_ground: &str,
    fill_elevation: i32,
) -> Result<WorldMeta, String> {
    let mut conn = open_world_conn(path)?;
    let meta = read_world_meta(&conn)?;
    let new_w = meta.width + add_west + add_east;
    let new_h = meta.height + add_north + add_south;
    if !(16..=96).contains(&new_w) || !(16..=96).contains(&new_h) {
        return Err(format!(
            "調整後地圖尺寸 ({new_w}×{new_h}) 超出安全範圍（16..96 格）。"
        ));
    }

    let elev_clamped = fill_elevation.clamp(0, 6);
    let dx = add_west;
    let dy = add_north;

    // 1. 讀取既有地形建立新網格
    let mut old_tiles: HashMap<(i32, i32), TerrainTile> = HashMap::new();
    {
        let mut st = conn
            .prepare("SELECT x, y, elevation, ground_type, road_type, custom_color FROM terrain_tiles")
            .map_err(|e| format!("讀取既有地形失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
                Ok(TerrainTile {
                    x: r.get(0)?,
                    y: r.get(1)?,
                    elevation: r.get(2)?,
                    ground_type: r.get(3)?,
                    road_type: r.get(4)?,
                    custom_color: r.get(5)?,
                })
            })
            .map_err(|e| format!("遍歷既有地形失敗：{e}"))?;
        for r in rows {
            let t = r.map_err(|e| format!("解析地形格點失敗：{e}"))?;
            old_tiles.insert((t.x + dx, t.y + dy), t);
        }
    }

    // 2. 讀出路徑、事件等需要平移座標的 JSON 記錄
    let path_rows: Vec<(String, String)> = {
        let mut st = conn
            .prepare("SELECT id, waypoints_json FROM movement_paths")
            .map_err(|e| format!("讀取移動路徑失敗：{e}"))?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| format!("查詢移動路徑失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析移動路徑失敗：{e}"))?
    };

    let event_rows: Vec<(String, String, String)> = {
        let mut st = conn
            .prepare("SELECT id, trigger_config_json, actions_json FROM event_rules")
            .map_err(|e| format!("讀取事件規則失敗：{e}"))?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("查詢事件規則失敗：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析事件規則失敗：{e}"))?
    };

    let tx = conn
        .transaction()
        .map_err(|e| format!("開啟擴展地圖交易失敗：{e}"))?;

    tx.execute("DELETE FROM terrain_tiles", [])
        .map_err(|e| format!("重建地形網格失敗：{e}"))?;
    for y in 0..new_h {
        for x in 0..new_w {
            if let Some(t) = old_tiles.get(&(x, y)) {
                tx.execute(
                    "INSERT INTO terrain_tiles (x, y, elevation, ground_type, road_type, custom_color)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![x, y, t.elevation, t.ground_type, t.road_type, t.custom_color],
                )
                .map_err(|e| format!("寫入遷移地形失敗：{e}"))?;
            } else {
                tx.execute(
                    "INSERT INTO terrain_tiles (x, y, elevation, ground_type, road_type, custom_color)
                     VALUES (?1, ?2, ?3, ?4, 'none', NULL)",
                    params![x, y, elev_clamped, fill_ground],
                )
                .map_err(|e| format!("寫入新增邊界地形失敗：{e}"))?;
            }
        }
    }

    let dxf = dx as f64;
    let dyf = dy as f64;
    if dx != 0 || dy != 0 {
        tx.execute(
            "UPDATE world_entities SET
               x = MIN(?3, MAX(0.5, x + ?1)),
               y = MIN(?4, MAX(0.5, y + ?2))",
            params![dxf, dyf, new_w as f64 - 0.5, new_h as f64 - 0.5],
        )
        .map_err(|e| format!("平移實體座標失敗：{e}"))?;

        tx.execute(
            "UPDATE wander_zones SET
               cx = MIN(?3, MAX(0.5, cx + ?1)),
               cy = MIN(?4, MAX(0.5, cy + ?2))",
            params![dxf, dyf, new_w as f64 - 0.5, new_h as f64 - 0.5],
        )
        .map_err(|e| format!("平移漫遊區域座標失敗：{e}"))?;

        for (pid, wp_str) in path_rows {
            if let Ok(mut wps) = serde_json::from_str::<Vec<Waypoint>>(&wp_str) {
                for w in &mut wps {
                    w.x = (w.x + dxf).clamp(0.5, new_w as f64 - 0.5);
                    w.y = (w.y + dyf).clamp(0.5, new_h as f64 - 0.5);
                }
                let next_wp = serde_json::to_string(&wps).unwrap_or(wp_str);
                tx.execute(
                    "UPDATE movement_paths SET waypoints_json = ?2 WHERE id = ?1",
                    params![pid, next_wp],
                )
                .map_err(|e| format!("平移路徑節點失敗：{e}"))?;
            }
        }

        for (eid, trig_str, act_str) in event_rows {
            let mut trig: Value = serde_json::from_str(&trig_str).unwrap_or(Value::Null);
            shift_xy_in_json(&mut trig, dxf, dyf);
            let mut acts: Value = serde_json::from_str(&act_str).unwrap_or(Value::Array(vec![]));
            if let Some(arr) = acts.as_array_mut() {
                for a in arr {
                    shift_xy_in_json(a, dxf, dyf);
                }
            }
            tx.execute(
                "UPDATE event_rules SET trigger_config_json = ?2, actions_json = ?3 WHERE id = ?1",
                params![eid, trig.to_string(), acts.to_string()],
            )
            .map_err(|e| format!("平移事件座標失敗：{e}"))?;
        }

        if let Some(px) = meta.player_x {
            let npx = (px + dxf).clamp(0.5, new_w as f64 - 0.5);
            tx.execute(
                "INSERT INTO world_meta (key, value) VALUES ('player_x', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![npx.to_string()],
            )
            .map_err(|e| format!("平移玩家 X 座標失敗：{e}"))?;
        }
        if let Some(py) = meta.player_y {
            let npy = (py + dyf).clamp(0.5, new_h as f64 - 0.5);
            tx.execute(
                "INSERT INTO world_meta (key, value) VALUES ('player_y', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![npy.to_string()],
            )
            .map_err(|e| format!("平移玩家 Y 座標失敗：{e}"))?;
        }
    }

    tx.execute(
        "INSERT INTO world_meta (key, value) VALUES ('width', ?1), ('height', ?2), ('updated_at', ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![new_w.to_string(), new_h.to_string(), now_iso()],
    )
    .map_err(|e| format!("更新世界尺寸失敗：{e}"))?;

    tx.commit()
        .map_err(|e| format!("提交擴展地圖交易失敗：{e}"))?;
    read_world_meta(&conn)
}

fn shift_xy_in_json(v: &mut Value, dx: f64, dy: f64) {
    if let Some(obj) = v.as_object_mut() {
        if let Some(x) = obj.get("x").and_then(|x| x.as_f64()) {
            obj.insert("x".to_string(), serde_json::json!(x + dx));
        }
        if let Some(y) = obj.get("y").and_then(|y| y.as_f64()) {
            obj.insert("y".to_string(), serde_json::json!(y + dy));
        }
    }
}

/// 重置遊戲遊玩進度（保留自建世界）或完整回復世界初始預設狀態
pub fn reset_world_state(
    data_dir: &Path,
    path: &Path,
    mode: &str,
    safe_file_name: &str,
) -> Result<WorldFullState, String> {
    if !path.exists() {
        return Err("找不到指定的世界檔案。".to_string());
    }

    match mode {
        "progress_only" => {
            let conn = open_world_conn(path)?;
            conn.execute(
                "DELETE FROM world_meta WHERE key IN ('quest_progress_json', 'player_x', 'player_y')",
                [],
            )
            .map_err(|e| format!("清除遊玩進度失敗：{e}"))?;

            let init_tod: Option<String> = conn
                .query_row(
                    "SELECT value FROM world_meta WHERE key = 'initial_time_of_day'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .unwrap_or(None);
            if let Some(tod) = init_tod {
                set_meta_kv(&conn, "time_of_day", &tod)?;
            }
            set_meta_kv(&conn, "weather", "clear")?;

            // 還原因搶劫敗逃或破關而暫時隱藏的任務角色／物件
            conn.execute(
                "UPDATE world_entities SET visible = 1 WHERE visible = 0 AND meta_json LIKE '%\"hide_on_defeat\":true%'",
                [],
            )
            .map_err(|e| format!("恢復任務物件顯示狀態失敗：{e}"))?;

            touch_world_updated(&conn);
            drop(conn);
            load_full_world_state(path, safe_file_name)
        }
        "world_initial" => {
            let conn = open_world_conn(path)?;
            let meta = read_world_meta(&conn)?;
            let imported_flag: Option<String> = conn
                .query_row(
                    "SELECT value FROM world_meta WHERE key = 'imported_world'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .unwrap_or(None);
            let initial_snap_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM world_snapshots WHERE label = '__initial__' ORDER BY created_at ASC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .unwrap_or(None);
            drop(conn);

            if imported_flag.as_deref() == Some("1") {
                if let Some(snap_id) = initial_snap_id {
                    restore_snapshot(path, &snap_id)?;
                    let c2 = open_world_conn(path)?;
                    let _ = c2.execute(
                        "DELETE FROM world_meta WHERE key IN ('quest_progress_json', 'player_x', 'player_y')",
                        [],
                    );
                    touch_world_updated(&c2);
                    drop(c2);
                    return load_full_world_state(path, safe_file_name);
                }
            }

            presets::rebuild_world_in_place(
                data_dir,
                path,
                &meta.theme,
                &meta.name,
                &meta.author,
                &meta.world_id,
            )?;
            load_full_world_state(path, safe_file_name)
        }
        _ => Err("不支援的重置模式，請使用 progress_only 或 world_initial。".to_string()),
    }
}

/// 讀取指定世界所有資產的 PNG 貼圖並編碼為 Data URL（供單純遊戲獨立網頁版內嵌）
pub fn load_all_asset_textures_base64(
    path: &Path,
) -> Result<HashMap<String, serde_json::Value>, String> {
    let conn = open_world_conn(path)?;
    let mut stmt = conn
        .prepare("SELECT id, slices_png, normals_png, emissive_png FROM asset_catalog")
        .map_err(|e| format!("查詢資產貼圖失敗：{e}"))?;
    let rows = stmt
        .query_map([], |r| {
            let id: String = r.get(0)?;
            let slices: Vec<u8> = r.get(1)?;
            let normals: Vec<u8> = r.get(2)?;
            let emissive: Vec<u8> = r.get(3)?;
            Ok((id, slices, normals, emissive))
        })
        .map_err(|e| format!("讀取資產貼圖列失敗：{e}"))?;

    let mut map = HashMap::new();
    for row in rows {
        let (id, slices, normals, emissive) =
            row.map_err(|e| format!("解析資產貼圖失敗：{e}"))?;
        map.insert(
            id,
            serde_json::json!({
                "slices": format!("data:image/png;base64,{}", STANDARD.encode(slices)),
                "normals": format!("data:image/png;base64,{}", STANDARD.encode(normals)),
                "emissive": format!("data:image/png;base64,{}", STANDARD.encode(emissive)),
            }),
        );
    }
    Ok(map)
}

/// 讀取世界自訂背景音樂並編碼為 Data URL（供單純遊戲獨立網頁版離線播放）
pub fn read_world_bgm_data_url(path: &Path) -> Result<Option<String>, String> {
    let conn = open_world_conn(path)?;
    let Some((_name, mime, bytes)) = read_world_bgm(&conn)? else {
        return Ok(None);
    };
    Ok(Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes))))
}

/// 更新個人跨世界資產庫（`_library.db`）的名稱、分類、動畫、縮放或重烘後的積木藍圖
pub fn update_library_asset(
    data_dir: &Path,
    asset_id: &str,
    name: Option<&str>,
    category: Option<&str>,
    anim_preset: Option<&str>,
    default_scale: Option<f32>,
    rebaked: Option<&BakedStack>,
) -> Result<LibraryAssetItem, String> {
    let lib_path = data_dir.join(LIBRARY_DB_NAME);
    let conn = Connection::open(&lib_path).map_err(|e| format!("開啟個人資產庫失敗：{e}"))?;
    let existing: Option<(String, String, String, Option<String>, i32, i32, i32, f32, String, String, String)> = conn
        .query_row(
            "SELECT name, category, source_type, blueprint_json, slice_count, slice_w, slice_h,
                    default_scale, anim_preset, sha256, created_at
             FROM asset_catalog WHERE id = ?1",
            params![asset_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                ))
            },
        )
        .optional()
        .map_err(|e| format!("查詢個人資產庫項目失敗：{e}"))?;

    let Some((
        old_name,
        old_cat,
        source_type,
        old_bp,
        old_sc,
        old_sw,
        old_sh,
        old_scale,
        old_anim,
        old_sha,
        created_at,
    )) = existing
    else {
        return Err("個人資產庫中找不到指定的物件。".to_string());
    };

    let next_name = name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| rebaked.map(|b| b.title.clone()).unwrap_or(old_name));
    let next_cat = category
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or(old_cat);
    let next_anim = anim_preset
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or(old_anim);
    let next_scale = default_scale.unwrap_or(old_scale);

    if let Some(baked) = rebaked {
        // 若已有其他條目擁有相同 sha256，確保不會觸發 UNIQUE(sha256) 衝突
        let dup_id: Option<String> = conn
            .query_row(
                "SELECT id FROM asset_catalog WHERE sha256 = ?1 AND id != ?2",
                params![baked.sha256, asset_id],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or(None);
        let effective_sha = if dup_id.is_some() {
            format!("{}-{}", baked.sha256, &asset_id[..asset_id.len().min(8)])
        } else {
            baked.sha256.clone()
        };
        conn.execute(
            "UPDATE asset_catalog SET
                name = ?2, category = ?3, blueprint_json = ?4,
                slices_png = ?5, normals_png = ?6, emissive_png = ?7,
                slice_count = ?8, slice_w = ?9, slice_h = ?10,
                default_scale = ?11, anim_preset = ?12, sha256 = ?13
             WHERE id = ?1",
            params![
                asset_id,
                next_name,
                next_cat,
                baked.blueprint_json,
                baked.slices_png,
                baked.normals_png,
                baked.emissive_png,
                baked.slices as i32,
                baked.slice_w as i32,
                baked.slice_h as i32,
                next_scale,
                next_anim,
                effective_sha,
            ],
        )
        .map_err(|e| format!("更新個人資產庫貼圖失敗：{e}"))?;

        return Ok(LibraryAssetItem {
            id: asset_id.to_string(),
            name: next_name,
            category: next_cat,
            source_type,
            blueprint_json: baked.blueprint_json.clone(),
            slice_count: baked.slices as i32,
            slice_w: baked.slice_w as i32,
            slice_h: baked.slice_h as i32,
            default_scale: next_scale,
            anim_preset: next_anim,
            sha256: effective_sha,
            created_at,
        });
    }

    let next_bp = old_bp.map(|raw| {
        if let Ok(mut v) = serde_json::from_str::<Value>(&raw) {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("title".to_string(), Value::String(next_name.clone()));
                return v.to_string();
            }
        }
        raw
    });

    conn.execute(
        "UPDATE asset_catalog SET name = ?2, category = ?3, default_scale = ?4, anim_preset = ?5, blueprint_json = ?6 WHERE id = ?1",
        params![asset_id, next_name, next_cat, next_scale, next_anim, next_bp],
    )
    .map_err(|e| format!("更新個人資產庫屬性失敗：{e}"))?;

    Ok(LibraryAssetItem {
        id: asset_id.to_string(),
        name: next_name,
        category: next_cat,
        source_type,
        blueprint_json: next_bp,
        slice_count: old_sc,
        slice_w: old_sw,
        slice_h: old_sh,
        default_scale: next_scale,
        anim_preset: next_anim,
        sha256: old_sha,
        created_at,
    })
}

/// 從個人跨世界資產庫（`_library.db`）刪除指定物件
pub fn delete_library_asset(data_dir: &Path, asset_id: &str) -> Result<bool, String> {
    let lib_path = data_dir.join(LIBRARY_DB_NAME);
    let conn = Connection::open(&lib_path).map_err(|e| format!("開啟個人資產庫失敗：{e}"))?;
    let affected = conn
        .execute("DELETE FROM asset_catalog WHERE id = ?1", params![asset_id])
        .map_err(|e| format!("刪除個人資產庫項目失敗：{e}"))?;
    Ok(affected > 0)
}

/// 更新單一世界資料庫內指定資產之名稱、分類、動畫、縮放或重烘後的 3D 積木藍圖
pub fn update_world_asset(
    conn: &Connection,
    data_dir: &Path,
    asset_id: &str,
    name: Option<&str>,
    category: Option<&str>,
    anim_preset: Option<&str>,
    default_scale: Option<f32>,
    rebaked: Option<&BakedStack>,
) -> Result<AssetMeta, String> {
    let existing: Option<(String, String, String, Option<String>, i32, i32, i32, f32, String, String, String)> = conn
        .query_row(
            "SELECT name, category, source_type, blueprint_json, slice_count, slice_w, slice_h,
                    default_scale, anim_preset, sha256, created_at
             FROM asset_catalog WHERE id = ?1",
            params![asset_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                ))
            },
        )
        .optional()
        .map_err(|e| format!("查詢世界資產失敗：{e}"))?;

    let Some((
        old_name,
        old_cat,
        source_type,
        old_bp,
        old_sc,
        old_sw,
        old_sh,
        old_scale,
        old_anim,
        old_sha,
        created_at,
    )) = existing
    else {
        return Err("此世界內找不到指定的資產。".to_string());
    };

    let next_name = name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| rebaked.map(|b| b.title.clone()).unwrap_or(old_name));
    let next_cat = category
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or(old_cat);
    let next_anim = anim_preset
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or(old_anim);
    let next_scale = default_scale.unwrap_or(old_scale);

    if let Some(baked) = rebaked {
        let dup_id: Option<String> = conn
            .query_row(
                "SELECT id FROM asset_catalog WHERE sha256 = ?1 AND id != ?2",
                params![baked.sha256, asset_id],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or(None);
        let effective_sha = if dup_id.is_some() {
            format!("{}-{}", baked.sha256, &asset_id[..asset_id.len().min(8)])
        } else {
            baked.sha256.clone()
        };

        conn.execute(
            "UPDATE asset_catalog SET
                name = ?2, category = ?3, blueprint_json = ?4,
                slices_png = ?5, normals_png = ?6, emissive_png = ?7,
                slice_count = ?8, slice_w = ?9, slice_h = ?10,
                default_scale = ?11, anim_preset = ?12, sha256 = ?13
             WHERE id = ?1",
            params![
                asset_id,
                next_name,
                next_cat,
                baked.blueprint_json,
                baked.slices_png,
                baked.normals_png,
                baked.emissive_png,
                baked.slices as i32,
                baked.slice_w as i32,
                baked.slice_h as i32,
                next_scale,
                next_anim,
                effective_sha,
            ],
        )
        .map_err(|e| format!("更新世界資產貼圖失敗：{e}"))?;

        // 同步將更新後的版本寫入個人資產庫
        if let Ok(lib_conn) = Connection::open(data_dir.join(LIBRARY_DB_NAME)) {
            let _ = upsert_library_asset(&lib_conn, baked, &source_type);
        }

        touch_world_updated(conn);
        return Ok(AssetMeta {
            id: asset_id.to_string(),
            name: next_name,
            category: next_cat,
            source_type,
            blueprint_json: baked.blueprint_json.clone(),
            slice_count: baked.slices as i32,
            slice_w: baked.slice_w as i32,
            slice_h: baked.slice_h as i32,
            default_scale: next_scale,
            anim_preset: next_anim,
            sha256: effective_sha,
            created_at,
        });
    }

    let next_bp = old_bp.map(|raw| {
        if let Ok(mut v) = serde_json::from_str::<Value>(&raw) {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("title".to_string(), Value::String(next_name.clone()));
                return v.to_string();
            }
        }
        raw
    });

    conn.execute(
        "UPDATE asset_catalog SET name = ?2, category = ?3, default_scale = ?4, anim_preset = ?5, blueprint_json = ?6 WHERE id = ?1",
        params![asset_id, next_name, next_cat, next_scale, next_anim, next_bp],
    )
    .map_err(|e| format!("更新世界資產屬性失敗：{e}"))?;
    touch_world_updated(conn);

    Ok(AssetMeta {
        id: asset_id.to_string(),
        name: next_name,
        category: next_cat,
        source_type,
        blueprint_json: next_bp,
        slice_count: old_sc,
        slice_w: old_sw,
        slice_h: old_sh,
        default_scale: next_scale,
        anim_preset: next_anim,
        sha256: old_sha,
        created_at,
    })
}

/// 刪除世界內的指定資產，並安全清理所有使用該資產的場景實體與玩家角色引用
pub fn delete_world_asset(conn: &Connection, asset_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM world_entities WHERE asset_id = ?1")
        .map_err(|e| format!("查詢關聯實體失敗：{e}"))?;
    let entity_ids: Vec<String> = stmt
        .query_map(params![asset_id], |r| r.get(0))
        .map_err(|e| format!("讀取關聯實體失敗：{e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("解析關聯實體失敗：{e}"))?;
    drop(stmt);

    // 解除其他物件對此資產所屬載具／建築的 parent_id 附掛，避免殘留孤立參照
    for eid in &entity_ids {
        let _ = conn.execute(
            "UPDATE world_entities SET parent_id = NULL WHERE parent_id = ?1",
            params![eid],
        );
    }

    // 若玩家化身正使用被刪除的資產，改換為其他可用人物／動物，或清除指定
    let current_player: Option<String> = conn
        .query_row(
            "SELECT value FROM world_meta WHERE key = 'player_asset_id'",
            [],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);
    if current_player.as_deref() == Some(asset_id) {
        let fallback: Option<String> = conn
            .query_row(
                "SELECT id FROM asset_catalog WHERE id != ?1 AND category IN ('character', 'animal') ORDER BY name LIMIT 1",
                params![asset_id],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or(None);
        if let Some(fb) = fallback {
            let _ = set_meta_kv(conn, "player_asset_id", &fb);
        } else {
            let _ = conn.execute("DELETE FROM world_meta WHERE key = 'player_asset_id'", []);
        }
    }

    let affected = conn
        .execute("DELETE FROM asset_catalog WHERE id = ?1", params![asset_id])
        .map_err(|e| format!("刪除世界資產失敗：{e}"))?;
    if affected == 0 {
        return Err("此世界內找不到指定的資產。".to_string());
    }
    touch_world_updated(conn);
    Ok(entity_ids)
}

/// 關聯世界節點摘要（供門傳送掃描、多世界獨立 HTML 與 `.stackpack.zip` 打包）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedWorldEntry {
    pub world_id: String,
    pub file_name: String,
    pub name: String,
    pub theme: String,
    #[serde(skip)]
    pub path: PathBuf,
}

/// 從指定入口世界出發進行 BFS 圖形掃描（含循環防呆），找出所有透過「門 (`portal`)」連結的關聯世界
pub fn collect_linked_worlds(
    data_dir: &Path,
    entry_file: &str,
) -> Result<Vec<LinkedWorldEntry>, String> {
    let entry_path = world_path(data_dir, entry_file)?;
    if !entry_path.exists() {
        return Err("找不到指定的入口世界檔案。".to_string());
    }
    let entry_safe = entry_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(entry_file)
        .to_string();

    let all_summaries = list_worlds(data_dir)?;
    let mut by_id: HashMap<String, LinkedWorldEntry> = HashMap::new();
    let mut by_file: HashMap<String, LinkedWorldEntry> = HashMap::new();

    for sum in all_summaries {
        let p = data_dir.join(&sum.file_name);
        let item = LinkedWorldEntry {
            world_id: sum.world_id.clone(),
            file_name: sum.file_name.clone(),
            name: sum.name,
            theme: sum.theme,
            path: p,
        };
        by_id.insert(sum.world_id, item.clone());
        by_file.insert(sum.file_name, item);
    }

    let entry_item = by_file.get(&entry_safe).cloned().ok_or_else(|| {
        "無法讀取入口世界的中繼資料。".to_string()
    })?;

    let mut visited_ids: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<LinkedWorldEntry> = VecDeque::new();
    let mut result: Vec<LinkedWorldEntry> = Vec::new();

    visited_ids.insert(entry_item.world_id.clone());
    queue.push_back(entry_item);

    while let Some(cur) = queue.pop_front() {
        if let Ok(conn) = open_world_conn(&cur.path) {
            if let Ok(mut st) = conn.prepare("SELECT meta_json FROM world_entities") {
                if let Ok(rows) = st.query_map([], |r| r.get::<_, String>(0)) {
                    for raw_meta in rows.flatten() {
                        let Ok(v) = serde_json::from_str::<Value>(&raw_meta) else {
                            continue;
                        };
                        let Some(portal) = v.get("portal") else {
                            continue;
                        };
                        if portal.get("enabled").and_then(|b| b.as_bool()) != Some(true) {
                            continue;
                        }
                        let next_item = portal
                            .get("target_world_id")
                            .and_then(|s| s.as_str())
                            .filter(|s| !s.is_empty())
                            .and_then(|wid| by_id.get(wid))
                            .or_else(|| {
                                portal
                                    .get("target_world_file")
                                    .and_then(|s| s.as_str())
                                    .filter(|s| !s.is_empty())
                                    .and_then(|fname| by_file.get(fname))
                            });
                        if let Some(nxt) = next_item {
                            if !visited_ids.contains(&nxt.world_id) {
                                visited_ids.insert(nxt.world_id.clone());
                                queue.push_back(nxt.clone());
                            }
                        }
                    }
                }
            }
        }
        result.push(cur);
    }

    Ok(result)
}

/// 依資產 `sha256` 將貼圖 Data URL 去重寫入共用貼圖池（供多世界獨立 `.html` 縮減體積）
pub fn load_asset_textures_by_sha256(
    path: &Path,
    pool: &mut HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    let conn = open_world_conn(path)?;
    let mut stmt = conn
        .prepare("SELECT sha256, slices_png, normals_png, emissive_png FROM asset_catalog")
        .map_err(|e| format!("查詢資產貼圖失敗：{e}"))?;
    let rows = stmt
        .query_map([], |r| {
            let sha: String = r.get(0)?;
            let slices: Vec<u8> = r.get(1)?;
            let normals: Vec<u8> = r.get(2)?;
            let emissive: Vec<u8> = r.get(3)?;
            Ok((sha, slices, normals, emissive))
        })
        .map_err(|e| format!("讀取資產貼圖列失敗：{e}"))?;

    for row in rows {
        let (sha, slices, normals, emissive) =
            row.map_err(|e| format!("解析資產貼圖失敗：{e}"))?;
        if pool.contains_key(&sha) {
            continue;
        }
        pool.insert(
            sha,
            serde_json::json!({
                "slices": format!("data:image/png;base64,{}", STANDARD.encode(slices)),
                "normals": format!("data:image/png;base64,{}", STANDARD.encode(normals)),
                "emissive": format!("data:image/png;base64,{}", STANDARD.encode(emissive)),
            }),
        );
    }
    Ok(())
}

/// 將入口世界及其所有透過「門」相連的關聯世界打包為單一 `.stackpack.zip` 多世界編輯包
pub fn export_world_pack_zip_bytes(
    data_dir: &Path,
    entry_file: &str,
) -> Result<(Vec<u8>, String, usize), String> {
    let linked = collect_linked_worlds(data_dir, entry_file)?;
    if linked.is_empty() {
        return Err("沒有可匯出的世界資料。".to_string());
    }
    let entry_name = linked[0].name.clone();
    let entry_world_id = linked[0].world_id.clone();
    let entry_file_name = linked[0].file_name.clone();
    let count = linked.len();

    let manifest = serde_json::json!({
        "format": "stackcity-world-pack",
        "version": 1,
        "entry_world_id": entry_world_id,
        "entry_file": entry_file_name,
        "exported_at": now_iso(),
        "worlds": linked.iter().map(|w| serde_json::json!({
            "world_id": w.world_id,
            "file_name": w.file_name,
            "name": w.name,
            "theme": w.theme,
        })).collect::<Vec<_>>(),
    });

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("manifest.json", opts)
        .map_err(|e| format!("建立多世界清單失敗：{e}"))?;
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|e| format!("序列化多世界清單失敗：{e}"))?;
    zip.write_all(&manifest_bytes)
        .map_err(|e| format!("寫入多世界清單失敗：{e}"))?;

    for item in &linked {
        let db_bytes = export_world_db_bytes(&item.path)?;
        let entry_path_in_zip = format!("worlds/{}", item.file_name);
        zip.start_file(&entry_path_in_zip, opts)
            .map_err(|e| format!("建立多世界壓縮項目失敗 ({})：{e}", item.file_name))?;
        zip.write_all(&db_bytes)
            .map_err(|e| format!("寫入多世界壓縮資料失敗 ({})：{e}", item.file_name))?;
    }

    let cursor = zip
        .finish()
        .map_err(|e| format!("完成多世界編輯包壓縮失敗：{e}"))?;
    Ok((cursor.into_inner(), entry_name, count))
}

/// 驗證並匯入 `.stackpack.zip` 多世界編輯包，自動重新指派全新 World UUID 並重寫所有門 (`portal`) 的雙向連結
pub fn import_world_pack_zip_bytes(
    bytes: &[u8],
    data_dir: &Path,
) -> Result<(WorldSummary, Vec<WorldSummary>), String> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("無法解析多世界編輯包（ZIP 損毀或格式錯誤）：{e}"))?;

    let mut manifest_opt: Option<Value> = None;
    let mut db_entries: Vec<(String, Vec<u8>)> = Vec::new();

    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| format!("讀取壓縮項目失敗：{e}"))?;
        if f.is_dir() {
            continue;
        }
        let raw_name = f.name().replace('\\', "/");
        if raw_name == "manifest.json" {
            let mut text = String::new();
            if f.read_to_string(&mut text).is_ok() {
                manifest_opt = serde_json::from_str(&text).ok();
            }
            continue;
        }
        if raw_name.ends_with(WORLD_EXT) {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)
                .map_err(|e| format!("解壓世界檔失敗 ({raw_name})：{e}"))?;
            let short_file = Path::new(&raw_name)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&raw_name)
                .to_string();
            db_entries.push((short_file, buf));
        }
    }

    if db_entries.is_empty() {
        return Err("多世界編輯包內找不到任何 `.stackworld.db` 世界檔。".to_string());
    }

    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut file_map: HashMap<String, String> = HashMap::new();
    let mut imported: Vec<(String, WorldSummary)> = Vec::new(); // (old_world_id, summary)

    for (orig_file, db_bytes) in db_entries {
        // 先讀取原檔內的 old_world_id 以便重寫所有 portal 引用
        let tmp_dir = tempfile::tempdir().map_err(|e| format!("建立暫存目錄失敗：{e}"))?;
        let tmp_p = tmp_dir.path().join("peek.stackworld.db");
        fs::write(&tmp_p, &db_bytes).map_err(|e| format!("寫入暫存世界失敗：{e}"))?;
        let old_conn = open_world_conn(&tmp_p)?;
        let old_meta = read_world_meta(&old_conn)?;
        drop(old_conn);

        let mut summary = import_world_db_bytes(&db_bytes, data_dir, Some(&old_meta.name))?;
        let new_world_id = Uuid::new_v4().to_string();
        let dest_path = data_dir.join(&summary.file_name);
        {
            let conn = open_world_conn(&dest_path)?;
            set_meta_kv(&conn, "world_id", &new_world_id)?;
            touch_world_updated(&conn);
        }
        summary.world_id = new_world_id.clone();

        id_map.insert(old_meta.world_id.clone(), new_world_id);
        file_map.insert(orig_file, summary.file_name.clone());
        imported.push((old_meta.world_id, summary));
    }

    // 全部世界落地並取得新 UUID 後，遍歷重寫所有門的目標 ID 與目標檔名，並刷新 __initial__ 快照
    for (_, sum) in &imported {
        let dest_path = data_dir.join(&sum.file_name);
        let mut conn = open_world_conn(&dest_path)?;
        let updates: Vec<(String, String)> = {
            let mut st = conn
                .prepare("SELECT id, meta_json FROM world_entities")
                .map_err(|e| format!("讀取匯入世界實體失敗：{e}"))?;
            let rows = st
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| format!("列舉匯入世界實體失敗：{e}"))?;
            let mut list = Vec::new();
            for row in rows.flatten() {
                let (eid, raw_meta) = row;
                let Ok(mut root) = serde_json::from_str::<Value>(&raw_meta) else {
                    continue;
                };
                let Some(portal) = root.get_mut("portal").and_then(|p| p.as_object_mut()) else {
                    continue;
                };
                let mut changed = false;
                if let Some(old_tid) = portal
                    .get("target_world_id")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string)
                {
                    if let Some(new_tid) = id_map.get(&old_tid) {
                        portal.insert("target_world_id".to_string(), Value::String(new_tid.clone()));
                        changed = true;
                    }
                }
                if let Some(old_tfile) = portal
                    .get("target_world_file")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string)
                {
                    if let Some(new_tfile) = file_map.get(&old_tfile) {
                        portal.insert(
                            "target_world_file".to_string(),
                            Value::String(new_tfile.clone()),
                        );
                        changed = true;
                    }
                }
                if changed {
                    if let Ok(next_str) = serde_json::to_string(&root) {
                        list.push((eid, next_str));
                    }
                }
            }
            list
        };

        if !updates.is_empty() {
            let tx = conn.transaction().map_err(|e| format!("開啟更新門連結交易失敗：{e}"))?;
            for (eid, meta_str) in updates {
                tx.execute(
                    "UPDATE world_entities SET meta_json = ?2 WHERE id = ?1",
                    params![eid, meta_str],
                )
                .map_err(|e| format!("更新匯入門連結失敗：{e}"))?;
            }
            tx.commit().map_err(|e| format!("提交門連結更新失敗：{e}"))?;
        }
        let _ = conn.execute("DELETE FROM world_snapshots WHERE label = '__initial__'", []);
        drop(conn);
        let _ = create_snapshot(&dest_path, &sum.file_name, "__initial__");
    }

    let entry_old_id = manifest_opt
        .as_ref()
        .and_then(|m| m.get("entry_world_id"))
        .and_then(|v| v.as_str());

    let entry_summary = entry_old_id
        .and_then(|eid| imported.iter().find(|(old_id, _)| old_id == eid).map(|(_, s)| s.clone()))
        .unwrap_or_else(|| imported[0].1.clone());

    let all_summaries = imported.into_iter().map(|(_, s)| s).collect();
    Ok((entry_summary, all_summaries))
}

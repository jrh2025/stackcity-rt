//! Sprite-Stacking 造物工坊、貼圖讀取、四視圖雕刻與 `.zip` / `blueprint.json` / `slices.png` 匯入 API。

use super::{ApiError, AppState};
use crate::db;
use crate::voxel::{
    ai::{check_env_api_keys, generate_ai_blueprint_json, list_available_models},
    bake_from_strip_bytes,
    blueprint::bake_blueprint_json,
    carver::carve_four_views,
    generators::{generate_from_prompt, list_presets as voxel_presets},
};
use axum::{
    extract::{FromRequest, Multipart, Path, Request, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Cursor, Read};
use zip::ZipArchive;

pub async fn list_presets() -> Json<Value> {
    Json(json!({ "presets": voxel_presets() }))
}

pub async fn list_library(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let items = db::list_library_assets(&st.data_dir).map_err(ApiError::internal)?;
    Ok(Json(json!({ "assets": items })))
}

pub async fn ai_status() -> Json<Value> {
    Json(json!(check_env_api_keys()))
}

#[derive(Deserialize)]
pub struct ListAiModelsReq {
    #[serde(default = "default_provider")]
    pub provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

/// 即時向 AI 供應商查詢此金鑰目前實際可用於文字／藍圖生成的模型名稱列表
pub async fn list_ai_models(
    Json(req): Json<ListAiModelsReq>,
) -> Result<Json<Value>, ApiError> {
    let models = list_available_models(
        &req.provider,
        req.api_key.as_deref(),
        req.base_url.as_deref(),
    )
    .await
    .map_err(ApiError::bad_req)?;
    Ok(Json(json!({ "models": models })))
}

pub async fn get_asset_texture(
    State(st): State<AppState>,
    Path((file, id, kind_raw)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let kind = kind_raw.trim_end_matches(".png");
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let Some((bytes, sha)) =
        db::read_asset_texture(&path, &id, kind).map_err(ApiError::bad_req)?
    else {
        return Err(ApiError::not_found("找不到指定的資產貼圖。"));
    };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/png".to_string()),
            (header::ETAG, format!("\"{sha}\"")),
            (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
        ],
        bytes,
    ))
}

#[derive(Deserialize)]
pub struct GenerateReq {
    pub prompt: String,
    #[serde(default = "default_slices")]
    pub slices: usize,
    #[serde(default = "default_seed")]
    pub seed: u64,
}
fn default_slices() -> usize {
    44
}
fn default_seed() -> u64 {
    42
}

pub async fn generate_asset(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<GenerateReq>,
) -> Result<Json<Value>, ApiError> {
    let prompt = req.prompt.trim();
    if prompt.is_empty() {
        return Err(ApiError::bad_req("請輸入想製作的物件名稱或描述。"));
    }
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let baked =
        generate_from_prompt(prompt, req.slices.clamp(16, 64), req.seed).map_err(ApiError::bad_req)?;
    let meta = db::insert_asset_into_world(&conn, &st.data_dir, &baked, "local_prompt", None)
        .map_err(ApiError::internal)?;
    Ok(Json(json!({
        "asset": meta,
        "diagnostics": baked.diagnostics
    })))
}

#[derive(Deserialize)]
pub struct BlueprintReq {
    pub blueprint: Value,
    pub name: Option<String>,
    #[serde(default = "default_cat")]
    pub category: String,
    #[serde(default = "default_scale")]
    pub default_scale: f32,
    #[serde(default = "default_anim_preset")]
    pub anim_preset: String,
    pub slices: Option<usize>,
    pub update_asset_id: Option<String>,
}
fn default_cat() -> String {
    "prop".to_string()
}
fn default_scale() -> f32 {
    1.0
}
fn default_anim_preset() -> String {
    "static".to_string()
}

fn prepare_blueprint_with_name(mut bp: Value, explicit_name: Option<&str>) -> Value {
    if let Some(n) = explicit_name.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(obj) = bp.as_object_mut() {
            obj.insert("title".to_string(), Value::String(n.to_string()));
        }
    }
    bp
}

/// 純記憶體即時烘焙 3D 積木藍圖預覽（絕對不寫入世界或個人資產庫）
pub async fn preview_blueprint_asset(
    Json(req): Json<BlueprintReq>,
) -> Result<Json<Value>, ApiError> {
    let bp = prepare_blueprint_with_name(req.blueprint, req.name.as_deref());
    let baked = bake_blueprint_json(
        &bp,
        &req.category,
        req.default_scale.clamp(0.2, 5.0),
        &req.anim_preset,
        req.slices,
        true,
        true,
    )
    .map_err(ApiError::bad_req)?;

    Ok(Json(json!({
        "name": baked.title,
        "slice_count": baked.slices,
        "slice_w": baked.slice_w,
        "slice_h": baked.slice_h,
        "slices_data_url": format!("data:image/png;base64,{}", STANDARD.encode(&baked.slices_png)),
        "emissive_data_url": format!("data:image/png;base64,{}", STANDARD.encode(&baked.emissive_png)),
        "diagnostics": baked.diagnostics
    })))
}

pub async fn bake_blueprint_asset(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<BlueprintReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let bp = prepare_blueprint_with_name(req.blueprint, req.name.as_deref());
    let baked = bake_blueprint_json(
        &bp,
        &req.category,
        req.default_scale.clamp(0.2, 5.0),
        &req.anim_preset,
        req.slices,
        true,
        true,
    )
    .map_err(ApiError::bad_req)?;
    let meta = db::insert_asset_into_world(
        &conn,
        &st.data_dir,
        &baked,
        "blueprint",
        req.update_asset_id.as_deref(),
    )
    .map_err(ApiError::internal)?;
    Ok(Json(json!({
        "asset": meta,
        "diagnostics": baked.diagnostics
    })))
}

pub async fn carve_asset(
    State(st): State<AppState>,
    Path(file): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let mut img_bytes: Option<Vec<u8>> = None;
    let mut title = "四視圖雕刻物件".to_string();
    let mut category = "prop".to_string();
    let mut slices: Option<usize> = Some(36);
    let mut round = false;
    let mut side_nose_right = false;
    let mut top_nose = "auto".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_req(format!("讀取上傳表單失敗：{e}")))?
    {
        let fname = field.name().unwrap_or("").to_string();
        match fname.as_str() {
            "image" | "file" => {
                img_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::bad_req(format!("讀取圖片失敗：{e}")))?
                        .to_vec(),
                );
            }
            "title" => {
                let t = field.text().await.unwrap_or_default();
                if !t.trim().is_empty() {
                    title = t.trim().to_string();
                }
            }
            "category" => {
                let c = field.text().await.unwrap_or_default();
                if !c.trim().is_empty() {
                    category = c.trim().to_string();
                }
            }
            "slices" => {
                if let Ok(v) = field.text().await.unwrap_or_default().trim().parse::<usize>() {
                    if v > 0 {
                        slices = Some(v);
                    }
                }
            }
            "round" => {
                round = matches!(
                    field.text().await.unwrap_or_default().trim(),
                    "true" | "1" | "on"
                );
            }
            "side_nose" => {
                side_nose_right = field.text().await.unwrap_or_default().trim() == "right";
            }
            "top_nose" => {
                top_nose = field.text().await.unwrap_or_default().trim().to_string();
            }
            _ => {}
        }
    }

    let bytes = img_bytes.ok_or_else(|| ApiError::bad_req("請選擇四視圖 PNG/JPG 圖片。"))?;
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;

    let baked = carve_four_views(
        &bytes,
        title,
        category,
        slices,
        round,
        side_nose_right,
        false,
        &top_nose,
        1.0,
    )
    .map_err(ApiError::bad_req)?;
    let meta = db::insert_asset_into_world(&conn, &st.data_dir, &baked, "carved_4view", None)
        .map_err(ApiError::internal)?;
    Ok(Json(json!({
        "asset": meta,
        "diagnostics": baked.diagnostics
    })))
}

#[derive(Deserialize)]
pub struct AiGenerateReq {
    pub prompt: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    #[serde(default = "default_slices")]
    pub slices: usize,
    #[serde(default = "default_cat")]
    pub category: String,
    pub lang: Option<String>,
}
fn default_provider() -> String {
    "gemini".to_string()
}

pub async fn ai_generate_asset(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Json(req): Json<AiGenerateReq>,
) -> Result<Json<Value>, ApiError> {
    if req.prompt.trim().is_empty() {
        return Err(ApiError::bad_req("請輸入 AI 建模提示描述。"));
    }
    let bp = generate_ai_blueprint_json(
        &req.prompt,
        &req.provider,
        req.api_key.as_deref(),
        req.model.as_deref(),
        req.base_url.as_deref(),
        req.slices,
        req.lang.as_deref(),
    )
    .await
    .map_err(ApiError::bad_req)?;

    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let baked = bake_blueprint_json(
        &bp,
        &req.category,
        1.0,
        "static",
        Some(req.slices),
        true,
        true,
    )
    .map_err(ApiError::bad_req)?;
    let meta = db::insert_asset_into_world(&conn, &st.data_dir, &baked, "ai_blueprint", None)
        .map_err(ApiError::internal)?;
    Ok(Json(json!({
        "asset": meta,
        "diagnostics": baked.diagnostics
    })))
}

/// 支援多種匯入模式：
/// 1) JSON `{ "library_asset_id": "..." }`：從個人資產庫複製至當前世界
/// 2) Multipart 上傳 `sprite-stacking` `.zip` 套件 / `blueprint.json` / `slices.png`
pub async fn import_asset(
    State(st): State<AppState>,
    Path(file): Path<String>,
    req: Request,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;

    let ct = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if ct.contains("application/json") {
        let body = axum::body::to_bytes(req.into_body(), 4 * 1024 * 1024)
            .await
            .map_err(|e| ApiError::bad_req(format!("讀取請求內容失敗：{e}")))?;
        let v: Value = serde_json::from_slice(&body)
            .map_err(|e| ApiError::bad_req(format!("JSON 解析失敗：{e}")))?;
        if let Some(lib_id) = v.get("library_asset_id").and_then(|x| x.as_str()) {
            let Some((item, slices, norms, emis)) =
                db::get_library_asset_blobs(&st.data_dir, lib_id).map_err(ApiError::internal)?
            else {
                return Err(ApiError::not_found("個人資產庫找不到該物件。"));
            };
            let baked = bake_from_strip_bytes(
                item.name,
                item.category,
                &slices,
                item.slice_count as usize,
                Some(&norms),
                Some(&emis),
                item.blueprint_json,
                item.default_scale,
                item.anim_preset,
            )
            .map_err(ApiError::bad_req)?;
            let meta =
                db::insert_asset_into_world(&conn, &st.data_dir, &baked, &item.source_type, None)
                    .map_err(ApiError::internal)?;
            return Ok(Json(json!({ "asset": meta })));
        }
        return Err(ApiError::bad_req("請提供 library_asset_id。"));
    }

    let mut mp = Multipart::from_request(req, &st)
        .await
        .map_err(|e| ApiError::bad_req(format!("無效的上傳表單格式：{e}")))?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut orig_name = "imported".to_string();
    let mut title_override: Option<String> = None;
    let mut category = "prop".to_string();
    let mut slices_override: usize = 36;

    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| ApiError::bad_req(format!("解析上傳表單失敗：{e}")))?
    {
        let fname = field.name().unwrap_or("").to_string();
        match fname.as_str() {
            "file" => {
                if let Some(n) = field.file_name() {
                    orig_name = n.to_string();
                }
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::bad_req(format!("讀取檔案失敗：{e}")))?
                        .to_vec(),
                );
            }
            "title" => {
                let t = field.text().await.unwrap_or_default();
                if !t.trim().is_empty() {
                    title_override = Some(t.trim().to_string());
                }
            }
            "category" => {
                let c = field.text().await.unwrap_or_default();
                if !c.trim().is_empty() {
                    category = c.trim().to_string();
                }
            }
            "slices" => {
                if let Ok(v) = field.text().await.unwrap_or_default().trim().parse::<usize>() {
                    if v > 0 {
                        slices_override = v;
                    }
                }
            }
            _ => {}
        }
    }

    let raw = file_bytes.ok_or_else(|| ApiError::bad_req("請選擇要匯入的 .zip、blueprint.json 或 slices.png 檔案。"))?;
    let lower_name = orig_name.to_ascii_lowercase();

    let baked = if lower_name.ends_with(".zip") {
        import_from_zip_bytes(&raw, title_override, &category)?
    } else if lower_name.ends_with(".json") {
        let bp: Value = serde_json::from_slice(&raw)
            .map_err(|e| ApiError::bad_req(format!("解析藍圖 JSON 失敗：{e}")))?;
        bake_blueprint_json(&bp, &category, 1.0, "static", None, true, true)
            .map_err(ApiError::bad_req)?
    } else {
        let title = title_override.unwrap_or_else(|| {
            orig_name
                .trim_end_matches(".png")
                .trim_end_matches("_slices")
                .to_string()
        });
        bake_from_strip_bytes(
            title,
            category,
            &raw,
            slices_override,
            None,
            None,
            None,
            1.0,
            "static",
        )
        .map_err(ApiError::bad_req)?
    };

    let meta = db::insert_asset_into_world(&conn, &st.data_dir, &baked, "imported", None)
        .map_err(ApiError::internal)?;
    Ok(Json(json!({
        "asset": meta,
        "diagnostics": baked.diagnostics
    })))
}

fn import_from_zip_bytes(
    raw: &[u8],
    title_override: Option<String>,
    category: &str,
) -> Result<crate::voxel::BakedStack, ApiError> {
    let reader = Cursor::new(raw);
    let mut archive =
        ZipArchive::new(reader).map_err(|e| ApiError::bad_req(format!("無法開啟 ZIP 壓縮檔：{e}")))?;

    let mut slices_png: Option<Vec<u8>> = None;
    let mut normals_png: Option<Vec<u8>> = None;
    let mut emissive_png: Option<Vec<u8>> = None;
    let mut metadata_json: Option<Value> = None;
    let mut blueprint_str: Option<String> = None;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ApiError::bad_req(format!("讀取 ZIP 項目失敗：{e}")))?;
        let name = entry.name().to_ascii_lowercase();
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| ApiError::bad_req(format!("解壓縮項目失敗：{e}")))?;
        if name.ends_with("slices.png") {
            slices_png = Some(buf);
        } else if name.ends_with("normals.png") {
            normals_png = Some(buf);
        } else if name.ends_with("emissive.png") {
            emissive_png = Some(buf);
        } else if name.ends_with("metadata.json") {
            metadata_json = serde_json::from_slice(&buf).ok();
        } else if name.ends_with("blueprint.json") {
            blueprint_str = String::from_utf8(buf).ok();
        }
    }

    if let Some(spng) = slices_png {
        let slices = metadata_json
            .as_ref()
            .and_then(|m| m.get("slices").and_then(|v| v.as_u64()))
            .unwrap_or(48) as usize;
        let title = title_override
            .or_else(|| {
                metadata_json
                    .as_ref()
                    .and_then(|m| m.get("title").and_then(|v| v.as_str()).map(ToString::to_string))
            })
            .unwrap_or_else(|| "匯入精靈堆疊物件".to_string());
        return bake_from_strip_bytes(
            title,
            category,
            &spng,
            slices,
            normals_png.as_deref(),
            emissive_png.as_deref(),
            blueprint_str,
            1.0,
            "static",
        )
        .map_err(ApiError::bad_req);
    }

    if let Some(bp_raw) = blueprint_str {
        let bp: Value = serde_json::from_str(&bp_raw)
            .map_err(|e| ApiError::bad_req(format!("解析 ZIP 內 blueprint.json 失敗：{e}")))?;
        return bake_blueprint_json(&bp, category, 1.0, "static", None, true, true)
            .map_err(ApiError::bad_req);
    }

    Err(ApiError::bad_req(
        "ZIP 套件內未找到 slices.png 或 blueprint.json。",
    ))
}

#[derive(Deserialize)]
pub struct UpdateAssetReq {
    pub name: Option<String>,
    pub category: Option<String>,
    pub anim_preset: Option<String>,
    pub default_scale: Option<f32>,
    pub blueprint: Option<Value>,
    pub slices: Option<usize>,
}

/// 更新個人跨世界資產庫（`_library.db`）的名稱、分類或重新烘焙 3D 積木藍圖
pub async fn update_library_asset(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAssetReq>,
) -> Result<Json<Value>, ApiError> {
    let cat_for_bake = req
        .category
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("prop");
    let scale_for_bake = req.default_scale.unwrap_or(1.0).clamp(0.2, 5.0) as f32;
    let anim_for_bake = req
        .anim_preset
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("static");

    let mut baked_holder = None;
    if let Some(mut bp) = req.blueprint.clone() {
        if let Some(n) = req.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(obj) = bp.as_object_mut() {
                obj.insert("title".to_string(), Value::String(n.to_string()));
            }
        }
        let baked = bake_blueprint_json(
            &bp,
            cat_for_bake,
            scale_for_bake,
            anim_for_bake,
            req.slices,
            true,
            true,
        )
        .map_err(ApiError::bad_req)?;
        baked_holder = Some(baked);
    }

    let updated = db::update_library_asset(
        &st.data_dir,
        &id,
        req.name.as_deref(),
        req.category.as_deref(),
        req.anim_preset.as_deref(),
        req.default_scale,
        baked_holder.as_ref(),
    )
    .map_err(ApiError::bad_req)?;

    Ok(Json(json!({
        "asset": updated,
        "diagnostics": baked_holder.as_ref().map(|b| &b.diagnostics)
    })))
}

/// 從個人跨世界資產庫（`_library.db`）刪除指定物件
pub async fn delete_library_asset(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let deleted = db::delete_library_asset(&st.data_dir, &id).map_err(ApiError::internal)?;
    if !deleted {
        return Err(ApiError::not_found("個人資產庫找不到該物件。"));
    }
    Ok(Json(json!({ "ok": true, "id": id })))
}

/// 更新目前世界資料庫內的指定資產（名稱、分類、或重烘 3D 積木藍圖）
pub async fn update_world_asset(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
    Json(req): Json<UpdateAssetReq>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;

    let cat_for_bake = req
        .category
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("prop");
    let scale_for_bake = req.default_scale.unwrap_or(1.0).clamp(0.2, 5.0) as f32;
    let anim_for_bake = req
        .anim_preset
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("static");

    let mut baked_holder = None;
    if let Some(mut bp) = req.blueprint.clone() {
        if let Some(n) = req.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(obj) = bp.as_object_mut() {
                obj.insert("title".to_string(), Value::String(n.to_string()));
            }
        }
        let baked = bake_blueprint_json(
            &bp,
            cat_for_bake,
            scale_for_bake,
            anim_for_bake,
            req.slices,
            true,
            true,
        )
        .map_err(ApiError::bad_req)?;
        baked_holder = Some(baked);
    }

    let updated = db::update_world_asset(
        &conn,
        &st.data_dir,
        &id,
        req.name.as_deref(),
        req.category.as_deref(),
        req.anim_preset.as_deref(),
        req.default_scale,
        baked_holder.as_ref(),
    )
    .map_err(ApiError::bad_req)?;

    Ok(Json(json!({
        "asset": updated,
        "diagnostics": baked_holder.as_ref().map(|b| &b.diagnostics)
    })))
}

/// 刪除目前世界內的指定資產，並回傳同步移除之場景實體 ID 清單
pub async fn delete_world_asset(
    State(st): State<AppState>,
    Path((file, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let conn = db::open_world_conn(&path).map_err(ApiError::bad_req)?;
    let removed_entity_ids = db::delete_world_asset(&conn, &id).map_err(ApiError::bad_req)?;
    Ok(Json(json!({
        "ok": true,
        "id": id,
        "removed_entity_ids": removed_entity_ids
    })))
}

//! Axum REST API 路由與靜態前端資源內嵌模組。

pub mod assets;
pub mod share;
pub mod worlds;

use axum::{
    extract::DefaultBodyLimit,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use rust_embed::RustEmbed;
use serde_json::json;
use std::path::PathBuf;
use tower_http::cors::CorsLayer;

/// 將 `web/` 資料夾內所有前端資源直接編譯內嵌至執行檔
#[derive(RustEmbed)]
#[folder = "web/"]
struct WebAssets;

/// 讀取編譯時內嵌的靜態網頁文字檔案（供獨立單純遊戲 HTML 匯出器打包使用）
pub(crate) fn get_embedded_web_text(path: &str) -> Option<String> {
    WebAssets::get(path).and_then(|f| String::from_utf8(f.data.into_owned()).ok())
}

/// 伺服器共用狀態
#[derive(Clone)]
pub struct AppState {
    pub data_dir: PathBuf,
}

/// 統一繁體中文 JSON 錯誤回應
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);

impl ApiError {
    pub fn bad_req(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self(StatusCode::NOT_FOUND, msg.into())
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

/// 建置完整的 Axum 路由器
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // 世界管理與狀態 CRUD
        .route("/api/worlds", get(worlds::list_worlds).post(worlds::create_world))
        .route("/api/worlds/import", post(share::import_world))
        .route("/api/worlds/:file", delete(worlds::delete_world))
        .route("/api/worlds/:file/state", get(worlds::get_world_state))
        .route("/api/worlds/:file/meta", patch(worlds::update_world_meta))
        .route("/api/worlds/:file/resize", post(worlds::resize_world_handler))
        .route(
            "/api/worlds/:file/bgm",
            get(worlds::get_world_bgm)
                .post(worlds::upload_world_bgm)
                .delete(worlds::delete_world_bgm_handler),
        )
        .route("/api/worlds/:file/export", get(share::export_world))
        .route("/api/worlds/:file/export-html", get(share::export_world_html))
        .route("/api/worlds/:file/terrain/batch", post(worlds::batch_update_terrain))
        .route("/api/worlds/:file/entities", post(worlds::create_entities))
        .route(
            "/api/worlds/:file/entities/:id",
            patch(worlds::update_entity).delete(worlds::delete_entity),
        )
        .route("/api/worlds/:file/paths", post(worlds::create_path))
        .route(
            "/api/worlds/:file/paths/:id",
            put(worlds::update_path).delete(worlds::delete_path),
        )
        .route("/api/worlds/:file/wander-zones", post(worlds::create_wander_zone))
        .route(
            "/api/worlds/:file/wander-zones/:id",
            put(worlds::update_wander_zone).delete(worlds::delete_wander_zone),
        )
        .route("/api/worlds/:file/events", post(worlds::create_event))
        .route(
            "/api/worlds/:file/events/:id",
            put(worlds::update_event).delete(worlds::delete_event),
        )
        .route("/api/worlds/:file/snapshots", post(worlds::create_snapshot))
        .route(
            "/api/worlds/:file/snapshots/:id/restore",
            post(worlds::restore_snapshot),
        )
        .route("/api/worlds/:file/reset", post(worlds::reset_world_handler))
        // Sprite-Stacking 造物與資產庫 API
        .route("/api/assets/presets", get(assets::list_presets))
        .route("/api/assets/library", get(assets::list_library))
        .route("/api/assets/preview-blueprint", post(assets::preview_blueprint_asset))
        .route(
            "/api/assets/library/:id",
            patch(assets::update_library_asset).delete(assets::delete_library_asset),
        )
        .route("/api/ai/status", get(assets::ai_status))
        .route("/api/ai/models", post(assets::list_ai_models))
        .route("/api/worlds/:file/assets/:id/:kind", get(assets::get_asset_texture))
        .route(
            "/api/worlds/:file/assets/:id",
            patch(assets::update_world_asset).delete(assets::delete_world_asset),
        )
        .route("/api/worlds/:file/assets/generate", post(assets::generate_asset))
        .route("/api/worlds/:file/assets/blueprint", post(assets::bake_blueprint_asset))
        .route("/api/worlds/:file/assets/carve", post(assets::carve_asset))
        .route("/api/worlds/:file/assets/ai", post(assets::ai_generate_asset))
        .route("/api/worlds/:file/assets/import", post(assets::import_asset))
        // 前端靜態資源路由
        .fallback(serve_embedded_web)
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn serve_embedded_web(uri: Uri) -> impl IntoResponse {
    let raw_path = uri.path().trim_start_matches('/');
    let path = if raw_path.is_empty() {
        "index.html"
    } else {
        raw_path
    };

    if let Some(content) = WebAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime.as_ref().to_string()),
                (header::CACHE_CONTROL, "no-cache".to_string()),
            ],
            content.data.into_owned(),
        )
            .into_response();
    }

    if let Some(index) = WebAssets::get("index.html") {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8".to_string())],
            index.data.into_owned(),
        )
            .into_response();
    }

    (StatusCode::NOT_FOUND, "找不到網頁資源").into_response()
}

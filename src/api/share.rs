//! 好友世界分享匯出與匯入 API：
//! 1) 完整分享（`.stackworld.db`）：單檔自包含 SQLite，好友可參觀亦可切換編輯模式共同擴建。
//! 2) 單純遊戲分享（`.html`）：單檔免安裝純玩家網頁版，內嵌 WebGL2 渲染器、所有基地64切片貼圖與配樂，
//!    鎖定僅供以玩家身分闖關體驗，完全無法修改地圖、事件或模型資產。

use super::{get_embedded_web_text, ApiError, AppState};
use crate::db;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub async fn export_world(
    State(st): State<AppState>,
    Path(file): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let bytes = db::export_world_db_bytes(&path).map_err(ApiError::bad_req)?;
    let fname = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("world.stackworld.db")
        .to_string();
    let encoded_name = url_encode_filename(&fname);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/x-sqlite3".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{fname}\"; filename*=UTF-8''{encoded_name}"),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        bytes,
    ))
}

/// 移除單行 ES Module `import` 與行首 `export ` 關鍵字，使其可安全包裝進獨立網頁的 IIFE 中執行
fn strip_esm_lines(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// 防止 JSON 內文（如對話或標題）出現 `</script>` 或 `<!--` 而截斷內嵌 `<script>` 區塊
fn escape_json_for_inline_script(raw_json: &str) -> String {
    raw_json
        .replace("</", "<\\/")
        .replace("<!--", "<\\!--")
}

#[derive(Deserialize, Default)]
pub struct ExportHtmlQuery {
    pub preview: Option<String>,
}

/// 匯出「單純遊戲分享」之獨立單檔 `.html` 網頁（自動遞迴打包所有透過「門」相連的關聯世界＋貼圖 SHA256 去重）
pub async fn export_world_html(
    State(st): State<AppState>,
    Path(file): Path<String>,
    Query(query): Query<ExportHtmlQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let path = db::world_path(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let safe_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&file)
        .to_string();

    let linked_worlds = db::collect_linked_worlds(&st.data_dir, &safe_name).map_err(ApiError::internal)?;
    let full_state = db::load_full_world_state(&path, &safe_name).map_err(ApiError::internal)?;
    let bgm_data_url = db::read_world_bgm_data_url(&path).map_err(ApiError::internal)?;

    let mut texture_pool: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    let mut worlds_map: std::collections::HashMap<String, Value> = std::collections::HashMap::new();

    for lw in &linked_worlds {
        db::load_asset_textures_by_sha256(&lw.path, &mut texture_pool)
            .map_err(ApiError::internal)?;
        let st_data = db::load_full_world_state(&lw.path, &lw.file_name)
            .map_err(ApiError::internal)?;
        let lw_bgm = db::read_world_bgm_data_url(&lw.path).map_err(ApiError::internal)?;
        worlds_map.insert(
            lw.world_id.clone(),
            json!({
                "state": st_data,
                "bgmDataUrl": lw_bgm,
            }),
        );
    }

    let style_css = get_embedded_web_text("css/style.css")
        .ok_or_else(|| ApiError::internal("找不到內嵌樣式表 css/style.css。"))?;
    let i18n_js = get_embedded_web_text("js/i18n.js")
        .ok_or_else(|| ApiError::internal("找不到內嵌多語系模組 js/i18n.js。"))?;
    let audio_js = get_embedded_web_text("js/audio.js")
        .ok_or_else(|| ApiError::internal("找不到內嵌模組 js/audio.js。"))?;
    let renderer_js = get_embedded_web_text("js/renderer.js")
        .ok_or_else(|| ApiError::internal("找不到內嵌模組 js/renderer.js。"))?;
    let simulation_js = get_embedded_web_text("js/simulation.js")
        .ok_or_else(|| ApiError::internal("找不到內嵌模組 js/simulation.js。"))?;
    let quest_js = get_embedded_web_text("js/quest.js")
        .ok_or_else(|| ApiError::internal("找不到內嵌模組 js/quest.js。"))?;
    let bootstrap_js = get_embedded_web_text("js/standalone_bootstrap.js")
        .ok_or_else(|| ApiError::internal("找不到內嵌啟動器 js/standalone_bootstrap.js。"))?;

    let payload_json = serde_json::to_string(&json!({
        "entryWorldId": full_state.meta.world_id,
        "state": full_state,
        "bgmDataUrl": bgm_data_url,
        "texturePool": texture_pool,
        "worlds": worlds_map,
    }))
    .map_err(|e| ApiError::internal(format!("序列化世界遊戲封包失敗：{e}")))?;
    let safe_payload = escape_json_for_inline_script(&payload_json);

    let world_title_escaped = full_state
        .meta
        .name
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");

    let bundled_i18n = strip_esm_lines(&i18n_js);
    let bundled_audio = strip_esm_lines(&audio_js);
    let bundled_renderer = strip_esm_lines(&renderer_js);
    let bundled_simulation = strip_esm_lines(&simulation_js);
    let bundled_quest = strip_esm_lines(&quest_js);

    let html = format!(
        r#"<!doctype html>
<html lang="zh-Hant">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{title} — StackCity RT 純遊戲版</title>
  <style>
{style_css}
/* 單純遊戲獨立網頁版專屬樣式：無側邊工具欄、畫布全版沉浸遊玩 */
.standalone-workspace {{
  flex: 1;
  min-height: 0;
  display: flex;
}}
.standalone-badge {{
  background: rgba(16, 185, 129, 0.16);
  border: 1px solid rgba(52, 211, 153, 0.45);
  color: #6ee7b7;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}}
.standalone-title-pill {{
  font-size: 13px;
  font-weight: 700;
  color: #f8fafc;
  white-space: nowrap;
}}
.standalone-workspace .quest-hud {{
  top: 98px;
  max-height: calc(100% - 152px);
}}
  </style>
</head>
<body>
  <div id="app" class="app-shell">
    <header class="topbar">
      <div class="topbar-group">
        <div class="brand">
          <span class="brand-logo">◆</span>
          <div>
            <strong>StackCity RT</strong>
            <small>像素疊界・遊玩版</small>
          </div>
        </div>
        <span id="standaloneWorldTitle" class="standalone-title-pill">{title}</span>
        <span class="standalone-badge">🎮 純玩家模式（已鎖定編輯）</span>
      </div>

      <div class="topbar-group">
        <button id="btnPlayPause" class="btn btn-icon" title="暫停／繼續遊戲">⏸️</button>
        <span id="timeLabel" class="mono-badge">10:30</span>
        <button id="btnPlayerAvatar" class="btn btn-gold" title="更換玩家化身造型">🧑 換造型</button>
        <button id="btnCityWalk" class="btn btn-citywalk" title="進入第三人稱街道漫步">🚶 街道漫步</button>
        <button id="btnMute" class="btn btn-icon" title="開啟／靜音全部聲音">🔊</button>
        <button id="btnResetProgress" class="btn btn-sub" title="清除本機進度重玩本關">🔄 重玩本關</button>
        <select id="langSelect" class="ctrl-select ctrl-compact" title="切換介面語言 / Language">
          <option value="zh-Hant">🌐 繁體中文</option>
          <option value="en">🌐 English</option>
          <option value="ja">🌐 日本語</option>
        </select>
        <button id="btnFullscreen" class="btn btn-icon" title="切換全螢幕沉浸體驗">⛶</button>
      </div>
    </header>

    <main class="standalone-workspace">
      <section class="viewport-wrap" style="flex:1;">
        <canvas id="glCanvas"></canvas>
        <canvas id="overlayCanvas"></canvas>

        <div class="hud-top-left">
          <div id="worldBadge" class="hud-pill">載入中...</div>
          <button id="btnResetCam" class="hud-pill hud-btn" title="重設視角 (R)">🧭 重設視角</button>
          <button id="btnRotateLeft" class="hud-pill hud-btn" title="向左旋轉 45° (Q)">↺</button>
          <button id="btnRotateRight" class="hud-pill hud-btn" title="向右旋轉 45° (E)">↻</button>
        </div>

        <div id="cityWalkBanner" class="citywalk-banner hidden">
          <span>🚶 <strong>CityRT 街道漫步</strong>｜WASD 移動（A左/D右）、按住拖曳轉向、點擊或按 F 與身旁物件互動、按 G 搭乘交通工具、空白鍵跳躍</span>
          <button id="btnWalkChangeAvatar" class="btn btn-gold" style="padding:3px 9px;">🧑 換造型</button>
          <button id="btnExitWalk" class="btn btn-sub">返回上帝視角</button>
        </div>

        <div id="mountBanner" class="mount-banner hidden">
          <span id="mountBannerText">🚗 正在駕駛載具</span>
          <button id="btnMountSwitchMode" class="btn btn-sub hidden" style="padding:3px 9px;">🔄 切換駕駛／巡航 (Tab)</button>
          <button id="btnMountDismount" class="btn btn-gold" style="padding:3px 9px;">🚪 下車 (G)</button>
        </div>

        <div id="hudHint" class="hud-bottom-hint">
          WASD／方向鍵：移動・G：上下車／船／馬・空白鍵：跳躍／鳴笛・F 或點擊物件：任務互動・右鍵旋轉・中鍵／Space+左鍵：平移畫面
        </div>

        <div id="questHud" class="quest-hud hidden"></div>
        <div id="portalTransition" class="portal-transition">
          <div class="portal-transition-icon">🚪</div>
          <div id="portalTransitionText">正在穿越門扉前往新世界...</div>
        </div>
        <div id="toastStack" class="toast-stack"></div>
      </section>
    </main>
  </div>

  <!-- 玩家換造型視窗 -->
  <div id="standaloneAvatarModal" class="modal-backdrop hidden">
    <div class="modal-card modal-sm">
      <div class="modal-header">
        <h3>🧑 選擇我的玩家角色造型</h3>
        <button class="modal-close" data-close-modal="standaloneAvatarModal">✕</button>
      </div>
      <div class="modal-content">
        <div id="standaloneAvatarGrid" class="player-asset-grid"></div>
      </div>
    </div>
  </div>

  <!-- NPC／物件任務互動與考驗對話 Modal -->
  <div id="encounterModal" class="modal-backdrop hidden">
    <div class="modal-card modal-sm">
      <div class="modal-header">
        <div class="row-gap">
          <span id="encIcon" style="font-size:22px;">💬</span>
          <div>
            <h3 id="encTitle">事件互動</h3>
            <p id="encSubtitle">來自場景物件</p>
          </div>
        </div>
        <button class="modal-close" data-close-modal="encounterModal">✕</button>
      </div>
      <div id="encBody" class="modal-content"></div>
    </div>
  </div>

  <!-- 過關慶賀視窗 Modal -->
  <div id="victoryModal" class="modal-backdrop hidden">
    <div class="modal-card modal-sm victory-card">
      <div class="victory-banner">
        <div class="victory-trophy">🏆</div>
        <h2 id="victoryTitle">🎉 恭喜完成關卡目標！</h2>
        <p id="victoryMessage">太厲害了！你已順利達成這個世界的所有挑戰目標！</p>
      </div>
      <div class="modal-content">
        <div id="victoryStats" class="vic-stats-grid"></div>
        <div class="row-gap" style="margin-top:6px;">
          <button id="btnVicReplay" class="btn btn-sub" style="flex:1;">🔄 清除進度再玩一次</button>
          <button id="btnVicContinue" class="btn btn-gold" style="flex:1.2;">🎉 繼續探索世界</button>
        </div>
      </div>
    </div>
  </div>

  <script>
window.__STACKCITY_STANDALONE__ = {safe_payload};

function escapeHtml(s) {{
  return String(s ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}}

const {{ SUPPORTED_LANGS, detectInitialLang, getLang, setLang, onLangChange, tr, applyI18nToTree, initI18n }} = (() => {{
{bundled_i18n}
return {{ SUPPORTED_LANGS, detectInitialLang, getLang, setLang, onLangChange, tr, applyI18nToTree, initI18n }};
}})();

const {{ BGM_STYLES, soundEngine }} = (() => {{
{bundled_audio}
return {{ BGM_STYLES, soundEngine }};
}})();

const {{ StackCityRenderer, renderStackThumbnail }} = (() => {{
{bundled_renderer}
return {{ StackCityRenderer, renderStackThumbnail }};
}})();

const {{ WorldSimulation, parseEntityVehicle, buildEntityMetaWithVehicle, parseEntityPortal, buildEntityMetaWithPortal }} = (() => {{
{bundled_simulation}
return {{ WorldSimulation, parseEntityVehicle, buildEntityMetaWithVehicle, parseEntityPortal, buildEntityMetaWithPortal }};
}})();

const {{ INTERACTION_META, GOAL_TYPE_LABELS, parseEntityInteraction, buildEntityMetaWithInteraction, QuestManager }} = (() => {{
{bundled_quest}
return {{ INTERACTION_META, GOAL_TYPE_LABELS, parseEntityInteraction, buildEntityMetaWithInteraction, QuestManager }};
}})();

{bootstrap_js}

bootStandalonePlayer();
  </script>
</body>
</html>
"#,
        title = world_title_escaped,
        style_css = style_css,
        safe_payload = safe_payload,
        bundled_i18n = bundled_i18n,
        bundled_audio = bundled_audio,
        bundled_renderer = bundled_renderer,
        bundled_simulation = bundled_simulation,
        bundled_quest = bundled_quest,
        bootstrap_js = bootstrap_js,
    );

    let clean_base = full_state
        .meta
        .name
        .chars()
        .map(|c| {
            if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    let dl_name = format!(
        "{}_純遊戲版.html",
        if clean_base.trim().is_empty() {
            "stackcity"
        } else {
            clean_base.trim()
        }
    );
    let encoded_name = url_encode_filename(&dl_name);
    let is_preview = matches!(query.preview.as_deref(), Some("1" | "true" | "yes"));
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if !is_preview {
        let disp = format!(
            "attachment; filename=\"stackcity-play.html\"; filename*=UTF-8''{encoded_name}"
        );
        if let Ok(hv) = HeaderValue::from_str(&disp) {
            headers.insert(header::CONTENT_DISPOSITION, hv);
        }
    }

    Ok((StatusCode::OK, headers, html.into_bytes()))
}

fn url_encode_filename(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub async fn import_world(
    State(st): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let mut db_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_req(format!("讀取匯入表單失敗：{e}")))?
    {
        if field.name() == Some("file") {
            file_name = field.file_name().map(ToString::to_string);
            db_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad_req(format!("讀取世界檔失敗：{e}")))?
                    .to_vec(),
            );
        }
    }

    let bytes = db_bytes.ok_or_else(|| ApiError::bad_req("請選擇好友分享的 .stackworld.db 或 .stackpack.zip 檔案。"))?;
    let is_zip = bytes.starts_with(b"PK\x03\x04")
        || file_name
            .as_deref()
            .map(|s| s.to_ascii_lowercase().ends_with(".zip"))
            .unwrap_or(false);
    if is_zip {
        let (entry_world, imported_worlds) =
            db::import_world_pack_zip_bytes(&bytes, &st.data_dir).map_err(ApiError::bad_req)?;
        let count = imported_worlds.len();
        return Ok(Json(json!({
            "world": entry_world,
            "imported_worlds": imported_worlds,
            "count": count,
            "is_pack": true
        })));
    }
    let summary = db::import_world_db_bytes(&bytes, &st.data_dir, file_name.as_deref())
        .map_err(ApiError::bad_req)?;
    Ok(Json(json!({ "world": summary, "is_pack": false })))
}

/// 匯出 `.stackpack.zip` 多世界編輯包（含入口世界與所有透過「門」相連之室內外世界）
pub async fn export_world_pack(
    State(st): State<AppState>,
    Path(file): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let (bytes, entry_name, _count) =
        db::export_world_pack_zip_bytes(&st.data_dir, &file).map_err(ApiError::bad_req)?;
    let clean_base = entry_name
        .chars()
        .map(|c| {
            if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    let dl_name = format!(
        "{}_多世界編輯包.stackpack.zip",
        if clean_base.trim().is_empty() {
            "stackcity"
        } else {
            clean_base.trim()
        }
    );
    let encoded_name = url_encode_filename(&dl_name);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"stackcity-pack.zip\"; filename*=UTF-8''{encoded_name}"),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        bytes,
    ))
}

/// 匯入 `.stackpack.zip` 多世界編輯包（自動重配全新世界 UUID 並重寫所有門連結）
pub async fn import_world_pack(
    State(st): State<AppState>,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    import_world(State(st), multipart).await
}

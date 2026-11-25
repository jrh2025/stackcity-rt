//! 可選外部 AI 3D 藍圖生成模組（支援 Google Gemini、Google Gemma 開放模型與 OpenAI 相容端點）。
//!
//! 優先使用介面傳入的金鑰，若留白則自動讀取環境變數 `GEMINI_API_KEY`、`GEMMA_API_KEY` 或 `OPENAI_API_KEY`。
//! 支援使用者自訂任意模型名稱與 OpenAI 相容基地台 URL；所有錯誤訊息均經過金鑰遮罩處理，確保安全不外洩。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

const BLUEPRINT_SYSTEM_PROMPT: &str = r#"你是一位精通 Sprite-Stacking（精靈堆疊）3D 像素體素建模的技術美術專家。
請將使用者的描述轉換成精確、結構豐富、具備立體剪影與夜間自發光點綴的 3D 體素藍圖 JSON。

座標與規範：
- size 建議為 [42, 42, 48]（寬 W, 長 L, 高 H；上限 96×96×80）。
- z=0 為地面，z=H-1 為最高點。
- +y（較大的 y 值）為正面（Front）朝向。
- 所有結構元件寬度或半徑請確保至少 2px（例如 radius >= 1.1），避免旋轉時閃爍。
- 為窗戶、車燈、火把、水晶、霓虹或能量核心加上 "emissive": true 並選用高亮度暖黃/青/粉顏色，讓夜景模式更具魅力。
- 可選 "material" 屬性："stone" | "wood" | "glass" | "metal" | "fur" | "mane" | "foliage" | "fabric" | "crystal" | "flat"。

支援的 primitives 類型：
1. {"type": "box", "x0": int, "y0": int, "z0": int, "x1": int, "y1": int, "z1": int, "color": key, "mirror_x": bool, "emissive": bool}
2. {"type": "clear_box", "x0": int, "y0": int, "z0": int, "x1": int, "y1": int, "z1": int, "mirror_x": bool}
3. {"type": "cylinder", "cx": float, "cy": float, "z0": int, "z1": int, "rx": float, "ry": float, "hollow": bool, "wall": float, "color": key, "mirror_x": bool}
4. {"type": "cone", "cx": float, "cy": float, "z0": int, "z1": int, "rx0": float, "ry0": float, "rx1": float, "ry1": float, "color": key}
5. {"type": "sphere", "cx": float, "cy": float, "cz": float, "rx": float, "ry": float, "rz": float, "color": key, "mirror_x": bool, "emissive": bool}
6. {"type": "pyramid", "cx": float, "cy": float, "z0": int, "z1": int, "w0": float, "l0": float, "w1": float, "l1": float, "stepped": bool, "steps": int, "color": key}
7. {"type": "arch_cut", "cx": float, "cy": float, "z0": int, "z1": int, "half_span": float, "depth": float, "axis": "x"|"y"}
8. {"type": "line", "p0": [x,y,z], "p1": [x,y,z], "radius": float, "color": key, "mirror_x": bool}

只輸出合法 JSON 物件，欄位包含：
{
  "title": "繁體中文物件名稱",
  "size": [W, L, H],
  "palette": { "顏色代號": [R, G, B] },
  "primitives": [ ... ]
}"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiKeyStatus {
    pub gemini: bool,
    pub gemma: bool,
    pub openai: bool,
}

pub fn check_env_api_keys() -> AiKeyStatus {
    let gemini = std::env::var("GEMINI_API_KEY")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let gemma_own = std::env::var("GEMMA_API_KEY")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let openai = std::env::var("OPENAI_API_KEY")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    AiKeyStatus {
        gemini,
        // Google AI Studio 上的 Gemma 模型可直接共用 GEMINI_API_KEY
        gemma: gemma_own || gemini,
        openai,
    }
}

/// 建置 Google Generative Language API（Gemini / Gemma）請求內容。
/// Gemma 模型目前在部分端點不支援 `systemInstruction` 與 `responseMimeType`，故改為合併寫入 user prompt。
pub fn build_google_request_body(provider: &str, model: &str, user_msg: &str) -> Value {
    let is_gemma = provider.trim().eq_ignore_ascii_case("gemma")
        || model.trim().to_ascii_lowercase().starts_with("gemma");
    if is_gemma {
        let combined = format!(
            "{BLUEPRINT_SYSTEM_PROMPT}\n\n---\n{user_msg}\n請務必只回傳一個合法的 JSON 物件，不要加上額外解釋文字。"
        );
        json!({
            "contents": [{ "role": "user", "parts": [{ "text": combined }] }],
            "generationConfig": {
                "temperature": 0.4
            }
        })
    } else {
        json!({
            "systemInstruction": { "parts": [{ "text": BLUEPRINT_SYSTEM_PROMPT }] },
            "contents": [{ "role": "user", "parts": [{ "text": user_msg }] }],
            "generationConfig": {
                "temperature": 0.4,
                "responseMimeType": "application/json"
            }
        })
    }
}

pub fn validate_model_name(raw: &str) -> Result<String, String> {
    let m = raw.trim().trim_start_matches("models/");
    if m.is_empty() {
        return Err("請輸入有效的模型名稱。".to_string());
    }
    if m.chars().any(|c| c.is_whitespace() || matches!(c, '/' | '?' | '#' | '&')) {
        return Err("模型名稱格式無效（不可包含空白或網址符號 / ? # &）。".to_string());
    }
    Ok(m.to_string())
}

fn resolve_api_key(prov: &str, explicit_key: Option<&str>) -> String {
    match explicit_key {
        Some(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => match prov {
            "openai" => std::env::var("OPENAI_API_KEY")
                .unwrap_or_default()
                .trim()
                .to_string(),
            "gemma" => std::env::var("GEMMA_API_KEY")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| std::env::var("GEMINI_API_KEY").ok())
                .unwrap_or_default()
                .trim()
                .to_string(),
            _ => std::env::var("GEMINI_API_KEY")
                .unwrap_or_default()
                .trim()
                .to_string(),
        },
    }
}

fn normalize_provider(provider: &str) -> &'static str {
    let p_lower = provider.trim().to_ascii_lowercase();
    if p_lower == "openai" {
        "openai"
    } else if p_lower == "gemma" {
        "gemma"
    } else {
        "gemini"
    }
}

/// 呼叫官方 ListModels API，查詢目前 API Key 實際支援文字生成（generateContent）的可用模型清單
pub async fn list_available_models(
    provider: &str,
    explicit_key: Option<&str>,
    base_url: Option<&str>,
) -> Result<Vec<String>, String> {
    let prov = normalize_provider(provider);
    let key = resolve_api_key(prov, explicit_key);
    if key.is_empty() {
        return Err("請先輸入或記憶 API Key，再查詢帳號可用模型列表。".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("建立 HTTP 用戶端失敗：{e}"))?;

    if prov == "openai" {
        let root = base_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/');
        let url = format!("{root}/models");
        let resp = client
            .get(&url)
            .bearer_auth(&key)
            .send()
            .await
            .map_err(|e| sanitize_error(format!("查詢 OpenAI 相容模型列表失敗：{e}"), &key))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| sanitize_error(format!("解析模型列表失敗：{e}"), &key))?;
        if !status.is_success() {
            return Err(sanitize_error(
                format!("查詢模型列表失敗 ({}): {}", status, body),
                &key,
            ));
        }
        let mut out: Vec<String> = body["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["id"].as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out.dedup();
        return Ok(out);
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?pageSize=100&key={}",
        key
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| sanitize_error(format!("查詢 Google AI 模型列表失敗：{e}"), &key))?;
    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| sanitize_error(format!("解析 Google AI 模型列表失敗：{e}"), &key))?;
    if !status.is_success() {
        return Err(sanitize_error(
            format!("查詢 Google AI 模型列表失敗 ({}): {}", status, body),
            &key,
        ));
    }

    let mut matched = Vec::new();
    let mut others = Vec::new();
    if let Some(arr) = body["models"].as_array() {
        for m in arr {
            let Some(raw_name) = m["name"].as_str() else {
                continue;
            };
            let short = raw_name.trim_start_matches("models/").to_string();
            let lower = short.to_ascii_lowercase();
            // 僅保留支援 generateContent 的模型
            let supports_gen = m["supportedGenerationMethods"]
                .as_array()
                .map(|methods| {
                    methods
                        .iter()
                        .any(|v| v.as_str() == Some("generateContent"))
                })
                .unwrap_or(false);
            if !supports_gen {
                continue;
            }
            if lower.contains("embedding")
                || lower.contains("imagen")
                || lower.contains("veo")
                || lower.contains("tts")
                || lower.contains("aqa")
            {
                continue;
            }
            if prov == "gemma" {
                if lower.starts_with("gemma") {
                    matched.push(short);
                } else if lower.starts_with("gemini") {
                    others.push(short);
                }
            } else if lower.starts_with("gemini") {
                matched.push(short);
            } else if lower.starts_with("gemma") {
                others.push(short);
            }
        }
    }

    // 優先將最新一代模型排在前面（例如 gemma-4 / gemini-4 / gemini-3 / gemini-2.5）
    let score_rank = |name: &str| -> (i32, String) {
        let l = name.to_ascii_lowercase();
        let tier = if l.contains("gemma-4") || l.contains("gemini-4") {
            0
        } else if l.contains("gemini-3.8") || l.contains("gemini-3.6") || l.contains("gemini-3.1") || l.contains("gemini-3") {
            1
        } else if l.contains("gemini-2.5") || l.contains("gemma-3") {
            2
        } else {
            3
        };
        (tier, l)
    };
    matched.sort_by_key(|s| score_rank(s));
    if matched.is_empty() {
        others.sort_by_key(|s| score_rank(s));
        return Ok(others);
    }
    Ok(matched)
}

fn sanitize_error(mut msg: String, key: &str) -> String {
    if !key.is_empty() {
        msg = msg.replace(key, "***");
    }
    msg
}

fn extract_json_object(raw: &str) -> Result<Value, String> {
    let mut s = raw.trim();
    if let Some(rest) = s.strip_prefix("```json") {
        s = rest.trim();
    } else if let Some(rest) = s.strip_prefix("```") {
        s = rest.trim();
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.trim();
    }
    let start = s
        .find('{')
        .ok_or_else(|| "AI 回傳內容未包含有效 JSON 藍圖物件。".to_string())?;
    let end = s
        .rfind('}')
        .ok_or_else(|| "AI 回傳內容未包含完整 JSON 結尾。".to_string())?;
    if end <= start {
        return Err("AI 回傳 JSON 結構不完整。".to_string());
    }
    serde_json::from_str(&s[start..=end]).map_err(|e| format!("解析 AI 藍圖 JSON 失敗：{e}"))
}

/// 呼叫 Gemini、Gemma 或 OpenAI 相容端點生成 3D 幾何藍圖 JSON
pub async fn generate_ai_blueprint_json(
    prompt: &str,
    provider: &str,
    explicit_key: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    slices: usize,
    lang: Option<&str>,
) -> Result<Value, String> {
    let prov = normalize_provider(provider);
    let key = resolve_api_key(prov, explicit_key);
    if key.is_empty() {
        let hint = if prov == "gemma" {
            "Gemma / Gemini API Key"
        } else if prov == "gemini" {
            "Gemini API Key"
        } else {
            "OpenAI API Key"
        };
        return Err(format!(
            "使用 AI 藍圖模式需設定 {hint}（請於造物工坊填入並記憶，或設定系統環境變數）。"
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| format!("建立 HTTP 用戶端失敗：{e}"))?;

    let title_lang_hint = match lang.unwrap_or("zh-Hant") {
        "en" => "請將 JSON 的 \"title\" 欄位寫成英文名稱。",
        "ja" => "請將 JSON 的 \"title\" 欄位寫成日文名稱。",
        _ => "請將 JSON 的 \"title\" 欄位寫成繁體中文名稱。",
    };
    let user_msg = format!(
        "請為以下物件設計 Sprite-Stacking 3D 體素藍圖 JSON（目標高度切片數約 {} 層；{}）：\n{}",
        slices.clamp(16, 72),
        title_lang_hint,
        prompt.trim()
    );

    if prov == "gemini" || prov == "gemma" {
        // Google API (v1beta) 託管之 Gemma 4 官方代號為 gemma-4-31b-it / gemma-4-26b-a4b-it
        let default_mdl = if prov == "gemma" {
            "gemma-4-31b-it"
        } else {
            "gemini-2.5-flash"
        };
        let mdl = validate_model_name(
            model
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(default_mdl),
        )?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            mdl, key
        );
        let payload = build_google_request_body(prov, &mdl, &user_msg);
        let label = if prov == "gemma" { "Gemma" } else { "Gemini" };
        let resp = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| sanitize_error(format!("無法連線至 {label} API：{e}"), &key))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| sanitize_error(format!("讀取 {label} 回應失敗：{e}"), &key))?;
        if !status.is_success() {
            // 若遇 404 模型不支援或不存在，自動呼叫 ListModels 提供此帳號真正可用的模型代號
            if status.as_u16() == 404 {
                if let Ok(avail) = list_available_models(prov, Some(&key), None).await {
                    if !avail.is_empty() {
                        let preview: Vec<String> = avail.into_iter().take(6).collect();
                        return Err(sanitize_error(
                            format!(
                                "{label} 模型「{mdl}」目前不支援或不存在 (404)。您的 API Key 實際支援的模型包含：{}（可於上方直接改填或點選「查詢帳號可用模型」）",
                                preview.join("、")
                            ),
                            &key,
                        ));
                    }
                }
            }
            return Err(sanitize_error(
                format!("{label} API ({mdl}) 回傳錯誤 ({}): {}", status, body),
                &key,
            ));
        }
        let text = body["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| format!("{label} 回應格式異常，找不到藍圖文字內容。"))?;
        extract_json_object(text)
    } else {
        let mdl = model
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("gpt-4o-mini");
        let root = base_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/');
        let url = format!("{root}/chat/completions");
        let payload = json!({
            "model": mdl,
            "temperature": 0.4,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": BLUEPRINT_SYSTEM_PROMPT },
                { "role": "user", "content": user_msg }
            ]
        });
        let resp = client
            .post(&url)
            .bearer_auth(&key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| sanitize_error(format!("無法連線至 OpenAI 相容 API：{e}"), &key))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| sanitize_error(format!("讀取 OpenAI 相容 API 回應失敗：{e}"), &key))?;
        if !status.is_success() {
            return Err(sanitize_error(
                format!("OpenAI 相容 API ({mdl}) 回傳錯誤 ({}): {}", status, body),
                &key,
            ));
        }
        let text = body["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| "OpenAI 相容 API 回應格式異常，找不到藍圖文字內容。".to_string())?;
        extract_json_object(text)
    }
}

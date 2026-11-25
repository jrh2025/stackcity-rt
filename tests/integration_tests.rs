//! 《StackCity RT（像素疊界）》整合測試套件：
//! 1. 純 Rust VoxelCanvas 體素原語、AO、法線、發光與防閃爍測試
//! 2. 既有 `sprite-stacking` 專案真實 `blueprint.json` 100% 相容烘焙測試
//! 3. 68 種戶外造物與室內傢俱預設與中文語意修飾詞生成測試
//! 4. 純 Rust 四視圖 PNG 視覺外殼雕刻測試
//! 5. SQLite 單檔 `.stackworld.db` 十二大戶外/室內主題建置、VACUUM INTO 匯出與驗證匯入測試
//! 6. Axum HTTP API 端到端測試（含地圖擴展 resize 與自訂背景音樂 BLOB 往返）

use image::{Rgba, RgbaImage};
use serde_json::{json, Value};
use stackcity_rt::api::{build_router, AppState};
use stackcity_rt::db::{
    ensure_data_dir_initialized, export_world_db_bytes, import_world_db_bytes,
    load_full_world_state, presets::create_world_from_preset,
};
use stackcity_rt::voxel::blueprint::bake_blueprint_json;
use stackcity_rt::voxel::carver::carve_four_views;
use stackcity_rt::voxel::generators::{generate_from_prompt, list_presets};
use stackcity_rt::voxel::VoxelCanvas;
use std::io::Cursor;
use std::path::Path;
use tokio::net::TcpListener;

#[test]
fn test_voxel_canvas_primitives_and_baking() {
    let mut c = VoxelCanvas::new(32, 32, 32);
    c.fill_box(6, 6, 0, 26, 26, 12, [144, 150, 158], true, false, "stone", None);
    c.fill_cylinder(15.5, 15.5, 12, 24, 6.0, 6.0, [52, 114, 206], false, 2.0, true, false, "metal");
    c.fill_box(14, 24, 14, 18, 27, 20, [255, 232, 88], false, true, "flat", None);

    let baked = c
        .bake("測試碉堡", "building", None, 1.0, "static", true, true)
        .expect("烘焙應該成功");

    assert!(baked.diagnostics.voxels > 1000);
    assert!(baked.diagnostics.emissive_voxels > 0);
    assert!(baked.diagnostics.blink_free);
    assert!(!baked.slices_png.is_empty());
    assert!(!baked.normals_png.is_empty());
    assert!(!baked.emissive_png.is_empty());

    let img = image::load_from_memory(&baked.slices_png).expect("slices.png 應為合法 PNG");
    assert_eq!(img.width() as usize, baked.slice_w * baked.slices);
    assert_eq!(img.height() as usize, baked.slice_h);
}

#[test]
fn test_existing_workspace_blueprints_compatibility() {
    let candidates = [
        ("..\\outputs\\adult_male_lion\\blueprint.json", "animal"),
        ("..\\outputs\\bulbasaur\\blueprint.json", "character"),
    ];
    for (rel, cat) in candidates {
        let p = Path::new(rel);
        if !p.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(p).expect("讀取既有藍圖失敗");
        let bp: Value = serde_json::from_str(&raw).expect("解析既有藍圖 JSON 失敗");
        let baked = bake_blueprint_json(&bp, cat, 0.8, "walk", None, true, true)
            .unwrap_or_else(|e| panic!("烘焙既有藍圖 {rel} 失敗：{e}"));
        assert!(baked.diagnostics.voxels > 5000, "{rel} 體素數應 > 5000");
        assert!(baked.diagnostics.blink_free, "{rel} 應通過防閃爍檢查");
        assert!(baked.diagnostics.emissive_voxels > 0, "{rel} 應保留夜間發光體素");
    }
}

#[test]
fn test_all_presets_and_chinese_modifiers() {
    let presets = list_presets();
    assert!(presets.len() >= 65, "應至少內建 65 種造物與室內傢俱原型（實得 {}）", presets.len());

    for key in [
        "castle",
        "knight",
        "lion",
        "sakura",
        "car",
        "lamp",
        "spaceship",
        "pagoda",
        // 新增室內建築專用預設物件
        "throne",
        "vending",
        "robot_arm",
        "office_desk",
        "blackboard",
        "sofa",
        "kitchen_stove",
    ] {
        let baked = generate_from_prompt(key, 36, 42)
            .unwrap_or_else(|e| panic!("生成預設 {key} 失敗：{e}"));
        assert!(baked.diagnostics.voxels > 150, "{key} 體素數量過少");
        assert!(baked.diagnostics.blink_free, "{key} 應防閃爍");
        assert!(baked.blueprint_json.is_some());
    }

    let snowy = generate_from_prompt("雪中的金色城堡", 40, 7)
        .expect("應能解析中文顏色與覆雪修飾詞");
    assert!(snowy.diagnostics.voxels > 1000);
    assert_eq!(snowy.category, "building");

    let custom = generate_from_prompt("奇幻漂浮的神秘方尖碑", 36, 9)
        .expect("自由創意描述應退回可編輯奇觀雕塑");
    assert!(custom.diagnostics.voxels > 300);
}

#[test]
fn test_four_view_carver() {
    // 程式建立一張純白底 1×4 正交四視圖測試圖（簡單塔狀物件）
    let mut sheet = RgbaImage::from_pixel(160, 48, Rgba([255, 255, 255, 255]));
    let draw_rect = |img: &mut RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32, col: [u8; 4]| {
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, Rgba(col));
            }
        }
    };
    // TOP (左1)
    draw_rect(&mut sheet, 10, 12, 30, 32, [60, 130, 210, 255]);
    // SIDE (左2)
    draw_rect(&mut sheet, 48, 8, 68, 40, [60, 130, 210, 255]);
    // FRONT (左3)
    draw_rect(&mut sheet, 86, 8, 106, 40, [220, 180, 60, 255]);
    // BACK (左4)
    draw_rect(&mut sheet, 124, 8, 144, 40, [50, 110, 180, 255]);

    let mut png_buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(sheet)
        .write_to(&mut png_buf, image::ImageFormat::Png)
        .expect("編碼測試四視圖失敗");

    let baked = carve_four_views(
        png_buf.get_ref(),
        "測試雕刻塔",
        "building",
        Some(24),
        true,
        false,
        true,
        "down",
        1.0,
    )
    .expect("四視圖雕刻應成功");
    assert!(baked.diagnostics.voxels > 500);
}

#[test]
fn test_sqlite_world_themes_export_and_import_roundtrip() {
    let dir = tempfile::tempdir().expect("建立測試暫存目錄失敗");
    ensure_data_dir_initialized(dir.path()).expect("初始化預設世界失敗");

    // 驗證除預設 kingdom 外的全數 11 個戶外與室內主題（總計 12 種場景範本）
    for theme in [
        "city",
        "farm",
        "campus",
        "harbor",
        "blank",
        "palace_interior",
        "shop_interior",
        "factory_interior",
        "office_interior",
        "classroom_interior",
        "home_interior",
    ] {
        let sum = create_world_from_preset(dir.path(), theme, &format!("測試_{theme}"), "測試員", None)
            .unwrap_or_else(|e| panic!("建立主題 {theme} 失敗：{e}"));
        if theme != "blank" {
            assert!(sum.entity_count >= 5, "主題 {theme} 應具備豐富預設物件");
            assert!(sum.asset_count >= 5, "主題 {theme} 應具備多樣資產原型");
        }
        let st = load_full_world_state(&dir.path().join(&sum.file_name), &sum.file_name)
            .unwrap_or_else(|e| panic!("載入主題 {theme} 失敗：{e}"));
        if theme != "blank" {
            let qc = st
                .meta
                .quest_config
                .as_ref()
                .unwrap_or_else(|| panic!("主題 {theme} 應預設附帶 quest_config 任務設定"));
            assert_eq!(qc["enabled"], json!(true), "主題 {theme} 預設任務應為啟用狀態");
            let interactive_cnt = st
                .entities
                .iter()
                .filter(|e| e.meta_json.contains("\"interaction\""))
                .count();
            assert!(
                interactive_cnt >= 3,
                "主題 {theme} 應至少具備 3 個任務互動 NPC／物件（實得 {interactive_cnt}）"
            );
        }
        // 驗證港灣與六大室內建築主題均具備預設相遇互動事件規則
        if theme == "harbor" || theme.ends_with("_interior") {
            assert!(
                st.events.iter().any(|ev| ev.trigger_type == "encounter"),
                "主題 {theme} 應內建物件相遇互動事件規則"
            );
        }
        // 驗證四大有水戶外主題均正確鋪設橋樑，且所有非水生／非飛行實體均未落入無橋水中
        if matches!(theme, "city" | "farm" | "harbor") {
            assert_no_land_entity_in_water(&st, theme);
        }
    }

    let src_path = dir.path().join("kingdom_dawn.stackworld.db");
    let src_state = load_full_world_state(&src_path, "kingdom_dawn.stackworld.db")
        .expect("讀取預設王國狀態失敗");
    assert_no_land_entity_in_water(&src_state, "kingdom");
    assert_eq!(src_state.terrain.len(), 1600);
    assert!(src_state.events.len() >= 5);
    assert!(!src_state.paths.is_empty());
    assert!(!src_state.wander_zones.is_empty());
    let k_qc = src_state.meta.quest_config.as_ref().expect("預設王國應包含 quest_config");
    assert_eq!(k_qc["enabled"], json!(true));
    assert_eq!(k_qc["goal_type"], json!("score"));

    // 測試 VACUUM INTO 原子單檔匯出
    let exported_bytes = export_world_db_bytes(&src_path).expect("匯出 .stackworld.db 失敗");
    assert!(exported_bytes.starts_with(b"SQLite format 3\0"));

    // 測試驗證匯入至另一乾淨目錄
    let friend_dir = tempfile::tempdir().expect("建立好友目錄失敗");
    ensure_data_dir_initialized(friend_dir.path()).expect("初始化好友目錄失敗");
    let imported_sum =
        import_world_db_bytes(&exported_bytes, friend_dir.path(), Some("好友王國"))
            .expect("匯入合法 .stackworld.db 應成功");
    assert_eq!(imported_sum.entity_count, src_state.entities.len() as i64);
    assert_eq!(imported_sum.asset_count, src_state.assets.len() as i64);

    // 確認無效檔案會被安全拒絕
    let bad = import_world_db_bytes(b"not a sqlite database", friend_dir.path(), None);
    assert!(bad.is_err());
}

#[tokio::test]
async fn test_axum_http_end_to_end_api() {
    let dir = tempfile::tempdir().expect("建立 API 測試目錄失敗");
    ensure_data_dir_initialized(dir.path()).expect("初始化資料目錄失敗");

    let app = build_router(AppState {
        data_dir: dir.path().to_path_buf(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("綁定測試埠失敗");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // 1. 取得世界清單與預設世界完整狀態
    let worlds: Value = client
        .get(format!("{base}/api/worlds"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let file_name = worlds["worlds"][0]["file_name"].as_str().unwrap().to_string();

    let state_resp = client
        .get(format!("{base}/api/worlds/{file_name}/state"))
        .send()
        .await
        .unwrap();
    assert_eq!(state_resp.status(), 200);
    let state_json: Value = state_resp.json().await.unwrap();
    let asset_id = state_json["assets"][0]["id"].as_str().unwrap().to_string();

    // 2. 取得貼圖 PNG
    let tex_resp = client
        .get(format!("{base}/api/worlds/{file_name}/assets/{asset_id}/slices.png"))
        .send()
        .await
        .unwrap();
    assert_eq!(tex_resp.status(), 200);
    let png_bytes = tex_resp.bytes().await.unwrap();
    assert!(png_bytes.starts_with(b"\x89PNG"));

    // 3. 呼叫造物端點即時生成新物件並投放、調整縮放大小
    let gen_resp: Value = client
        .post(format!("{base}/api/worlds/{file_name}/assets/generate"))
        .json(&json!({ "prompt": "藍色城市巴士", "slices": 34 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let new_asset_id = gen_resp["asset"]["id"].as_str().unwrap();

    let spawn_resp: Value = client
        .post(format!("{base}/api/worlds/{file_name}/entities"))
        .json(&json!({ "asset_id": new_asset_id, "x": 15.5, "y": 24.5, "scale": 1.8, "yaw": 90.0 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let new_ent_id = spawn_resp["entities"][0]["id"].as_str().unwrap();

    // 先測試單一 scale 向後相容，會將三軸同時設為 2.4
    let patch_resp: Value = client
        .patch(format!("{base}/api/worlds/{file_name}/entities/{new_ent_id}"))
        .json(&json!({ "scale": 2.4, "yaw": 180.0 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!((patch_resp["entity"]["scale_x"].as_f64().unwrap() - 2.4).abs() < 1e-6);
    assert!((patch_resp["entity"]["scale_y"].as_f64().unwrap() - 2.4).abs() < 1e-6);
    assert!((patch_resp["entity"]["scale_z"].as_f64().unwrap() - 2.4).abs() < 1e-6);

    // 再測試 XYZ 長寬高三軸各自獨立縮放並儲存
    let patch_xyz: Value = client
        .patch(format!("{base}/api/worlds/{file_name}/entities/{new_ent_id}"))
        .json(&json!({ "scale_x": 2.2, "scale_y": 0.7, "scale_z": 3.1 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!((patch_xyz["entity"]["scale_x"].as_f64().unwrap() - 2.2).abs() < 1e-6);
    assert!((patch_xyz["entity"]["scale_y"].as_f64().unwrap() - 0.7).abs() < 1e-6);
    assert!((patch_xyz["entity"]["scale_z"].as_f64().unwrap() - 3.1).abs() < 1e-6);

    // 3B. 設定玩家專屬角色與位置座標
    let meta_player: Value = client
        .patch(format!("{base}/api/worlds/{file_name}/meta"))
        .json(&json!({
            "player_asset_id": new_asset_id,
            "player_name": "勇者測試員",
            "player_scale": 1.25,
            "player_x": 12.0,
            "player_y": 14.0
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(meta_player["meta"]["player_name"], "勇者測試員");
    assert_eq!(meta_player["meta"]["player_asset_id"], new_asset_id);

    // 4. 驗證單檔 .stackworld.db 匯出端點
    let exp_resp = client
        .get(format!("{base}/api/worlds/{file_name}/export"))
        .send()
        .await
        .unwrap();
    assert_eq!(exp_resp.status(), 200);
    let exp_bytes = exp_resp.bytes().await.unwrap();
    assert!(exp_bytes.starts_with(b"SQLite format 3\0"));

    // 5. 驗證擴大 / 調整地圖尺寸（四邊各擴張 4 格，從預設 40×40 擴展至 48×48）與實體／玩家座標平移
    let resize_resp: Value = client
        .post(format!("{base}/api/worlds/{file_name}/resize"))
        .json(&json!({
            "add_north": 4,
            "add_south": 4,
            "add_west": 4,
            "add_east": 4,
            "fill_ground": "grass",
            "fill_elevation": 0
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resize_resp["meta"]["width"], 48);
    assert_eq!(resize_resp["meta"]["height"], 48);
    assert!((resize_resp["meta"]["player_x"].as_f64().unwrap() - 16.0).abs() < 1e-6);
    assert!((resize_resp["meta"]["player_y"].as_f64().unwrap() - 18.0).abs() < 1e-6);

    let after_resize: Value = client
        .get(format!("{base}/api/worlds/{file_name}/state"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after_resize["terrain"].as_array().unwrap().len(), 48 * 48);
    let moved_ent = after_resize["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"].as_str() == Some(new_ent_id))
        .expect("擴展地圖後原先投放的實體應仍存在");
    assert!((moved_ent["x"].as_f64().unwrap() - 19.5).abs() < 1e-6);
    assert!((moved_ent["y"].as_f64().unwrap() - 28.5).abs() < 1e-6);
    assert!((moved_ent["scale_x"].as_f64().unwrap() - 2.2).abs() < 1e-6);
    assert!((moved_ent["scale_y"].as_f64().unwrap() - 0.7).abs() < 1e-6);
    assert!((moved_ent["scale_z"].as_f64().unwrap() - 3.1).abs() < 1e-6);

    // 6. 驗證自訂背景音樂上傳、讀取、打包匯出/匯入保留 BLOB 及刪除清除
    let fake_wav: Vec<u8> = b"RIFF\x24\x00\x00\x00WAVEfmt \x10\x00\x00\x00".to_vec();
    let boundary = "----StackCityTestBoundary98765";
    let mut mp_body: Vec<u8> = Vec::new();
    mp_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"my_town.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    mp_body.extend_from_slice(&fake_wav);
    mp_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let bgm_up = client
        .post(format!("{base}/api/worlds/{file_name}/bgm"))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(mp_body)
        .send()
        .await
        .unwrap();
    assert_eq!(bgm_up.status(), 200);
    let bgm_up_json: Value = bgm_up.json().await.unwrap();
    assert_eq!(bgm_up_json["meta"]["has_custom_bgm"], true);
    assert_eq!(bgm_up_json["meta"]["custom_bgm_name"], "my_town.wav");

    let bgm_get = client
        .get(format!("{base}/api/worlds/{file_name}/bgm"))
        .send()
        .await
        .unwrap();
    assert_eq!(bgm_get.status(), 200);
    assert_eq!(bgm_get.bytes().await.unwrap().as_ref(), fake_wav.as_slice());

    // 匯出包含自訂 BGM、三軸縮放與玩家化身的完整 .stackworld.db，並驗證匯入後完整保留
    let exp_bgm_bytes = client
        .get(format!("{base}/api/worlds/{file_name}/export"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let reimport_dir = tempfile::tempdir().unwrap();
    let reimported = import_world_db_bytes(&exp_bgm_bytes, reimport_dir.path(), Some("音樂好友世界"))
        .expect("匯入含自訂音樂與玩家的世界應成功");
    let reimported_state = load_full_world_state(
        &reimport_dir.path().join(&reimported.file_name),
        &reimported.file_name,
    )
    .unwrap();
    assert!(reimported_state.meta.has_custom_bgm);
    assert_eq!(
        reimported_state.meta.custom_bgm_name.as_deref(),
        Some("my_town.wav")
    );
    assert_eq!(reimported_state.meta.player_name, "勇者測試員");
    assert_eq!(
        reimported_state.meta.player_asset_id.as_deref(),
        Some(new_asset_id)
    );
    let reimp_ent = reimported_state
        .entities
        .iter()
        .find(|e| e.id == new_ent_id)
        .unwrap();
    assert!((reimp_ent.scale_x - 2.2).abs() < 1e-6);
    assert!((reimp_ent.scale_y - 0.7).abs() < 1e-6);
    assert!((reimp_ent.scale_z - 3.1).abs() < 1e-6);

    let bgm_del = client
        .delete(format!("{base}/api/worlds/{file_name}/bgm"))
        .send()
        .await
        .unwrap();
    assert_eq!(bgm_del.status(), 200);
    let bgm_del_json: Value = bgm_del.json().await.unwrap();
    assert_eq!(bgm_del_json["meta"]["has_custom_bgm"], false);

    let bgm_after_del = client
        .get(format!("{base}/api/worlds/{file_name}/bgm"))
        .send()
        .await
        .unwrap();
    assert_eq!(bgm_after_del.status(), 404);
}

#[tokio::test]
async fn test_parent_detach_quest_and_world_reset_api() {
    let dir = tempfile::tempdir().expect("建立重置與附掛測試目錄失敗");
    ensure_data_dir_initialized(dir.path()).expect("初始化資料目錄失敗");

    let app = build_router(AppState {
        data_dir: dir.path().to_path_buf(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("綁定測試埠失敗");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let worlds: Value = client
        .get(format!("{base}/api/worlds"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let file_name = worlds["worlds"][0]["file_name"].as_str().unwrap().to_string();

    let init_state: Value = client
        .get(format!("{base}/api/worlds/{file_name}/state"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let init_count = init_state["entities"].as_array().unwrap().len();
    let ent_a_id = init_state["entities"][0]["id"].as_str().unwrap().to_string();
    let ent_b_id = init_state["entities"][1]["id"].as_str().unwrap().to_string();
    let asset_id = init_state["assets"][0]["id"].as_str().unwrap().to_string();
    let robber_id = init_state["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["meta_json"].as_str().unwrap_or("").contains("\"hide_on_defeat\":true"))
        .expect("奇幻星光王國應包含可退場的山賊物件")["id"]
        .as_str()
        .unwrap()
        .to_string();

    // 1. 驗證附掛搭乘 -> 循環附掛拒絕 -> 明確傳入 null 真正解除附掛
    let attach_resp: Value = client
        .patch(format!("{base}/api/worlds/{file_name}/entities/{ent_b_id}"))
        .json(&json!({ "parent_id": ent_a_id }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(attach_resp["entity"]["parent_id"].as_str(), Some(ent_a_id.as_str()));

    let cycle_resp = client
        .patch(format!("{base}/api/worlds/{file_name}/entities/{ent_a_id}"))
        .json(&json!({ "parent_id": ent_b_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(cycle_resp.status(), 400, "循環附掛應回傳 400 Bad Request");

    let detach_resp: Value = client
        .patch(format!("{base}/api/worlds/{file_name}/entities/{ent_b_id}"))
        .json(&json!({ "parent_id": null }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        detach_resp["entity"]["parent_id"].is_null(),
        "明確傳入 parent_id: null 應真正解除附掛"
    );

    let verify_detach: Value = client
        .get(format!("{base}/api/worlds/{file_name}/state"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ent_b_reloaded = verify_detach["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"].as_str() == Some(ent_b_id.as_str()))
        .unwrap();
    assert!(ent_b_reloaded["parent_id"].is_null(), "重讀資料庫後仍應保持解除附掛狀態");

    // 2. 模擬遊玩進度、隱藏敗退山賊，並新建一個玩家自創實體
    client
        .patch(format!("{base}/api/worlds/{file_name}/entities/{robber_id}"))
        .json(&json!({ "visible": false }))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/worlds/{file_name}/entities"))
        .json(&json!({ "asset_id": asset_id, "name": "玩家自建裝飾箱", "x": 16.0, "y": 28.0 }))
        .send()
        .await
        .unwrap();
    client
        .patch(format!("{base}/api/worlds/{file_name}/meta"))
        .json(&json!({
            "player_x": 18.0,
            "player_y": 24.0,
            "quest_progress": { "score": 75, "shield": 1, "completed": false }
        }))
        .send()
        .await
        .unwrap();

    // 3. 測試模式 A：progress_only（保留玩家自建物件、清除進度並恢復敗退劫匪可見）
    let reset_prog_resp = client
        .post(format!("{base}/api/worlds/{file_name}/reset"))
        .json(&json!({ "mode": "progress_only" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reset_prog_resp.status(), 200);
    let after_prog: Value = reset_prog_resp.json().await.unwrap();
    assert!(after_prog["meta"]["quest_progress"].is_null(), "清除進度後應無殘留任務進度");
    assert!(after_prog["meta"]["player_x"].is_null(), "清除進度後應清除玩家舊座標");
    assert_eq!(after_prog["entities"].as_array().unwrap().len(), init_count + 1);
    let robber_after = after_prog["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"].as_str() == Some(robber_id.as_str()))
        .unwrap();
    assert_eq!(robber_after["visible"], json!(true), "敗退隱藏的搶匪應於重置進度後復位可見");

    // 4. 測試模式 B：world_initial（完全回復最初場景出廠數量與預設任務）
    let reset_init_resp = client
        .post(format!("{base}/api/worlds/{file_name}/reset"))
        .json(&json!({ "mode": "world_initial" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reset_init_resp.status(), 200);
    let after_init: Value = reset_init_resp.json().await.unwrap();
    assert_eq!(after_init["entities"].as_array().unwrap().len(), init_count);
    assert!(after_init["meta"]["quest_progress"].is_null());
    assert_eq!(after_init["meta"]["quest_config"]["enabled"], json!(true));
}

/// 驗證有水主題必有橋樑，且所有非水生／非飛行物件與其路徑皆未落入無橋水中
fn assert_no_land_entity_in_water(st: &stackcity_rt::db::models::WorldFullState, theme: &str) {
    let w = st.meta.width;
    let h = st.meta.height;
    let has_bridge = st.terrain.iter().any(|t| t.road_type == "bridge");
    assert!(has_bridge, "有水主題 {theme} 應包含跨越水域的 bridge 橋樑格點");

    let is_water_no_bridge = |fx: f64, fy: f64| -> bool {
        let gx = (fx.floor() as i32).clamp(0, w - 1);
        let gy = (fy.floor() as i32).clamp(0, h - 1);
        if let Some(t) = st.terrain.get((gy * w + gx) as usize) {
            t.ground_type == "water" && t.road_type != "bridge"
        } else {
            false
        }
    };

    let is_air_or_water_actor = |name: &str, z: f64| -> bool {
        z >= 1.8 || ["船", "鴨", "海鷗", "鳥", "熱氣球", "飛船"].iter().any(|kw| name.contains(kw))
    };

    for ent in &st.entities {
        if is_air_or_water_actor(&ent.name, ent.z) {
            continue;
        }
        assert!(
            !is_water_no_bridge(ent.x, ent.y),
            "主題 {theme} 的陸地物件「{}」不應出生在無橋水域 ({}, {})",
            ent.name,
            ent.x,
            ent.y
        );
    }

    for p in &st.paths {
        let Some(eid) = &p.entity_id else { continue };
        let Some(ent) = st.entities.iter().find(|e| &e.id == eid) else { continue };
        if is_air_or_water_actor(&ent.name, ent.z) {
            continue;
        }
        for wp in &p.waypoints {
            assert!(
                !is_water_no_bridge(wp.x, wp.y),
                "主題 {theme} 路徑「{}」節點 ({}, {}) 不可落在無橋水域上",
                p.name,
                wp.x,
                wp.y
            );
        }
    }
}

/// 啟動獨立隨機埠之測試 HTTP 伺服器，回傳暫存目錄與基底網址
async fn spawn_test_server() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("建立 API 測試目錄失敗");
    ensure_data_dir_initialized(dir.path()).expect("初始化資料目錄失敗");
    let app = build_router(AppState {
        data_dir: dir.path().to_path_buf(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("綁定測試埠失敗");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (dir, format!("http://{addr}"))
}

#[tokio::test]
async fn test_standalone_html_export_is_self_contained_and_player_only() {
    let (_tmp, base) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let worlds: Value = client
        .get(format!("{base}/api/worlds"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let file_name = worlds["worlds"][0]["file_name"].as_str().unwrap().to_string();

    // 1. 保留原本的完整分享 (.stackworld.db)
    let db_resp = client
        .get(format!("{base}/api/worlds/{file_name}/export"))
        .send()
        .await
        .unwrap();
    assert_eq!(db_resp.status(), 200);
    assert!(db_resp.bytes().await.unwrap().starts_with(b"SQLite format 3\0"));

    // 2. 新增單純遊戲分享（獨立純玩家 HTML 網頁檔）
    let html_resp = client
        .get(format!("{base}/api/worlds/{file_name}/export-html"))
        .send()
        .await
        .unwrap();
    assert_eq!(html_resp.status(), 200);
    let ct = html_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.contains("text/html"));

    let body = html_resp.text().await.unwrap();
    assert!(body.contains("window.__STACKCITY_STANDALONE__ ="));
    assert!(body.contains("data:image/png;base64,"));
    assert!(body.contains("id=\"langSelect\""));
    assert!(body.contains("initI18n"));
    assert!(body.contains("bootStandalonePlayer();"));
    // 確保完全不包含任何編輯側欄、造物工坊或模式切換按鈕
    assert!(!body.contains("id=\"leftSidebar\""));
    assert!(!body.contains("id=\"rightSidebar\""));
    assert!(!body.contains("id=\"btnModeToggle\""));
    assert!(!body.contains("id=\"studioModal\""));
}

#[tokio::test]
async fn test_delete_last_world_recreates_default() {
    let (_tmp, base) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let worlds: Value = client
        .get(format!("{base}/api/worlds"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let arr = worlds["worlds"].as_array().unwrap();
    assert!(!arr.is_empty());

    // 依序刪除所有現存世界，驗證最後一個世界刪除後會自動重建預設世界
    let mut last_resp = Value::Null;
    for w in arr {
        let fnm = w["file_name"].as_str().unwrap();
        let resp = client
            .delete(format!("{base}/api/worlds/{fnm}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        last_resp = resp.json().await.unwrap();
    }
    assert_eq!(last_resp["ok"], json!(true));
    assert_eq!(last_resp["recreated_default"], json!(true));
    assert_eq!(last_resp["worlds"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_library_and_world_asset_edit_and_delete() {
    let (_tmp, base) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let worlds: Value = client
        .get(format!("{base}/api/worlds"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let file_name = worlds["worlds"][0]["file_name"].as_str().unwrap().to_string();

    // 1. 透過 3D 積木藍圖烘焙一個自訂物件（自動同時收錄於本世界與個人跨世界資產庫）
    let bp = json!({
        "title": "測試木頭寶箱",
        "size": [32, 32, 32],
        "primitives": [
            { "type": "box", "x0": 8, "y0": 8, "z0": 0, "x1": 24, "y1": 24, "z1": 12, "color": "wood", "material": "wood" },
            { "type": "box", "x0": 14, "y0": 22, "z0": 6, "x1": 18, "y1": 25, "z1": 10, "color": "gold", "material": "metal", "emissive": true }
        ]
    });
    let bake_resp: Value = client
        .post(format!("{base}/api/worlds/{file_name}/assets/blueprint"))
        .json(&json!({ "blueprint": bp, "category": "prop" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let world_asset_id = bake_resp["asset"]["id"].as_str().unwrap().to_string();
    let orig_sha = bake_resp["asset"]["sha256"].as_str().unwrap().to_string();

    // 在地圖投放一個使用該世界資產的實體
    let spawn_resp: Value = client
        .post(format!("{base}/api/worlds/{file_name}/entities"))
        .json(&json!({ "asset_id": world_asset_id, "x": 15.0, "y": 15.0 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let spawned_eid = spawn_resp["entities"][0]["id"].as_str().unwrap().to_string();

    // 2. 檢查個人資產庫存在此項目
    let lib_list: Value = client
        .get(format!("{base}/api/assets/library"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let lib_item = lib_list["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["sha256"].as_str() == Some(orig_sha.as_str()))
        .expect("烘焙後應自動收錄進個人跨世界資產庫");
    let lib_id = lib_item["id"].as_str().unwrap().to_string();

    // 3. 編輯個人資產庫物件（改名、改分類、並調整積木長寬高重烘）
    let updated_bp = json!({
        "title": "升級黃金大寶箱",
        "size": [36, 36, 36],
        "primitives": [
            { "type": "box", "x0": 6, "y0": 6, "z0": 0, "x1": 30, "y1": 30, "z1": 18, "color": "gold", "material": "metal", "emissive": false }
        ]
    });
    let lib_patch: Value = client
        .patch(format!("{base}/api/assets/library/{lib_id}"))
        .json(&json!({
            "name": "升級黃金大寶箱",
            "category": "building",
            "blueprint": updated_bp
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(lib_patch["asset"]["name"], "升級黃金大寶箱");
    assert_eq!(lib_patch["asset"]["category"], "building");
    assert_ne!(lib_patch["asset"]["sha256"].as_str().unwrap(), orig_sha);

    // 4. 編輯本世界資產並重烘
    let world_patch: Value = client
        .patch(format!("{base}/api/worlds/{file_name}/assets/{world_asset_id}"))
        .json(&json!({
            "name": "世界專用豪華箱",
            "category": "prop",
            "blueprint": updated_bp
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(world_patch["asset"]["name"], "世界專用豪華箱");

    // 5. 刪除本世界資產（應聯級移除地圖上的實體）
    let world_del: Value = client
        .delete(format!("{base}/api/worlds/{file_name}/assets/{world_asset_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(world_del["ok"], json!(true));
    assert!(world_del["removed_entity_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x.as_str() == Some(spawned_eid.as_str())));

    // 6. 刪除個人資產庫項目
    let lib_del: Value = client
        .delete(format!("{base}/api/assets/library/{lib_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(lib_del["ok"], json!(true));

    let lib_after: Value = client
        .get(format!("{base}/api/assets/library"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!lib_after["assets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["id"].as_str() == Some(lib_id.as_str())));
}

#[tokio::test]
async fn test_gemma_ai_request_omits_system_instruction_and_supports_custom_model() {
    use stackcity_rt::voxel::ai::{
        build_google_request_body, check_env_api_keys, validate_model_name,
    };

    // 支援自動剝除誤貼的 models/ 前綴與 Gemma 4 模型代號
    assert_eq!(
        validate_model_name("models/gemma-4-31b-it").unwrap(),
        "gemma-4-31b-it"
    );
    assert_eq!(
        validate_model_name("gemma-4-26b-a4b-it").unwrap(),
        "gemma-4-26b-a4b-it"
    );

    let gemma4_body = build_google_request_body("gemma", "gemma-4-31b-it", "中世紀風車");
    assert!(
        gemma4_body.get("systemInstruction").is_none(),
        "Gemma 4 請求不應包含 systemInstruction，以免端點回傳 400"
    );
    assert!(
        gemma4_body["generationConfig"].get("responseMimeType").is_none(),
        "Gemma 4 請求不應強制設定 responseMimeType"
    );
    assert!(gemma4_body["contents"][0]["parts"][0]["text"]
        .as_str()
        .unwrap()
        .contains("中世紀風車"));

    let gemini_body = build_google_request_body("gemini", "gemini-3-flash-preview", "中世紀風車");
    assert!(gemini_body.get("systemInstruction").is_some());
    assert_eq!(
        gemini_body["generationConfig"]["responseMimeType"],
        json!("application/json")
    );

    let status_val = serde_json::to_value(check_env_api_keys()).unwrap();
    assert!(status_val.get("gemma").is_some());
    assert!(status_val.get("gemini").is_some());
    assert!(status_val.get("openai").is_some());
}

#[tokio::test]
async fn test_blueprint_preview_does_not_persist_and_rename_save_works() {
    let (_tmp, base) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let worlds: Value = client
        .get(format!("{base}/api/worlds"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let file_name = worlds["worlds"][0]["file_name"].as_str().unwrap().to_string();

    let lib_before: Value = client
        .get(format!("{base}/api/assets/library"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let lib_count_before = lib_before["assets"].as_array().unwrap().len();

    // 1. 呼叫純預覽端點：僅回傳 Data URL，絕不寫入個人資產庫或世界 DB
    let draft_bp = json!({
        "title": "草稿未存庫物件",
        "size": [32, 32, 32],
        "primitives": [
            { "type": "box", "x0": 8, "y0": 8, "z0": 0, "x1": 24, "y1": 24, "z1": 16, "color": "blue", "material": "crystal" }
        ]
    });
    let prev_resp: Value = client
        .post(format!("{base}/api/assets/preview-blueprint"))
        .json(&json!({ "name": "草稿未存庫物件", "blueprint": draft_bp, "category": "prop" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(prev_resp["slices_data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));

    let lib_after_preview: Value = client
        .get(format!("{base}/api/assets/library"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        lib_after_preview["assets"].as_array().unwrap().len(),
        lib_count_before,
        "純預覽絕不可寫入個人資產庫！"
    );

    // 2. 明確確認儲存至資產庫，並測試僅更改物件名稱（相同幾何 sha256）仍能正確儲存新名稱
    let save1: Value = client
        .post(format!("{base}/api/worlds/{file_name}/assets/blueprint"))
        .json(&json!({ "name": "水晶柱一號", "blueprint": draft_bp, "category": "prop" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(save1["asset"]["name"], "水晶柱一號");

    let save2: Value = client
        .post(format!("{base}/api/worlds/{file_name}/assets/blueprint"))
        .json(&json!({ "name": "最終命名水晶塔", "blueprint": draft_bp, "category": "building" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(save2["asset"]["name"], "最終命名水晶塔");
    assert_eq!(save2["asset"]["category"], "building");

    let lib_after_rename: Value = client
        .get(format!("{base}/api/assets/library"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        lib_after_rename["assets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "最終命名水晶塔"),
        "相同幾何僅改名儲存後，個人資產庫也必須正確更新為最終名稱"
    );
}

#[tokio::test]
async fn test_rename_world_and_list_reflects_new_name() {
    let (_tmp, base) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let worlds: Value = client
        .get(format!("{base}/api/worlds"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let file_name = worlds["worlds"][0]["file_name"].as_str().unwrap().to_string();

    // 1. 空白名稱應被拒絕 (400)
    let bad_resp = client
        .patch(format!("{base}/api/worlds/{file_name}/meta"))
        .json(&json!({ "name": "   " }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_resp.status(), 400);

    // 2. 重新命名為合法新名稱
    let ok_resp: Value = client
        .patch(format!("{base}/api/worlds/{file_name}/meta"))
        .json(&json!({ "name": "星光夢想新城市", "author": "夢想市長" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(ok_resp["meta"]["name"], "星光夢想新城市");
    assert_eq!(ok_resp["meta"]["author"], "夢想市長");

    // 3. 世界清單與狀態 API 皆須立即反映新名稱
    let worlds_after: Value = client
        .get(format!("{base}/api/worlds"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let found = worlds_after["worlds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["file_name"].as_str() == Some(file_name.as_str()))
        .unwrap();
    assert_eq!(found["name"], "星光夢想新城市");
}

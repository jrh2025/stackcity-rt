//! 《社畜的一天》5 世界跨門與固定公車通勤完整企劃建置器：
//! 1. 溫馨住家（入口起點：起床・刷牙洗臉・吃早餐 → 下班返家：盥洗・滑手機・睡覺）
//! 2. 都市通勤大道（暖心公寓・星辰科技總部・生活大賣場・美食餐廳 ＋ 4 座公車站牌與 208 號固定環線公車）
//! 3. 公司辦公室（業務聯繫・打印資料・製作文件・開會報告・同事午餐・主管任務・茶水間咖啡・突發急件）
//! 4. 生活大賣場（採購清單・鮮奶零食貨架・舒壓香氛展示櫃・收銀結帳）
//! 5. 療癒美食餐廳（點餐櫃檯・暖心主廚・窗邊舒壓晚餐）

use super::presets::{build_interior_walls, WorldBuilder};
use super::{
    init_world_schema, list_worlds, now_iso, open_world_conn, sanitize_world_filename,
    SCHEMA_VERSION, WORLD_EXT,
};
use crate::db::models::WorldSummary;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

struct CampaignTarget {
    world_id: String,
    file_name: String,
    world_name: String,
}

fn allocate_world_file(data_dir: &Path, raw_name: &str) -> Result<String, String> {
    let mut candidate = sanitize_world_filename(raw_name)?;
    if data_dir.join(&candidate).exists() {
        let stem = candidate.trim_end_matches(WORLD_EXT);
        let short = &Uuid::new_v4().to_string()[..6];
        candidate = format!("{stem}_{short}{WORLD_EXT}");
    }
    Ok(candidate)
}

fn portal_json(
    kind: &str,
    label: &str,
    target: &CampaignTarget,
    target_x: f64,
    target_y: f64,
    target_yaw: f64,
    target_portal_id: &str,
    door_offset_fwd: f64,
    required_item: &str,
    locked_text: &str,
) -> Value {
    json!({
        "enabled": true,
        "kind": kind,
        "label": label,
        "target_world_id": target.world_id,
        "target_world_file": target.file_name,
        "target_x": target_x,
        "target_y": target_y,
        "target_yaw": target_yaw,
        "target_portal_id": target_portal_id,
        "door_offset_fwd": door_offset_fwd,
        "required_item": required_item,
        "required_flag": "",
        "locked_text": locked_text
    })
}

fn force_entity_id(b: &mut WorldBuilder<'_>, temp_id: &str, fixed_id: &str) {
    if let Some(ent) = b.entities.iter_mut().find(|e| e.id == temp_id) {
        ent.id = fixed_id.to_string();
    }
}

fn base_meta_pairs(
    world_id: &str,
    world_name: &str,
    author: &str,
    desc: &str,
    theme: &str,
    w: i32,
    h: i32,
    start_time: f64,
    bgm_preset: &str,
    player_asset_id: String,
    player_x: f64,
    player_y: f64,
) -> Vec<(&'static str, String)> {
    let created = now_iso();
    vec![
        ("schema_version", SCHEMA_VERSION.to_string()),
        ("world_id", world_id.to_string()),
        ("name", world_name.to_string()),
        ("author", author.to_string()),
        ("description", desc.to_string()),
        ("theme", theme.to_string()),
        ("width", w.to_string()),
        ("height", h.to_string()),
        ("time_of_day", start_time.to_string()),
        ("initial_time_of_day", start_time.to_string()),
        ("time_speed", "0.8".to_string()),
        ("weather", "clear".to_string()),
        ("bgm_preset", bgm_preset.to_string()),
        ("bgm_volume", "0.45".to_string()),
        ("player_asset_id", player_asset_id),
        ("player_name", "上班族小美".to_string()),
        ("player_scale", "1.0".to_string()),
        ("player_x", player_x.to_string()),
        ("player_y", player_y.to_string()),
        ("created_at", created.clone()),
        ("updated_at", created),
        ("imported_world", "1".to_string()),
        ("thumbnail_b64", String::new()),
    ]
}

fn open_fresh_campaign_db(data_dir: &Path, target: &CampaignTarget) -> Result<Connection, String> {
    let path = data_dir.join(&target.file_name);
    let conn = open_world_conn(&path)?;
    init_world_schema(&conn)?;
    Ok(conn)
}

/// 一次建置《社畜的一天》5 座透過「門」相互連結的完整遊戲世界
pub fn create_office_worker_day_campaign(
    data_dir: &Path,
    base_name: &str,
    author: &str,
) -> Result<Vec<WorldSummary>, String> {
    let clean_base = {
        let trimmed = base_name.trim();
        if trimmed.is_empty() {
            "社畜的一天"
        } else {
            trimmed
        }
    };

    let home = CampaignTarget {
        world_id: Uuid::new_v4().to_string(),
        world_name: format!("{clean_base}・溫馨住家"),
        file_name: allocate_world_file(data_dir, &format!("{clean_base}_溫馨住家"))?,
    };
    let city = CampaignTarget {
        world_id: Uuid::new_v4().to_string(),
        world_name: format!("{clean_base}・都市通勤大道"),
        file_name: allocate_world_file(data_dir, &format!("{clean_base}_都市通勤大道"))?,
    };
    let office = CampaignTarget {
        world_id: Uuid::new_v4().to_string(),
        world_name: format!("{clean_base}・公司辦公室"),
        file_name: allocate_world_file(data_dir, &format!("{clean_base}_公司辦公室"))?,
    };
    let mall = CampaignTarget {
        world_id: Uuid::new_v4().to_string(),
        world_name: format!("{clean_base}・生活大賣場"),
        file_name: allocate_world_file(data_dir, &format!("{clean_base}_生活大賣場"))?,
    };
    let rest = CampaignTarget {
        world_id: Uuid::new_v4().to_string(),
        world_name: format!("{clean_base}・療癒美食餐廳"),
        file_name: allocate_world_file(data_dir, &format!("{clean_base}_療癒美食餐廳"))?,
    };

    // 預配全部雙向門實體 ID
    let home_exit_id = Uuid::new_v4().to_string();
    let city_apt_id = Uuid::new_v4().to_string();
    let city_office_id = Uuid::new_v4().to_string();
    let office_exit_id = Uuid::new_v4().to_string();
    let city_mall_id = Uuid::new_v4().to_string();
    let mall_exit_id = Uuid::new_v4().to_string();
    let city_rest_id = Uuid::new_v4().to_string();
    let rest_exit_id = Uuid::new_v4().to_string();

    build_campaign_home(data_dir, author, &home, &city, &home_exit_id, &city_apt_id)?;
    build_campaign_city(
        data_dir,
        author,
        &city,
        (&home, &home_exit_id, &city_apt_id),
        (&office, &office_exit_id, &city_office_id),
        (&mall, &mall_exit_id, &city_mall_id),
        (&rest, &rest_exit_id, &city_rest_id),
    )?;
    build_campaign_office(data_dir, author, &office, &city, &office_exit_id, &city_office_id)?;
    build_campaign_mall(data_dir, author, &mall, &city, &mall_exit_id, &city_mall_id)?;
    build_campaign_restaurant(data_dir, author, &rest, &city, &rest_exit_id, &city_rest_id)?;

    let ordered_files = [
        &home.file_name,
        &city.file_name,
        &office.file_name,
        &mall.file_name,
        &rest.file_name,
    ];
    for fname in ordered_files {
        super::create_snapshot(&data_dir.join(fname), fname, "__initial__")?;
    }
    let all = list_worlds(data_dir)?;
    let mut out = Vec::with_capacity(5);
    for fname in ordered_files {
        let sum = all
            .iter()
            .find(|s| &s.file_name == fname)
            .cloned()
            .ok_or_else(|| format!("建置社畜的一天世界後無法讀取：{fname}"))?;
        out.push(sum);
    }
    Ok(out)
}

fn build_campaign_home(
    data_dir: &Path,
    author: &str,
    home: &CampaignTarget,
    city: &CampaignTarget,
    home_exit_id: &str,
    city_apt_id: &str,
) -> Result<(), String> {
    let conn = open_fresh_campaign_db(data_dir, home)?;
    let mut b = WorldBuilder::new(&conn, data_dir, 24, 24, "wood_floor");
    let ol_asset = b.ensure_asset("office_lady", 40)?;
    build_interior_walls(&mut b, "wall_cream");

    // 浴室／廚房磁磚區與客廳暖紅地毯
    b.fill_rect(14, 2, 23, 15, 1, "tile_floor", "none");
    b.fill_rect(4, 14, 12, 21, 1, "carpet_red", "none");

    let alarm_id = b.spawn("lamp", 34, "床頭貪睡鬧鐘", 4.5, 5.2, 0.0, 0.0, 0.9, "night_light")?;
    let bed_id = b.spawn("bed", 24, "柔軟雙人床", 7.0, 5.6, 0.0, 0.0, 1.08, "prop")?;
    b.spawn("bookshelf", 40, "臥室生活書櫃", 11.5, 3.8, 0.0, 0.0, 0.95, "prop")?;

    let wash_morning_id =
        b.spawn("sink_vanity", 34, "浴室洗手台（早晨刷牙洗臉）", 16.5, 4.2, 0.0, 0.0, 1.0, "prop")?;
    let wash_night_id =
        b.spawn("sink_vanity", 34, "下班舒壓熱水淋浴區", 20.6, 4.5, 0.0, 0.0, 1.0, "prop")?;
    b.set_tint(&wash_night_id, "#bae6fd");

    b.spawn("kitchen_stove", 28, "廚房流理台", 20.2, 10.2, 0.0, 0.0, 1.05, "night_light")?;
    let dining_id = b.spawn("dining_table", 22, "晨間早餐餐桌", 17.5, 14.5, 0.0, 0.0, 1.05, "prop")?;
    let tv_id = b.spawn("tv_stand", 32, "客廳大電視", 7.5, 12.0, 0.0, 180.0, 1.0, "night_light")?;
    let sofa_id = b.spawn("sofa", 26, "下班廢人沙發", 7.5, 17.6, 0.0, 0.0, 1.05, "prop")?;
    b.spawn("potted_plant", 34, "陽台琴葉榕", 4.2, 18.0, 0.0, 0.0, 1.0, "plant")?;

    let dog_id = b.spawn("shiba", 30, "愛犬柴柴・橘子", 12.2, 15.8, 0.0, 90.0, 0.98, "animal")?;
    b.add_wander_zone("客廳散步地毯區", 12.0, 15.5, 3.0, 0.9, vec![dog_id.clone()], "#4ade80");

    // 南側玄關大門（進入都市通勤大道，需帶上通勤悠遊卡）
    let tmp_door_id = b.spawn(
        "wooden_door",
        36,
        "家門出口（往都市大道）",
        12.0,
        21.8,
        0.0,
        180.0,
        1.0,
        "portal_exit",
    )?;
    force_entity_id(&mut b, &tmp_door_id, home_exit_id);
    b.set_portal(
        home_exit_id,
        portal_json(
            "enter",
            "出門上班／前往都市大道",
            city,
            6.9,
            27.5,
            270.0,
            city_apt_id,
            0.65,
            "通勤悠遊卡",
            "出門上班前別忘了先在「晨間早餐餐桌」吃早餐並拿走「通勤悠遊卡」喔！",
        ),
    );

    b.add_event(
        "點擊電視切換新聞與追劇頻道",
        "click",
        json!({"entity_id": tv_id}),
        vec![
            json!({"type": "play_sound", "sound": "chime"}),
            json!({"type": "speech_bubble", "entity_id": tv_id, "text": "早間氣象：今天晴朗微風，記得帶悠遊卡搭 208 公車！", "duration": 3.8}),
        ],
        1.5,
    );

    // 早晨任務 1：起床
    b.set_interaction(
        &alarm_id,
        json!({
            "kind": "clue",
            "title": "07:30 按掉鬧鐘・元氣起床！",
            "dialogue": "「叮鈴鈴——！」再賴五分鐘⋯⋯不行！今天是重要工作日，深呼吸伸個懶腰起床吧！接下來去東北角的「浴室洗手台」刷牙洗臉！",
            "clue_text": "【早安行程】① 關鬧鐘起床 → ② 浴室刷牙洗臉 → ③ 餐桌吃早餐拿悠遊卡 → 出大門搭公車！",
            "reward_points": 15,
            "reward_item": "清醒鬥志",
            "reward_flag": "wakeup",
            "once": true
        }),
    );
    // 早晨任務 2：刷牙洗臉
    b.set_interaction(
        &wash_morning_id,
        json!({
            "kind": "support",
            "title": "刷牙洗臉＆清爽上班淡妝",
            "dialogue": "用薄荷牙膏刷牙、冷水洗臉，鏡子裡的小美精神百倍！整裝完畢精神護盾 +1！",
            "clue_text": "【早晨盥洗完成】清爽微笑 ready！接著到「晨間早餐餐桌」吃早餐！",
            "grant_shield": 1,
            "heal_hp": 2,
            "reward_points": 15,
            "reward_flag": "morning_wash",
            "once": true
        }),
    );
    // 早晨任務 3：吃早餐＋帶悠遊卡
    b.set_interaction(
        &dining_id,
        json!({
            "kind": "clue",
            "title": "享用熱騰騰早餐＆帶上悠遊卡",
            "dialogue": "烤吐司配半熟太陽蛋和熱奶茶超幸福！把玄關旁的「通勤悠遊卡」收進托特包，就可以從南邊家門出發搭 208 號公車上班囉！",
            "clue_text": "【早餐＆悠遊卡】已取得「通勤悠遊卡」，前往南邊大門進入「都市通勤大道」並按 G 搭乘 208 公車！",
            "reward_points": 20,
            "reward_item": "通勤悠遊卡",
            "reward_flag": "breakfast",
            "once": true
        }),
    );
    // 柴犬打氣
    b.set_interaction(
        &dog_id,
        json!({
            "kind": "support",
            "title": "愛犬橘子的早安加油打氣",
            "dialogue": "汪嗚～橘子搖著尾巴蹭了蹭小美的裙擺，祝你今天上班順順利利！",
            "clue_text": "【橘子打氣】獲得柴柴溫暖能量（體力 +1、護盾 +1）！",
            "grant_shield": 1,
            "heal_hp": 1,
            "reward_points": 10,
            "once": true
        }),
    );
    // 夜晚返家 1：晚間舒壓盥洗（須吃完晚餐拿到美食晚餐收據）
    b.set_interaction(
        &wash_night_id,
        json!({
            "kind": "treasure",
            "title": "下班回家・舒壓熱水盥洗",
            "dialogue": "卸下整天上班疲勞，用薰衣草沐浴乳洗個熱呼呼的澡吧！",
            "required_item": "美食晚餐收據",
            "locked_text": "這是下班回家後的「晚間舒壓盥洗區」喔！請先出門完成公司工作、逛生活大賣場並在美食餐廳享用晚餐取得「美食晚餐收據」再回來！",
            "success_text": "好暖好放鬆！換上剛洗完澡的「香噴噴睡衣」，去客廳沙發滑手機追劇吧！",
            "reward_points": 25,
            "reward_item": "香噴噴睡衣",
            "reward_flag": "night_wash",
            "once": true
        }),
    );
    // 夜晚返家 2：沙發滑手機
    b.set_interaction(
        &sofa_id,
        json!({
            "kind": "treasure",
            "title": "下班儀式・窩在沙發滑手機追劇",
            "dialogue": "陷進軟綿綿的布藝沙發，配著賣場買來的零食飲料刷社群、看最愛影集！",
            "required_item": "香噴噴睡衣",
            "locked_text": "先到東北角「下班舒壓熱水淋浴區」盥洗換上「香噴噴睡衣」，再躺上沙發滑手機更享受喔！",
            "success_text": "哈哈太舒壓了～眼皮開始微沉，抱著「睡意滿點枕頭」上床睡覺吧！",
            "reward_points": 25,
            "reward_item": "睡意滿點枕頭",
            "reward_flag": "scroll_phone",
            "once": true
        }),
    );
    // 夜晚返家 3：蓋被睡覺（壓軸收官）
    b.set_interaction(
        &bed_id,
        json!({
            "kind": "treasure",
            "title": "晚安入睡・結束充實的社畜一天",
            "dialogue": "拉好暖呼呼的棉被，明天也是充滿希望的一天，晚安！",
            "required_item": "睡意滿點枕頭",
            "locked_text": "還沒到睡覺時間喔！請依序完成一天的工作、大賣場採買、餐廳晚餐、回家盥洗並在沙發滑手機拿到「睡意滿點枕頭」再蓋被安睡！",
            "success_text": "😴 進入甜蜜夢鄉！今天從起床通勤、高效辦公到療癒夜晚全都滿分達成！",
            "reward_points": 60,
            "reward_flag": "sleep_done",
            "once": true
        }),
    );

    // 主線全日 12 項綜合清單（跨 5 座世界即時同步追蹤）
    b.set_quest(json!({
        "enabled": true,
        "title": "💼 社畜的一天：女上班族全日充實日記（12 項里程碑）",
        "goal_type": "custom_checklist",
        "target_count": 12,
        "culprit_entity_id": "",
        "checklist": [
            { "key": "wakeup", "label": "【住家】按掉床頭鬧鐘起床" },
            { "key": "morning_wash", "label": "【住家】浴室洗手台刷牙洗臉" },
            { "key": "breakfast", "label": "【住家】吃早餐並帶走通勤悠遊卡" },
            { "key": "biz_contact", "label": "【公司】工位進行業務客戶聯繫" },
            { "key": "print_docs", "label": "【公司】事務機打印簡報資料" },
            { "key": "make_docs", "label": "【公司】製作季度提案文件" },
            { "key": "meeting_report", "label": "【公司】會議桌開會與進度討論" },
            { "key": "colleague_lunch", "label": "【公司】和同事阿凱一起吃午餐" },
            { "key": "boss_task", "label": "【公司】向主管王經理報告並完成任務" },
            { "key": "mall_shopping", "label": "【賣場】進入賣場購買指定商品" },
            { "key": "dinner_eat", "label": "【餐廳】餐廳點餐並享用舒壓晚餐" },
            { "key": "sleep_done", "label": "【返家】晚間盥洗・沙發滑手機・安心入睡" }
        ],
        "description": "化身女上班族小美！早晨完成起床・刷牙・早餐後走南門進都市，按 G 搭 208 公車前往「星辰科技總部」上班、下班逛「生活大賣場」與「美食餐廳」，再搭公車回家盥洗、滑手機、入睡即可滿分通關！",
        "victory_title": "🏆 社畜女王的一天大成功！",
        "victory_message": "從清晨鬧鐘、公車通勤、辦公室完美簡報，到下班賣場掃貨與餐廳晚餐、睡前儀式——充實又療癒的每一刻都拿下滿分！"
    }));

    let meta = base_meta_pairs(
        &home.world_id,
        &home.world_name,
        author,
        "《社畜的一天》入口起點・溫馨住家：起床、刷牙洗臉、吃早餐出門，夜晚回家盥洗、滑手機與安眠。",
        "home_interior",
        24,
        24,
        7.5,
        "indoor",
        ol_asset.id,
        8.8,
        7.8,
    );
    b.flush(meta)
}

fn build_campaign_city(
    data_dir: &Path,
    author: &str,
    city: &CampaignTarget,
    home_link: (&CampaignTarget, &str, &str),
    office_link: (&CampaignTarget, &str, &str),
    mall_link: (&CampaignTarget, &str, &str),
    rest_link: (&CampaignTarget, &str, &str),
) -> Result<(), String> {
    let conn = open_fresh_campaign_db(data_dir, city)?;
    let mut b = WorldBuilder::new(&conn, data_dir, 40, 40, "grass");
    let ol_asset = b.ensure_asset("office_lady", 40)?;

    // 中央市民綠蔭公園廣場（石板步道）與四周 2 格寬閉合柏油公車環線（絕無水域阻隔）
    b.fill_rect(13, 13, 27, 27, 1, "grass", "stone");
    b.fill_rect(15, 15, 25, 25, 1, "grass", "none");
    // 外環 2 格寬柏油公車專用大道：x:8..10, x:30..32, y:8..10, y:30..32
    b.fill_rect(8, 8, 10, 32, 1, "grass", "asphalt");
    b.fill_rect(30, 8, 32, 32, 1, "grass", "asphalt");
    b.fill_rect(8, 8, 32, 10, 1, "grass", "asphalt");
    b.fill_rect(8, 30, 32, 32, 1, "grass", "asphalt");
    // 人行道石板接駁各棟大門與公園
    b.fill_rect(5, 26, 8, 29, 1, "grass", "stone");
    b.fill_rect(5, 10, 8, 13, 1, "grass", "stone");
    b.fill_rect(32, 10, 35, 13, 1, "grass", "stone");
    b.fill_rect(32, 26, 35, 29, 1, "grass", "stone");
    b.set_road_line(10, 20, 30, 20, "stone");
    b.set_road_line(20, 10, 20, 30, "stone");

    // 4 棟跨世界聯動建築與 4 座公車站牌候車亭
    let (home_target, home_exit_id, city_apt_id) = home_link;
    let tmp_apt = b.spawn("cottage", 40, "暖心公寓大樓（小美住家）", 4.2, 27.5, 0.0, 270.0, 1.28, "landmark")?;
    force_entity_id(&mut b, &tmp_apt, city_apt_id);
    b.set_portal(
        city_apt_id,
        portal_json(
            "enter",
            "回家／進入「溫馨住家」",
            home_target,
            12.0,
            19.8,
            0.0,
            home_exit_id,
            1.55,
            "",
            "",
        ),
    );
    b.spawn("bus_stop", 34, "① 住家社區公車站", 6.4, 24.0, 0.0, 270.0, 0.92, "prop")?;

    let (office_target, office_exit_id, city_office_id) = office_link;
    let tmp_off = b.spawn("skyscraper", 48, "星辰科技總部大樓（公司）", 4.2, 11.5, 0.0, 270.0, 1.08, "landmark")?;
    force_entity_id(&mut b, &tmp_off, city_office_id);
    b.set_portal(
        city_office_id,
        portal_json(
            "enter",
            "進公司打卡／進入「公司辦公室」",
            office_target,
            12.0,
            19.8,
            0.0,
            office_exit_id,
            1.55,
            "",
            "",
        ),
    );
    b.spawn("bus_stop", 34, "② 科技總部公車站", 6.4, 15.0, 0.0, 270.0, 0.92, "prop")?;

    let (mall_target, mall_exit_id, city_mall_id) = mall_link;
    let tmp_mall = b.spawn("shop", 40, "生活精品大賣場", 35.8, 11.5, 0.0, 90.0, 1.25, "shop")?;
    force_entity_id(&mut b, &tmp_mall, city_mall_id);
    b.set_portal(
        city_mall_id,
        portal_json(
            "enter",
            "逛街採買／進入「生活大賣場」",
            mall_target,
            12.0,
            19.8,
            0.0,
            mall_exit_id,
            1.55,
            "",
            "",
        ),
    );
    b.spawn("bus_stop", 34, "③ 生活賣場公車站", 33.6, 15.0, 0.0, 90.0, 0.92, "prop")?;

    let (rest_target, rest_exit_id, city_rest_id) = rest_link;
    let tmp_rest = b.spawn("shop", 40, "深夜療癒美食餐廳", 35.8, 27.5, 0.0, 90.0, 1.2, "shop")?;
    force_entity_id(&mut b, &tmp_rest, city_rest_id);
    b.set_tint(city_rest_id, "#fde68a");
    b.set_portal(
        city_rest_id,
        portal_json(
            "enter",
            "享用晚餐／進入「療癒美食餐廳」",
            rest_target,
            12.0,
            19.8,
            0.0,
            rest_exit_id,
            1.55,
            "",
            "",
        ),
    );
    b.spawn("bus_stop", 34, "④ 美食餐廳公車站", 33.6, 24.0, 0.0, 90.0, 0.92, "prop")?;

    // 公園景觀、街燈與樹木
    let fountain_id = b.spawn("fountain", 32, "中央公園許願噴泉", 20.0, 20.0, 0.0, 0.0, 1.0, "landmark")?;
    for (tx, ty) in [(16.5, 16.5), (23.5, 16.5), (16.5, 23.5), (23.5, 23.5)] {
        b.spawn("sakura", 42, "公園櫻花樹", tx, ty, 0.0, 0.0, 0.85, "tree")?;
    }
    for (lx, ly) in [(11.5, 11.5), (28.5, 11.5), (11.5, 28.5), (28.5, 28.5)] {
        b.spawn("lamp", 34, "街道照明燈", lx, ly, 0.0, 0.0, 0.9, "night_light")?;
    }
    b.spawn("bench", 24, "公園休息長椅", 18.0, 22.8, 0.0, 0.0, 1.0, "prop")?;
    b.spawn("oak", 42, "社區大橡樹", 3.5, 20.0, 0.0, 0.0, 1.0, "tree")?;
    b.spawn("oak", 42, "東側綠蔭樹", 36.5, 20.0, 0.0, 0.0, 1.0, "tree")?;

    // 兩班交錯行駛的 208 號固定路線公車（每站停靠 4.5 秒，按 G 上下車，Tab 可切換親自駕駛）
    let bus1_id = b.spawn("bus", 34, "208 號通勤公車（晨光班）", 9.0, 27.5, 0.0, 180.0, 1.0, "vehicle")?;
    b.set_vehicle(
        &bus1_id,
        json!({
            "rideable": true,
            "mode": "both",
            "seat_style": "inside",
            "speed_mul": 1.65,
            "seat_z": 0.72,
            "seat_forward": 0.0
        }),
    );
    b.add_path(
        "208 號公車固定環線（晨光班）",
        &bus1_id,
        "loop",
        3.6,
        false,
        "#facc15",
        vec![
            (9.0, 27.5, 0.0, 4.5),
            (9.0, 11.5, 0.0, 4.5),
            (9.0, 9.0, 0.0, 0.2),
            (31.0, 9.0, 0.0, 0.2),
            (31.0, 11.5, 0.0, 4.5),
            (31.0, 27.5, 0.0, 4.5),
            (31.0, 31.0, 0.0, 0.2),
            (9.0, 31.0, 0.0, 0.2),
        ],
    );

    let bus2_id = b.spawn("bus", 34, "208 號通勤公車（下班直達班）", 31.0, 11.5, 0.0, 0.0, 1.0, "vehicle")?;
    b.set_tint(&bus2_id, "#93c5fd");
    b.set_vehicle(
        &bus2_id,
        json!({
            "rideable": true,
            "mode": "both",
            "seat_style": "inside",
            "speed_mul": 1.65,
            "seat_z": 0.72,
            "seat_forward": 0.0
        }),
    );
    b.add_path(
        "208 號公車固定環線（下班班次）",
        &bus2_id,
        "loop",
        3.6,
        false,
        "#38bdf8",
        vec![
            (31.0, 11.5, 0.0, 4.5),
            (31.0, 27.5, 0.0, 4.5),
            (31.0, 31.0, 0.0, 0.2),
            (9.0, 31.0, 0.0, 0.2),
            (9.0, 27.5, 0.0, 4.5),
            (9.0, 11.5, 0.0, 4.5),
            (9.0, 9.0, 0.0, 0.2),
            (31.0, 9.0, 0.0, 0.2),
        ],
    );

    // 停放在南街的共享自駕小汽車
    let share_car_id = b.spawn("car", 26, "共享微笑代步小車", 14.0, 32.8, 0.0, 270.0, 1.0, "vehicle")?;
    b.set_vehicle(
        &share_car_id,
        json!({
            "rideable": true,
            "mode": "drive",
            "seat_style": "inside",
            "speed_mul": 1.95,
            "seat_z": 0.52,
            "seat_forward": 0.0
        }),
    );

    // 街道 NPC 與街角咖啡補給
    let auntie_id = b.spawn("citizen", 40, "熱心鄰居王阿姨", 11.5, 25.0, 0.0, 180.0, 1.0, "npc")?;
    let park_coffee_id = b.spawn("coffee_bar", 34, "公園外帶咖啡車", 15.2, 20.0, 0.0, 90.0, 0.9, "prop")?;
    let commuter_id = b.spawn("citizen", 40, "通勤上班族小陳", 20.5, 17.5, 0.0, 0.0, 1.0, "npc")?;
    b.add_wander_zone("中央公園散步區", 20.0, 19.5, 3.2, 0.9, vec![commuter_id.clone()], "#34d399");

    b.add_event(
        "點擊噴泉帶來通勤好心情",
        "click",
        json!({"entity_id": fountain_id}),
        vec![
            json!({"type": "play_sound", "sound": "chime"}),
            json!({"type": "particle_burst", "effect": "sparkle", "x": 20.0, "y": 20.0, "z": 2.4, "count": 24}),
            json!({"type": "speech_bubble", "entity_id": fountain_id, "text": "✨ 祝所有通勤上班族今日事事順心！", "duration": 3.5}),
        ],
        1.5,
    );

    b.set_interaction(
        &auntie_id,
        json!({
            "kind": "clue",
            "title": "王阿姨的 208 號公車乘車指南",
            "dialogue": "小美早安呀！在站牌旁等 208 號公車停靠時按「G」上車就能自動巡航搭乘！路線順時針停靠：①住家站 → ②星辰科技總部（上班）→ ③生活大賣場 → ④療癒美食餐廳 → 回到住家站，按 Tab 還能自己開！",
            "clue_text": "【公車路線】G 上下公車自動巡航（②公司→③賣場→④餐廳→①回家），F 進出建築物大門！",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &park_coffee_id,
        json!({
            "kind": "support",
            "title": "街角外帶冰美式續命咖啡",
            "dialogue": "來杯現萃冰美式吧！這就是上班族最需要的精神護盾 +1！",
            "clue_text": "【咖啡提神】飲用冰美式後精神抖擻（護盾 +1、體力 +1）！",
            "grant_shield": 1,
            "heal_hp": 1,
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &commuter_id,
        json!({
            "kind": "quiz",
            "title": "小陳的通勤冷知識問答",
            "dialogue": "等公車順便動動腦，答對就送你通勤積分獎勵！",
            "question": "在 StackCity 中同時想「搭乘公車巡航」或「親自開走公車」時，上車後要按哪個按鍵切換模式？",
            "options": [
                "A. 按 Tab 鍵在自動巡航乘客與親自駕駛之間切換",
                "B. 把電腦螢幕關掉重開"
            ],
            "correct_index": 0,
            "reward_points": 20,
            "penalty_points": 0,
            "allow_retry": true,
            "success_text": "答對！G 鍵上下車、Tab 鍵切換巡航／自駕、F 鍵進建築大門！",
            "fail_text": "不對喔，看看畫面下方的提示按鈕：正是 Tab 鍵！",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "🚌 都市通勤大道：搭乘 208 公車往返四大據點",
        "goal_type": "custom_checklist",
        "target_count": 4,
        "culprit_entity_id": "",
        "checklist": [
            { "key": "breakfast", "label": "① 住家出門前完成早餐與拿悠遊卡" },
            { "key": "boss_task", "label": "② 搭公車到星辰科技總部完成主管工作" },
            { "key": "mall_shopping", "label": "③ 逛生活大賣場購買指定商品" },
            { "key": "dinner_eat", "label": "④ 到療癒美食餐廳吃晚餐後搭公車回家睡覺" }
        ],
        "description": "站在「住家社區公車站」旁等 208 號公車靠站按 G 搭乘巡航！在公司／賣場／餐廳站點按 G 下車、按 F 進門完成據點任務，最後再搭公車回「暖心公寓大樓」進家睡覺！",
        "victory_title": "🌟 四大都市據點行程圓滿跑完！",
        "victory_message": "現在走進西側南邊的「暖心公寓大樓」，回到溫馨住家洗個澡、滑手機並蓋被睡覺吧！"
    }));

    let meta = base_meta_pairs(
        &city.world_id,
        &city.world_name,
        author,
        "《社畜的一天》都市交通樞紐：208 號固定環線公車停靠住家、公司、賣場、餐廳四大建築入口，支援 G 上下車與 F 進門。",
        "city",
        40,
        40,
        8.25,
        "city",
        ol_asset.id,
        6.9,
        27.5,
    );
    b.flush(meta)
}

fn build_campaign_office(
    data_dir: &Path,
    author: &str,
    office: &CampaignTarget,
    city: &CampaignTarget,
    office_exit_id: &str,
    city_office_id: &str,
) -> Result<(), String> {
    let conn = open_fresh_campaign_db(data_dir, office)?;
    let mut b = WorldBuilder::new(&conn, data_dir, 24, 24, "carpet_blue");
    let ol_asset = b.ensure_asset("office_lady", 40)?;
    build_interior_walls(&mut b, "wall_glass");
    b.fill_rect(15, 2, 23, 12, 1, "tile_floor", "none");

    // 辦公座位與事務機區域
    let desk_contact_id =
        b.spawn("office_desk", 30, "小美的電腦工位（業務聯繫）", 6.2, 13.0, 0.0, 0.0, 1.0, "night_light")?;
    b.spawn("office_chair", 32, "人體工學辦公椅", 6.2, 11.2, 0.0, 0.0, 1.0, "prop")?;

    let desk_doc_id =
        b.spawn("office_desk", 30, "季度提案企劃站（製作文件）", 11.8, 13.0, 0.0, 0.0, 1.0, "night_light")?;
    b.spawn("office_chair", 32, "企劃組辦公椅", 11.8, 11.2, 0.0, 0.0, 1.0, "prop")?;

    let printer_id =
        b.spawn("printer", 34, "多功能事務印表機（打印資料）", 5.5, 5.8, 0.0, 0.0, 1.05, "prop")?;

    // 主管座位
    b.spawn("office_desk", 30, "主管王經理辦公桌", 12.0, 7.2, 0.0, 0.0, 1.05, "prop")?;
    let boss_id = b.spawn("citizen", 40, "部門主管王經理", 12.0, 5.0, 0.0, 0.0, 1.02, "npc")?;
    b.set_tint(&boss_id, "#1e3a8a");

    // 會議桌・茶水間・同事・突發急單危機
    let meeting_id =
        b.spawn("meeting_table", 22, "中央大型會議桌（專案開會）", 17.5, 16.0, 0.0, 0.0, 1.12, "prop")?;
    let coffee_id = b.spawn("coffee_bar", 34, "茶水間續命咖啡吧", 20.5, 4.8, 0.0, 0.0, 0.95, "prop")?;
    let colleague_id = b.spawn("citizen", 40, "熱心同事阿凱（揪吃午餐）", 17.2, 9.2, 0.0, 180.0, 1.0, "npc")?;
    b.set_tint(&colleague_id, "#0ea5e9");

    let urgent_id = b.spawn("citizen", 40, "急單業務大衛（突發急件）", 20.4, 11.8, 0.0, 270.0, 1.0, "robber")?;
    b.set_tint(&urgent_id, "#991b1b");

    b.spawn("bookshelf", 40, "專案檔案櫃", 8.5, 3.6, 0.0, 0.0, 0.95, "prop")?;
    b.spawn("potted_plant", 34, "辦公室幸運竹", 4.2, 18.5, 0.0, 0.0, 1.0, "plant")?;
    b.spawn("potted_plant", 34, "會議室綠植", 21.0, 18.8, 0.0, 0.0, 1.0, "plant")?;

    // 南側玻璃大門（回都市通勤大道）
    let tmp_door_id = b.spawn(
        "wooden_door",
        36,
        "公司大門（回都市大道搭公車）",
        12.0,
        21.8,
        0.0,
        180.0,
        1.0,
        "portal_exit",
    )?;
    force_entity_id(&mut b, &tmp_door_id, office_exit_id);
    b.set_portal(
        office_exit_id,
        portal_json(
            "exit",
            "下班打卡／回到「都市通勤大道」",
            city,
            6.9,
            11.5,
            270.0,
            city_office_id,
            0.65,
            "",
            "",
        ),
    );

    b.add_event(
        "點擊會議桌啟動腦力激盪",
        "click",
        json!({"entity_id": meeting_id}),
        vec![
            json!({"type": "play_sound", "sound": "chime"}),
            json!({"type": "particle_burst", "effect": "sparkle", "x": 17.5, "y": 16.0, "z": 2.0, "count": 20}),
        ],
        1.5,
    );

    // 任務 1：業務聯繫
    b.set_interaction(
        &desk_contact_id,
        json!({
            "kind": "clue",
            "title": "09:15 業務聯繫客戶＆確認報價需求",
            "dialogue": "「您好這裡是星辰科技小美！」電話與 Email 確認完客戶大單的所有規格，印出「客戶需求單」，快去西北角「多功能事務印表機」列印會議簡報資料吧！",
            "clue_text": "【業務聯繫完成】取得「客戶需求單」，可前往印表機列印資料與主管報告。",
            "reward_points": 20,
            "reward_item": "客戶需求單",
            "reward_flag": "biz_contact",
            "once": true
        }),
    );
    // 任務 2：打印資料（需先完成業務聯繫拿需求單）
    b.set_interaction(
        &printer_id,
        json!({
            "kind": "treasure",
            "title": "事務機打印簡報資料與會議講義",
            "dialogue": "嗡嗡嗡——雙面彩色列印全速運轉中！準備輸出本週業務報告投影片與客戶需求附件！",
            "required_item": "客戶需求單",
            "locked_text": "印表機還沒收到資料喔！請先到「小美的電腦工位」做業務聯繫拿到「客戶需求單」再來列印！",
            "success_text": "熱騰騰的「列印簡報資料」裝訂完成！（連同別忘去中間工位「製作季度提案文件」與開會喔！）",
            "reward_points": 25,
            "reward_item": "列印簡報資料",
            "reward_flag": "print_docs",
            "once": true
        }),
    );
    // 任務 3：製作文件
    b.set_interaction(
        &desk_doc_id,
        json!({
            "kind": "quiz",
            "title": "製作季度業務提案文件",
            "dialogue": "打開簡報與試算表軟體開始整理季度文件！哪一項原則最能讓主管和客戶一眼看懂重點？",
            "question": "製作對外業務提案文件時，最專業且高效率的排版方式是？",
            "options": [
                "A. 先講結論與關鍵數據圖表，輔以清晰項目條列",
                "B. 塞滿整頁 8 號字純文字不分段也不放圖表"
            ],
            "correct_index": 0,
            "reward_points": 30,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "季度提案文件",
            "reward_flag": "make_docs",
            "success_text": "神速完成一份專業又吸睛的「季度提案文件」！",
            "fail_text": "這樣主管眼睛會花掉喔！再選一次精簡清晰的簡報原則！",
            "once": true
        }),
    );
    // 任務 4：開會與業務討論
    b.set_interaction(
        &meeting_id,
        json!({
            "kind": "quiz",
            "title": "10:30 專案晨會與跨部門同步",
            "dialogue": "各組同仁齊聚大型會議桌！工程部詢問上線時程風險時，小美該如何協調？",
            "question": "專案開會發現某個功能趕工風險過高時，最專業的開會決策是？",
            "options": [
                "A. 與工程、設計共同確認優先順序（MVP 優先交付），並當場發送會議紀錄對齊",
                "B. 假裝沒聽到問題直接散會"
            ],
            "correct_index": 0,
            "reward_points": 35,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "會議紀錄簽核單",
            "reward_flag": "meeting_report",
            "success_text": "全場鼓掌！跨部門會議圓滿達成共識！",
            "fail_text": "逃避問題可不行喔，試試看積極對齊優先順序與會議紀錄！",
            "once": true
        }),
    );
    // 任務 5：和同事一起午餐
    b.set_interaction(
        &colleague_id,
        json!({
            "kind": "support",
            "title": "12:15 和同事阿凱一起午餐＆聊八卦",
            "dialogue": "小美走吧，一起吃椒麻雞腿便當配珍珠鮮奶茶！順便跟你說一個抗壓小秘訣：下午萬一大衛衝過來塞臨時急單，直接用護盾或時間管理拒絕無效加班！",
            "clue_text": "【午餐充電】和同事一起午餐好療癒，體力 +2、護盾 +1！",
            "grant_shield": 1,
            "heal_hp": 2,
            "reward_points": 25,
            "reward_flag": "colleague_lunch",
            "once": true
        }),
    );
    // 任務 6：業務報告＆執行主管任務（須印好簡報資料）
    b.set_interaction(
        &boss_id,
        json!({
            "kind": "treasure",
            "title": "16:30 業務進度報告＆執行主管任務",
            "dialogue": "王經理：「小美，本週的業務報告資料與主管交辦核對表準備好了嗎？」",
            "required_item": "列印簡報資料",
            "locked_text": "小美，請先去西北角的「多功能事務印表機」把簡報資料打印出來，再來做業務報告並領取準時下班憑證喔！",
            "success_text": "王經理大讚：「做得太好了！資料詳盡又有執行力，今天準時 18:00 打卡下班！」（記得下班後搭 208 公車去賣場購物和餐廳吃晚餐喔！）",
            "reward_points": 45,
            "reward_item": "準時下班打卡證",
            "reward_flag": "boss_task",
            "once": true
        }),
    );
    // 細節 1：茶水間續命咖啡吧
    b.set_interaction(
        &coffee_id,
        json!({
            "kind": "support",
            "title": "茶水間手沖燕麥拿鐵補給",
            "dialogue": "下午三點昏昏欲睡嗎？來一杯免費的茶水間精品燕麥拿鐵回血吧！",
            "clue_text": "【續命咖啡】能量滿滿，精神護盾 +1、體力 +1！",
            "grant_shield": 1,
            "heal_hp": 1,
            "reward_points": 15,
            "once": true
        }),
    );
    // 細節 2：五點半臨時急件突襲挑戰
    b.set_interaction(
        &urgent_id,
        json!({
            "kind": "robbery",
            "title": "⚠️ 17:30 急單業務大衛突然攔路插單！",
            "dialogue": "小美救命呀！我這份臨時客戶大修需求一定要你馬上停下手邊工作重做，害你準時下班泡湯啦！",
            "auto_trigger_radius": 1.8,
            "demand_points": 15,
            "hp_damage": 1,
            "allow_shield_counter": true,
            "hide_on_defeat": false,
            "question": "面對下班前突發急件插單，小美該如何展現社畜高情商化解加班危機？",
            "options": [
                "A. 條理拆解範疇，先提供現有標準方案並將客製急件排入明日第一順位",
                "B. 默默全部吞下來獨自加班到半夜兩點"
            ],
            "correct_index": 0,
            "reward_points": 35,
            "reward_item": "反加班救星徽章",
            "success_text": "漂亮反制！大衛佩服地豎起大拇指，小美成功守住準時下班防線！",
            "fail_text": "嗚嗚差點被迫留下加班，先去找「同事阿凱」吃午餐或「咖啡吧」補滿護盾再應戰！",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "🏢 公司辦公室：社畜高效工作日（6 大工作目標）",
        "goal_type": "custom_checklist",
        "target_count": 6,
        "culprit_entity_id": "",
        "checklist": [
            { "key": "biz_contact", "label": "小美電腦工位：業務聯繫客戶" },
            { "key": "print_docs", "label": "多功能印表機：打印簡報資料" },
            { "key": "make_docs", "label": "企劃電腦站：製作提案文件" },
            { "key": "meeting_report", "label": "中央會議桌：開會討論對齊" },
            { "key": "colleague_lunch", "label": "同事阿凱：一起午餐充電" },
            { "key": "boss_task", "label": "王經理辦公桌：業務報告與主管任務" }
        ],
        "description": "完成右上方 6 項辦公室工作任務後，就可以從南邊玻璃大門出去搭 208 號公車前往「生活大賣場」與「美食餐廳」放鬆啦！",
        "victory_title": "🎉 打卡下班！今日辦公室任務全數制霸！",
        "victory_message": "業務聯繫、印表機出紙、開會、同事午餐到主管任務全數搞定！快走出公司大門，散步或搭 208 公車去賣場購物和吃晚餐吧！"
    }));

    let meta = base_meta_pairs(
        &office.world_id,
        &office.world_name,
        author,
        "《社畜的一天》公司辦公室：電腦工位業務聯繫、印表機印資料、製作文件、開會、同事午餐、主管報告與茶水間咖啡。",
        "office_interior",
        24,
        24,
        9.0,
        "indoor",
        ol_asset.id,
        12.0,
        19.8,
    );
    b.flush(meta)
}

fn build_campaign_mall(
    data_dir: &Path,
    author: &str,
    mall: &CampaignTarget,
    city: &CampaignTarget,
    mall_exit_id: &str,
    city_mall_id: &str,
) -> Result<(), String> {
    let conn = open_fresh_campaign_db(data_dir, mall)?;
    let mut b = WorldBuilder::new(&conn, data_dir, 24, 24, "tile_floor");
    let ol_asset = b.ensure_asset("office_lady", 40)?;
    build_interior_walls(&mut b, "wall_cream");

    b.spawn("shelf", 34, "日常家居商品架", 5.8, 6.2, 0.0, 0.0, 1.0, "prop")?;
    let snack_shelf_id =
        b.spawn("shelf", 34, "鮮奶與零食特價貨架", 11.8, 6.2, 0.0, 0.0, 1.05, "prop")?;
    let aroma_case_id =
        b.spawn("display_case", 32, "舒壓香氛生活展示櫃", 17.5, 6.5, 0.0, 0.0, 1.05, "night_light")?;
    b.spawn("display_case", 32, "烘焙點心冷藏櫃", 17.5, 12.2, 0.0, 0.0, 1.0, "prop")?;
    b.spawn("shelf", 34, "文具小物貨架", 5.8, 12.2, 0.0, 0.0, 1.0, "prop")?;
    b.spawn("vending", 34, "發光飲料販賣機", 20.8, 4.8, 0.0, 0.0, 1.0, "night_light")?;
    let counter_id = b.spawn("counter", 34, "賣場收銀結帳台", 15.5, 17.0, 0.0, 0.0, 1.08, "night_light")?;

    let clerk_id = b.spawn("citizen", 40, "賣場店員小晴", 9.2, 15.8, 0.0, 0.0, 1.0, "npc")?;
    let shopper_id = b.spawn("citizen", 40, "採買鄰居小姐", 10.5, 10.5, 0.0, 90.0, 1.0, "npc")?;
    b.add_wander_zone("賣場購物走道", 11.0, 11.0, 2.8, 0.85, vec![shopper_id], "#38bdf8");

    let tmp_door_id = b.spawn(
        "wooden_door",
        36,
        "賣場出口大門（回都市大道）",
        12.0,
        21.8,
        0.0,
        180.0,
        1.0,
        "portal_exit",
    )?;
    force_entity_id(&mut b, &tmp_door_id, mall_exit_id);
    b.set_portal(
        mall_exit_id,
        portal_json(
            "exit",
            "提著戰利品走出賣場（回都市大道）",
            city,
            33.1,
            11.5,
            90.0,
            city_mall_id,
            0.65,
            "",
            "",
        ),
    );

    b.set_interaction(
        &clerk_id,
        json!({
            "kind": "clue",
            "title": "下班採購指定商品清單",
            "dialogue": "歡迎光臨生活大賣場！小美今天指定購買的兩樣舒壓好物是：「鮮奶與零食」在北側中央貨架，以及「薰衣草香氛」在東北側玻璃展示櫃，選完記得來「賣場收銀結帳台」結帳喔！",
            "clue_text": "【賣場清單】拿「鮮奶與零食特價貨架」＋「舒壓香氛展示櫃」兩樣指定商品後至收銀台結帳！",
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &snack_shelf_id,
        json!({
            "kind": "clue",
            "title": "挑選指定商品：鮮奶與追劇洋芋片",
            "dialogue": "把冰涼低脂鮮奶跟限時買一送一的海鹽洋芋片放進購物籃！今晚沙發追劇有著落了！",
            "clue_text": "【放入購物籃】已取得指定商品①：鮮奶與零食。",
            "reward_points": 20,
            "reward_item": "鮮奶與零食",
            "reward_flag": "mall_pick_snack",
            "once": true
        }),
    );
    b.set_interaction(
        &aroma_case_id,
        json!({
            "kind": "treasure",
            "title": "挑選指定商品：薰衣草舒壓擴香",
            "dialogue": "店員推薦超助眠的森林薰衣草精油香氛瓶，一聞瞬間忘記工作壓力！",
            "required_item": "",
            "success_text": "收進購物籃！已取得指定商品②「薰衣草香氛」，快去右下角收銀台結帳吧！",
            "reward_points": 20,
            "reward_item": "薰衣草香氛",
            "reward_flag": "mall_pick_aroma",
            "once": true
        }),
    );
    b.set_interaction(
        &counter_id,
        json!({
            "kind": "quiz",
            "title": "下班生活賣場會員結帳",
            "dialogue": "鮮奶零食組 120 元 ＋ 薰衣草香氛 180 元 ＝ 原價 300 元。使用小美的上班族會員 9 折優惠券！",
            "question": "300 元商品打 9 折後，在收銀台實際只需要支付多少元？",
            "options": [
                "A. 270 元（現省 30 元超划算！）",
                "B. 210 元",
                "C. 330 元"
            ],
            "correct_index": 0,
            "reward_points": 35,
            "penalty_points": 5,
            "allow_retry": true,
            "reward_item": "賣場戰利品",
            "reward_flag": "mall_shopping",
            "success_text": "結帳完成！提著滿滿「賣場戰利品」，接下來可以往南散步或搭公車到「療癒美食餐廳」吃晚餐囉！",
            "fail_text": "再算一下喔：300 × 0.9 = 270 元！",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "🛒 生活大賣場：下班指定商品採買清單",
        "goal_type": "custom_checklist",
        "target_count": 3,
        "culprit_entity_id": "",
        "checklist": [
            { "key": "mall_pick_snack", "label": "貨架拿取指定「鮮奶與零食」" },
            { "key": "mall_pick_aroma", "label": "展示櫃選購「薰衣草香氛」" },
            { "key": "mall_shopping", "label": "在「賣場收銀結帳台」完成結帳" }
        ],
        "description": "照著清單挑選鮮奶零食、香氛並到收銀台結帳！買完後即可離開賣場前往美食餐廳吃晚餐！",
        "victory_title": "🛍️ 買齊指定戰利品！滿載而歸！",
        "victory_message": "鮮奶、零食和舒壓香氛通通入手，該去美食餐廳好好慰勞自己一頓豐盛晚餐囉！"
    }));

    let meta = base_meta_pairs(
        &mall.world_id,
        &mall.world_name,
        author,
        "《社畜的一天》生活大賣場：選購指定鮮奶零食與舒壓香氛，於收銀台使用會員折扣完成下班採買。",
        "shop_interior",
        24,
        24,
        18.25,
        "indoor",
        ol_asset.id,
        12.0,
        19.8,
    );
    b.flush(meta)
}

fn build_campaign_restaurant(
    data_dir: &Path,
    author: &str,
    rest: &CampaignTarget,
    city: &CampaignTarget,
    rest_exit_id: &str,
    city_rest_id: &str,
) -> Result<(), String> {
    let conn = open_fresh_campaign_db(data_dir, rest)?;
    let mut b = WorldBuilder::new(&conn, data_dir, 24, 24, "wood_floor");
    let ol_asset = b.ensure_asset("office_lady", 40)?;
    build_interior_walls(&mut b, "wall_cream");

    // 開放式廚房地磚區 ＋ 暖紅用餐地毯區
    b.fill_rect(2, 2, 22, 8, 1, "tile_floor", "none");
    b.fill_rect(4, 10, 21, 19, 1, "carpet_red", "none");

    b.spawn("kitchen_stove", 28, "主廚熱炒料理台", 6.5, 4.2, 0.0, 0.0, 1.05, "night_light")?;
    b.spawn("kitchen_stove", 28, "燉湯吧台區", 12.0, 4.2, 0.0, 0.0, 1.0, "prop")?;
    let order_counter_id =
        b.spawn("counter", 34, "餐廳點餐櫃檯", 18.2, 6.2, 0.0, 0.0, 1.08, "night_light")?;
    let chef_id = b.spawn("citizen", 40, "暖心主廚阿宏", 14.8, 6.2, 0.0, 0.0, 1.0, "npc")?;
    b.set_tint(&chef_id, "#f8fafc");

    let my_table_id =
        b.spawn("dining_table", 22, "窗邊療癒晚餐桌（小美專屬）", 7.2, 14.5, 0.0, 0.0, 1.08, "night_light")?;
    b.spawn("dining_table", 22, "二人浪漫餐桌", 13.5, 14.5, 0.0, 0.0, 1.02, "prop")?;
    b.spawn("dining_table", 22, "好友聚餐圓桌", 19.0, 14.5, 0.0, 0.0, 1.02, "prop")?;
    b.spawn("potted_plant", 34, "餐廳植栽造景", 4.2, 18.2, 0.0, 0.0, 1.0, "plant")?;
    b.spawn("potted_plant", 34, "門口迎賓盆栽", 20.5, 18.2, 0.0, 0.0, 1.0, "plant")?;

    let waiter_id = b.spawn("citizen", 40, "餐廳服務生小宇", 10.5, 11.2, 0.0, 180.0, 1.0, "npc")?;
    b.add_wander_zone("餐廳桌邊送餐區", 12.5, 12.0, 3.0, 0.9, vec![waiter_id.clone()], "#fbbf24");

    let tmp_door_id = b.spawn(
        "wooden_door",
        36,
        "餐廳大門（搭 208 公車回家）",
        12.0,
        21.8,
        0.0,
        180.0,
        1.0,
        "portal_exit",
    )?;
    force_entity_id(&mut b, &tmp_door_id, rest_exit_id);
    b.set_portal(
        rest_exit_id,
        portal_json(
            "exit",
            "吃飽離開餐廳／前往搭公車回家",
            city,
            33.1,
            27.5,
            90.0,
            city_rest_id,
            0.65,
            "",
            "",
        ),
    );

    b.add_event(
        "點擊專屬晚餐桌亮燈氛圍",
        "click",
        json!({"entity_id": my_table_id}),
        vec![
            json!({"type": "play_sound", "sound": "chime"}),
            json!({"type": "particle_burst", "effect": "sparkle", "x": 7.2, "y": 14.5, "z": 1.8, "count": 18}),
        ],
        1.5,
    );

    b.set_interaction(
        &order_counter_id,
        json!({
            "kind": "quiz",
            "title": "19:30 看菜單點餐・今天吃什麼犒賞自己？",
            "dialogue": "服務生遞上熱茶與菜單！「今天工作這麼辛苦，推薦我們的招牌療癒晚餐！」",
            "question": "點哪一份招牌暖胃組合能讓辛苦一整天的小美補充滿滿元氣？",
            "options": [
                "A. 香煎嫩雞排暖心套餐（附現烤麵包、鮮蔬沙拉與熱玉米濃湯）",
                "B. 只喝一杯自來水就走"
            ],
            "correct_index": 0,
            "reward_points": 25,
            "penalty_points": 0,
            "allow_retry": true,
            "reward_item": "點餐號碼牌",
            "reward_flag": "dinner_order",
            "success_text": "點餐成功！拿到「點餐號碼牌」囉，快坐到西側「窗邊療癒晚餐桌」等待上菜吧！",
            "fail_text": "忙了一整天可別虧待自己，選招牌套餐犒賞小美吧！",
            "once": true
        }),
    );
    b.set_interaction(
        &chef_id,
        json!({
            "kind": "support",
            "title": "主廚阿宏的招待手作奶酪",
            "dialogue": "看你剛下班還提著賣場購物袋，主廚招待你一份黑糖鮮奶酪配熱濃湯！（體力 +2、護盾 +1）",
            "clue_text": "【主廚招待】暖胃濃湯與奶酪入肚，上班疲勞消散大半！",
            "grant_shield": 1,
            "heal_hp": 2,
            "reward_points": 15,
            "once": true
        }),
    );
    b.set_interaction(
        &my_table_id,
        json!({
            "kind": "treasure",
            "title": "窗邊燭光享用美食晚餐",
            "dialogue": "熱呼呼的香煎雞排配濃湯上桌啦，看著窗外都市夜景慢慢品嚐！",
            "required_item": "點餐號碼牌",
            "locked_text": "還沒點餐呢！請先前往東北角「餐廳點餐櫃檯」完成點餐拿到「點餐號碼牌」再來入座享用喔！",
            "success_text": "大滿足！「美食晚餐收據」已收好，現在走出餐廳搭 208 號公車回到「暖心公寓大樓」進溫馨住家洗澡、滑手機、安心入睡吧！",
            "reward_points": 40,
            "reward_item": "美食晚餐收據",
            "reward_flag": "dinner_eat",
            "once": true
        }),
    );

    b.set_quest(json!({
        "enabled": true,
        "title": "🍽️ 療癒美食餐廳：點餐與暖心晚餐時光",
        "goal_type": "custom_checklist",
        "target_count": 2,
        "culprit_entity_id": "",
        "checklist": [
            { "key": "dinner_order", "label": "在「餐廳點餐櫃檯」看菜單點餐" },
            { "key": "dinner_eat", "label": "在「窗邊療癒晚餐桌」享用美食晚餐" }
        ],
        "description": "向櫃檯點餐並入座享用餐點拿回「美食晚餐收據」，接著出餐廳門按 G 搭公車回家完成晚間盥洗・滑手機・睡覺收官！",
        "victory_title": "😋 飽足又療癒的一餐！該搭公車回家囉！",
        "victory_message": "出大門前往 208 號公車回家，進入「暖心公寓大樓」完成最後的盥洗、沙發滑手機與蓋被睡覺吧！"
    }));

    let meta = base_meta_pairs(
        &rest.world_id,
        &rest.world_name,
        author,
        "《社畜的一天》療癒美食餐廳：下班看菜單點餐、主廚招待甜點、窗邊享用美味晚餐，滿足踏上歸途。",
        "home_interior",
        24,
        24,
        19.5,
        "indoor",
        ol_asset.id,
        12.0,
        19.8,
    );
    b.flush(meta)
}

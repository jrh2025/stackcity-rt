//! 純 Rust 6 大類 42 種 Sprite-Stacking 物件工廠與中英文語意解析器。
//!
//! 每種物件均先生成標準結構化 `Blueprint` JSON，再統一交由 `blueprint::render_blueprint_canvas`
//! 與 `VoxelCanvas::bake` 處理 3D AO、材質肌理、防閃爍補厚與法線/自發光貼圖，
//! 讓玩家在遊戲內可即時將任何生成物件載入藍圖積木編輯器二次修改！

use super::blueprint::render_blueprint_canvas;
use super::BakedStack;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 預設造物分類資訊（供前端圖庫與工坊瀏覽）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetCatalogItem {
    pub key: String,
    pub name: String,
    pub category: String,
    pub default_scale: f32,
    pub anim_preset: String,
    pub prompt_hint: String,
}

/// 解析後的使用者創作規格
#[derive(Debug, Clone)]
pub struct ParsedPrompt {
    pub key: String,
    pub title: String,
    pub category: String,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub snow: bool,
    pub moss: bool,
    pub glow: bool,
    pub default_scale: f32,
    pub anim_preset: String,
}

struct PresetDef {
    key: &'static str,
    name: &'static str,
    category: &'static str,
    default_scale: f32,
    anim_preset: &'static str,
    keywords: &'static [&'static str],
}

const PRESETS: &[PresetDef] = &[
    // --- 建築與地標 (building) ---
    PresetDef {
        key: "castle",
        name: "中世紀城堡",
        category: "building",
        default_scale: 1.45,
        anim_preset: "static",
        keywords: &["城堡", "堡壘", "要塞", "皇宮", "castle", "fortress", "palace"],
    },
    PresetDef {
        key: "clock_tower",
        name: "黃金鐘樓",
        category: "building",
        default_scale: 1.2,
        anim_preset: "static",
        keywords: &["鐘樓", "大笨鐘", "鐘塔", "clock tower", "big ben"],
    },
    PresetDef {
        key: "cottage",
        name: "溫馨木屋",
        category: "building",
        default_scale: 1.15,
        anim_preset: "static",
        keywords: &["木屋", "小屋", "房子", "房屋", "農舍", "別墅", "cottage", "cabin", "house"],
    },
    PresetDef {
        key: "skyscraper",
        name: "霓虹摩天樓",
        category: "building",
        default_scale: 1.35,
        anim_preset: "static",
        keywords: &["摩天樓", "大樓", "大廈", "台北101", "101", "skyscraper", "tower", "office"],
    },
    PresetDef {
        key: "shop",
        name: "街角霓虹小店",
        category: "building",
        default_scale: 1.15,
        anim_preset: "static",
        keywords: &["小店", "拉麵", "咖啡", "商店", "便利店", "攤位", "shop", "cafe", "ramen"],
    },
    PresetDef {
        key: "coffee_bar",
        name: "精品咖啡茶水吧",
        category: "prop",
        default_scale: 0.88,
        anim_preset: "static",
        keywords: &["咖啡吧", "茶水吧", "咖啡機", "義式咖啡", "coffee bar", "espresso"],
    },
    PresetDef {
        key: "barn",
        name: "紅頂穀倉",
        category: "building",
        default_scale: 1.25,
        anim_preset: "static",
        keywords: &["穀倉", "倉庫", "牛棚", "barn"],
    },
    PresetDef {
        key: "windmill",
        name: "農莊風車",
        category: "building",
        default_scale: 1.25,
        anim_preset: "static",
        keywords: &["風車", "windmill"],
    },
    PresetDef {
        key: "school",
        name: "校園教學樓",
        category: "building",
        default_scale: 1.45,
        anim_preset: "static",
        keywords: &["學校", "校園", "教學樓", "教室", "校舍", "school", "campus"],
    },
    PresetDef {
        key: "lighthouse",
        name: "海邊燈塔",
        category: "building",
        default_scale: 1.2,
        anim_preset: "static",
        keywords: &["燈塔", "lighthouse"],
    },
    PresetDef {
        key: "torii",
        name: "神社大鳥居",
        category: "building",
        default_scale: 1.1,
        anim_preset: "static",
        keywords: &["鳥居", "神社", "torii"],
    },
    PresetDef {
        key: "fountain",
        name: "許願噴泉",
        category: "building",
        default_scale: 1.05,
        anim_preset: "bob",
        keywords: &["噴泉", "水池", "許願池", "fountain"],
    },
    PresetDef {
        key: "pagoda",
        name: "五重寶塔",
        category: "building",
        default_scale: 1.25,
        anim_preset: "static",
        keywords: &["五重塔", "寶塔", "佛塔", "pagoda"],
    },
    // --- 人物角色 (character) ---
    PresetDef {
        key: "knight",
        name: "巡邏騎士",
        category: "character",
        default_scale: 0.72,
        anim_preset: "walk",
        keywords: &["騎士", "守衛", "衛兵", "勇者", "聖騎士", "knight", "guard"],
    },
    PresetDef {
        key: "mage",
        name: "星辰法師",
        category: "character",
        default_scale: 0.72,
        anim_preset: "walk",
        keywords: &["法師", "巫師", "魔法師", "魔導士", "mage", "wizard", "witch"],
    },
    PresetDef {
        key: "student",
        name: "青春學生",
        category: "character",
        default_scale: 0.68,
        anim_preset: "walk",
        keywords: &["學生", "同學", "少年", "少女", "student", "pupil"],
    },
    PresetDef {
        key: "farmer",
        name: "草帽農夫",
        category: "character",
        default_scale: 0.7,
        anim_preset: "walk",
        keywords: &["農夫", "農民", "園丁", "farmer"],
    },
    PresetDef {
        key: "office_lady",
        name: "粉領女上班族",
        category: "character",
        default_scale: 0.70,
        anim_preset: "walk",
        keywords: &["女上班族", "上班族", "粉領族", "OL", "office lady", "office worker"],
    },
    PresetDef {
        key: "citizen",
        name: "都市市民",
        category: "character",
        default_scale: 0.68,
        anim_preset: "walk",
        keywords: &["市民", "路人", "行人", "旅人", "人物", "人偶", "citizen", "person", "villager"],
    },
    PresetDef {
        key: "astronaut",
        name: "星際太空人",
        category: "character",
        default_scale: 0.74,
        anim_preset: "walk",
        keywords: &["太空人", "宇航員", "astronaut"],
    },
    // --- 動物生態 (animal) ---
    PresetDef {
        key: "lion",
        name: "草原公獅",
        category: "animal",
        default_scale: 0.78,
        anim_preset: "walk",
        keywords: &["獅子", "公獅", "lion"],
    },
    PresetDef {
        key: "shiba",
        name: "可愛柴犬",
        category: "animal",
        default_scale: 0.62,
        anim_preset: "walk",
        keywords: &["柴犬", "小狗", "狗", "犬", "shiba", "dog", "puppy"],
    },
    PresetDef {
        key: "sheep",
        name: "牧場綿羊",
        category: "animal",
        default_scale: 0.65,
        anim_preset: "walk",
        keywords: &["綿羊", "小羊", "羊", "sheep", "lamb"],
    },
    PresetDef {
        key: "carriage",
        name: "皇家馬車",
        category: "vehicle",
        default_scale: 1.05,
        anim_preset: "bob",
        keywords: &["馬車", "皇家馬車", "篷車", "雙馬車", "carriage", "wagon", "coach"],
    },
    PresetDef {
        key: "horse",
        name: "草原駿馬",
        category: "animal",
        default_scale: 0.85,
        anim_preset: "walk",
        keywords: &["駿馬", "白馬", "馬", "戰馬", "horse"],
    },
    PresetDef {
        key: "duck",
        name: "池塘白鴨",
        category: "animal",
        default_scale: 0.52,
        anim_preset: "bob",
        keywords: &["鴨子", "白鴨", "小鴨", "天鵝", "duck", "swan"],
    },
    PresetDef {
        key: "bird",
        name: "展翅青鳥",
        category: "animal",
        default_scale: 0.55,
        anim_preset: "float",
        keywords: &["飛鳥", "青鳥", "小鳥", "鳥", "海鷗", "貓頭鷹", "bird", "owl"],
    },
    // --- 植物景觀 (plant) ---
    PresetDef {
        key: "sakura",
        name: "粉紅櫻花樹",
        category: "plant",
        default_scale: 1.05,
        anim_preset: "sway",
        keywords: &["櫻花", "櫻花樹", "sakura", "cherry"],
    },
    PresetDef {
        key: "pine",
        name: "蒼翠松樹",
        category: "plant",
        default_scale: 1.05,
        anim_preset: "sway",
        keywords: &["松樹", "聖誕樹", "杉樹", "針葉樹", "pine", "spruce"],
    },
    PresetDef {
        key: "oak",
        name: "大橡樹",
        category: "plant",
        default_scale: 1.05,
        anim_preset: "sway",
        keywords: &["橡樹", "大樹", "綠樹", "樹", "oak", "tree"],
    },
    PresetDef {
        key: "sunflower",
        name: "向日葵花圃",
        category: "plant",
        default_scale: 0.72,
        anim_preset: "sway",
        keywords: &["向日葵", "花圃", "花朵", "花", "sunflower", "flower"],
    },
    PresetDef {
        key: "wheat",
        name: "金色稻麥叢",
        category: "plant",
        default_scale: 0.82,
        anim_preset: "sway",
        keywords: &["稻田", "麥田", "稻麥", "小麥", "農作物", "wheat", "crop"],
    },
    PresetDef {
        key: "mushroom",
        name: "奇幻發光巨菇",
        category: "plant",
        default_scale: 0.78,
        anim_preset: "sway",
        keywords: &["蘑菇", "香菇", "巨菇", "菇", "mushroom"],
    },
    // --- 交通載具 (vehicle) ---
    PresetDef {
        key: "car",
        name: "復古跑車",
        category: "vehicle",
        default_scale: 0.82,
        anim_preset: "bob",
        keywords: &["跑車", "汽車", "轎車", "車子", "賽車", "car", "sports car"],
    },
    PresetDef {
        key: "bus_stop",
        name: "公車站牌候車亭",
        category: "prop",
        default_scale: 0.92,
        anim_preset: "static",
        keywords: &["公車站", "候車亭", "站牌", "巴士站", "bus stop"],
    },
    PresetDef {
        key: "bus",
        name: "城市巴士",
        category: "vehicle",
        default_scale: 0.95,
        anim_preset: "bob",
        keywords: &["巴士", "公車", "校車", "卡車", "貨車", "bus", "truck"],
    },
    PresetDef {
        key: "train",
        name: "蒸汽火車頭",
        category: "vehicle",
        default_scale: 0.98,
        anim_preset: "bob",
        keywords: &["火車", "蒸汽火車", "列車", "電車", "train", "locomotive"],
    },
    PresetDef {
        key: "sailboat",
        name: "航海帆船",
        category: "vehicle",
        default_scale: 1.0,
        anim_preset: "bob",
        keywords: &["帆船", "木船", "遊船", "海盜船", "船", "sailboat", "boat", "ship"],
    },
    PresetDef {
        key: "balloon",
        name: "彩虹熱氣球",
        category: "vehicle",
        default_scale: 1.15,
        anim_preset: "float",
        keywords: &["熱氣球", "氣球", "飛艇", "balloon", "airship"],
    },
    PresetDef {
        key: "spaceship",
        name: "星際飛船",
        category: "vehicle",
        default_scale: 0.95,
        anim_preset: "float",
        keywords: &["飛船", "太空船", "星艦", "火箭", "spaceship", "rocket"],
    },
    // --- 道具與機關 (prop) ---
    PresetDef {
        key: "lamp",
        name: "復古街燈",
        category: "prop",
        default_scale: 0.72,
        anim_preset: "static",
        keywords: &["路燈", "街燈", "燈柱", "lamp", "streetlight"],
    },
    PresetDef {
        key: "campfire",
        name: "溫暖營火堆",
        category: "prop",
        default_scale: 0.68,
        anim_preset: "static",
        keywords: &["營火", "篝火", "火堆", "campfire", "bonfire"],
    },
    PresetDef {
        key: "gate",
        name: "平交道柵欄",
        category: "prop",
        default_scale: 0.85,
        anim_preset: "static",
        keywords: &["柵欄", "平交道", "路障", "欄杆", "gate", "barrier"],
    },
    PresetDef {
        key: "crystal",
        name: "魔法發光水晶",
        category: "prop",
        default_scale: 0.78,
        anim_preset: "float",
        keywords: &["水晶", "魔晶", "寶石", "crystal", "gem"],
    },
    PresetDef {
        key: "bench",
        name: "公園長椅",
        category: "prop",
        default_scale: 0.65,
        anim_preset: "static",
        keywords: &["長椅", "椅子", "座椅", "bench"],
    },
    PresetDef {
        key: "chest",
        name: "黃金寶箱",
        category: "prop",
        default_scale: 0.62,
        anim_preset: "static",
        keywords: &["寶箱", "箱子", "chest"],
    },
    // --- 宮殿與華麗室內 ---
    PresetDef {
        key: "throne",
        name: "黃金王座",
        category: "prop",
        default_scale: 0.85,
        anim_preset: "static",
        keywords: &["王座", "寶座", "龍椅", "throne"],
    },
    PresetDef {
        key: "chandelier",
        name: "水晶吊燈燭台",
        category: "prop",
        default_scale: 0.85,
        anim_preset: "bob",
        keywords: &["吊燈", "水晶燈", "燭台", "chandelier"],
    },
    PresetDef {
        key: "pillar",
        name: "大理石柱",
        category: "building",
        default_scale: 0.95,
        anim_preset: "static",
        keywords: &["石柱", "羅馬柱", "大理石柱", "廊柱", "pillar", "column"],
    },
    PresetDef {
        key: "statue",
        name: "騎士石雕像",
        category: "prop",
        default_scale: 0.85,
        anim_preset: "static",
        keywords: &["雕像", "石像", "塑像", "statue"],
    },
    // --- 商店與零售 ---
    PresetDef {
        key: "counter",
        name: "收銀櫃檯",
        category: "prop",
        default_scale: 0.9,
        anim_preset: "static",
        keywords: &["櫃檯", "收銀台", "服務台", "counter"],
    },
    PresetDef {
        key: "shelf",
        name: "商品陳列貨架",
        category: "prop",
        default_scale: 0.9,
        anim_preset: "static",
        keywords: &["貨架", "商品架", "架子", "shelf"],
    },
    PresetDef {
        key: "vending",
        name: "發光自動販賣機",
        category: "prop",
        default_scale: 0.85,
        anim_preset: "static",
        keywords: &["販賣機", "飲料機", "自動販賣機", "vending"],
    },
    PresetDef {
        key: "display_case",
        name: "甜點展示櫃",
        category: "prop",
        default_scale: 0.85,
        anim_preset: "static",
        keywords: &["展示櫃", "甜點櫃", "玻璃櫃", "display case"],
    },
    // --- 工廠與機械 ---
    PresetDef {
        key: "conveyor",
        name: "工業輸送帶",
        category: "prop",
        default_scale: 0.95,
        anim_preset: "static",
        keywords: &["輸送帶", "流水線", "傳送帶", "conveyor"],
    },
    PresetDef {
        key: "robot_arm",
        name: "自動機械手臂",
        category: "prop",
        default_scale: 0.9,
        anim_preset: "sway",
        keywords: &["機械手臂", "機械臂", "機器臂", "robot arm"],
    },
    PresetDef {
        key: "furnace",
        name: "動力高爐機台",
        category: "prop",
        default_scale: 1.0,
        anim_preset: "bob",
        keywords: &["熔爐", "高爐", "機台", "動力爐", "鍋爐", "furnace", "machine"],
    },
    PresetDef {
        key: "crate",
        name: "木頭貨物箱堆",
        category: "prop",
        default_scale: 0.72,
        anim_preset: "static",
        keywords: &["貨箱", "木箱", "貨物", "crate", "crates"],
    },
    PresetDef {
        key: "forklift",
        name: "工廠堆高機",
        category: "vehicle",
        default_scale: 0.85,
        anim_preset: "bob",
        keywords: &["堆高機", "叉車", "鏟車", "forklift"],
    },
    // --- 辦公室與教室 ---
    PresetDef {
        key: "office_desk",
        name: "電腦辦公桌",
        category: "prop",
        default_scale: 0.85,
        anim_preset: "static",
        keywords: &["辦公桌", "電腦桌", "電腦", "office desk", "desk"],
    },
    PresetDef {
        key: "office_chair",
        name: "旋轉辦公椅",
        category: "prop",
        default_scale: 0.65,
        anim_preset: "static",
        keywords: &["辦公椅", "電腦椅", "轉椅", "office chair"],
    },
    PresetDef {
        key: "printer",
        name: "多功能事務印表機",
        category: "prop",
        default_scale: 0.82,
        anim_preset: "static",
        keywords: &["印表機", "影印機", "事務機", "printer", "copier"],
    },
    PresetDef {
        key: "bookshelf",
        name: "木頭書櫃",
        category: "prop",
        default_scale: 0.9,
        anim_preset: "static",
        keywords: &["書櫃", "書架", "圖書櫃", "bookshelf"],
    },
    PresetDef {
        key: "meeting_table",
        name: "大型會議桌",
        category: "prop",
        default_scale: 1.05,
        anim_preset: "static",
        keywords: &["會議桌", "長桌", "meeting table"],
    },
    PresetDef {
        key: "water_cooler",
        name: "辦公室飲水機",
        category: "prop",
        default_scale: 0.7,
        anim_preset: "static",
        keywords: &["飲水機", "開飲機", "water cooler"],
    },
    PresetDef {
        key: "blackboard",
        name: "教室黑板講台",
        category: "prop",
        default_scale: 1.05,
        anim_preset: "static",
        keywords: &["黑板", "白板", "講台", "blackboard", "whiteboard"],
    },
    PresetDef {
        key: "school_desk",
        name: "學生課桌椅",
        category: "prop",
        default_scale: 0.75,
        anim_preset: "static",
        keywords: &["課桌椅", "課桌", "書桌", "school desk"],
    },
    // --- 溫馨住家與室內植栽 ---
    PresetDef {
        key: "sofa",
        name: "客廳布藝沙發",
        category: "prop",
        default_scale: 0.9,
        anim_preset: "static",
        keywords: &["沙發", "沙發椅", "sofa", "couch"],
    },
    PresetDef {
        key: "bed",
        name: "舒適雙人床",
        category: "prop",
        default_scale: 0.95,
        anim_preset: "static",
        keywords: &["雙人床", "大床", "床鋪", "床", "bed"],
    },
    PresetDef {
        key: "tv_stand",
        name: "發光電視櫃",
        category: "prop",
        default_scale: 0.88,
        anim_preset: "static",
        keywords: &["電視", "電視櫃", "螢幕", "tv"],
    },
    PresetDef {
        key: "dining_table",
        name: "溫馨餐桌椅",
        category: "prop",
        default_scale: 0.88,
        anim_preset: "static",
        keywords: &["餐桌", "飯桌", "圓桌", "dining table"],
    },
    PresetDef {
        key: "kitchen_stove",
        name: "廚房流理台爐灶",
        category: "prop",
        default_scale: 0.9,
        anim_preset: "static",
        keywords: &["廚房", "流理台", "爐灶", "瓦斯爐", "kitchen", "stove"],
    },
    PresetDef {
        key: "sink_vanity",
        name: "浴室洗手台鏡櫃",
        category: "prop",
        default_scale: 0.85,
        anim_preset: "static",
        keywords: &["洗手台", "洗臉台", "盥洗台", "鏡櫃", "sink", "vanity"],
    },
    PresetDef {
        key: "potted_plant",
        name: "室內綠意盆栽",
        category: "plant",
        default_scale: 0.68,
        anim_preset: "sway",
        keywords: &["盆栽", "花盆", "盆景", "potted plant", "plant"],
    },
    PresetDef {
        key: "wooden_door",
        name: "古典木門",
        category: "prop",
        default_scale: 0.92,
        anim_preset: "static",
        keywords: &["木門", "大門", "房門", "建築門", "入口門", "出口門", "wooden door", "door"],
    },
    PresetDef {
        key: "portal_gate",
        name: "星光傳送門",
        category: "prop",
        default_scale: 1.0,
        anim_preset: "bob",
        keywords: &["傳送門", "光門", "時空門", "magic portal", "portal"],
    },
];

/// 回傳所有 76 種預設物件目錄清單
pub fn list_presets() -> Vec<PresetCatalogItem> {
    PRESETS
        .iter()
        .map(|p| PresetCatalogItem {
            key: p.key.to_string(),
            name: p.name.to_string(),
            category: p.category.to_string(),
            default_scale: p.default_scale,
            anim_preset: p.anim_preset.to_string(),
            prompt_hint: p.name.to_string(),
        })
        .collect()
}

const COLOR_RULES: &[(&str, &[&str])] = &[
    ("vermilion", &["朱紅", "硃砂", "vermilion"]),
    ("sakura", &["櫻花粉", "淺粉", "sakura"]),
    ("neon_cyan", &["霓虹青", "螢光藍", "雷射藍", "neon cyan"]),
    ("neon_pink", &["霓虹粉", "螢光粉", "電光粉", "neon pink"]),
    ("neon_yellow", &["霓虹黃", "螢光黃", "neon yellow"]),
    ("red", &["紅色", "深紅", "赤紅", "紅", "crimson", "red"]),
    ("blue", &["藍色", "深藍", "蔚藍", "藍", "azure", "blue"]),
    ("sky", &["天藍", "淺藍", "水藍", "sky"]),
    ("jade", &["碧玉", "翡翠", "玉綠", "jade"]),
    ("green", &["綠色", "翠綠", "森綠", "綠", "green"]),
    ("gold", &["金色", "黃金", "金黃", "金", "gold", "golden"]),
    ("yellow", &["黃色", "亮黃", "黃", "yellow"]),
    ("orange", &["橘色", "橙色", "橘", "橙", "orange"]),
    ("purple", &["紫色", "紫羅蘭", "紫", "purple", "violet"]),
    ("pink", &["粉紅", "粉色", "桃紅", "粉", "pink"]),
    ("cyan", &["青色", "青藍", "青", "cyan", "teal"]),
    ("white", &["白色", "純白", "雪白", "白", "white"]),
    ("black", &["黑色", "暗黑", "黑曜", "黑", "black"]),
    ("silver", &["銀色", "白銀", "銀", "silver"]),
    ("gray", &["灰色", "鐵灰", "灰", "gray", "grey"]),
    ("brown", &["棕色", "咖啡色", "木色", "褐", "棕", "brown", "wood"]),
    ("ice", &["冰藍", "寒冰", "ice"]),
];

/// 解析使用者輸入之繁體中文或英文描述為創作規格
pub fn parse_prompt(prompt: &str) -> ParsedPrompt {
    let raw = prompt.trim();
    let lower = raw.to_ascii_lowercase();

    // 偵測覆雪、青苔、夜光修飾
    let snow = ["雪中", "積雪", "下雪", "雪景", "覆雪", "冰封", "snow", "snowy", "winter"]
        .iter()
        .any(|k| raw.contains(k) || lower.contains(k));
    let moss = ["青苔", "古老", "遺跡", "藤蔓", "moss", "ancient", "ruins"]
        .iter()
        .any(|k| raw.contains(k) || lower.contains(k));
    let glow = ["發光", "夜光", "霓虹", "發亮", "點燈", "glow", "neon", "luminous"]
        .iter()
        .any(|k| raw.contains(k) || lower.contains(k));

    // 提取顏色關鍵字（最多主色與輔色）
    let mut found_colors: Vec<String> = Vec::new();
    for (col_name, keys) in COLOR_RULES {
        if keys.iter().any(|k| raw.contains(k) || lower.contains(k)) {
            if !found_colors.iter().any(|c| c == col_name) {
                found_colors.push((*col_name).to_string());
            }
        }
    }
    let primary_color = found_colors.first().cloned();
    let secondary_color = found_colors.get(1).cloned();

    // 若完全吻合某預設 key（例如從目錄直接點擊 "castle"）
    if let Some(p) = PRESETS
        .iter()
        .find(|p| p.key.eq_ignore_ascii_case(raw) || p.name == raw)
    {
        return ParsedPrompt {
            key: p.key.to_string(),
            title: p.name.to_string(),
            category: p.category.to_string(),
            primary_color,
            secondary_color,
            snow,
            moss,
            glow,
            default_scale: p.default_scale,
            anim_preset: p.anim_preset.to_string(),
        };
    }

    // 依關鍵字最長優先匹配預設原型
    let mut best: Option<(&PresetDef, usize)> = None;
    for p in PRESETS {
        for kw in p.keywords {
            if raw.contains(kw) || lower.contains(kw) {
                let score = kw.len();
                if best.map_or(true, |(_, bs)| score > bs) {
                    best = Some((p, score));
                }
            }
        }
    }

    if let Some((p, _)) = best {
        let title = if raw.is_empty() {
            p.name.to_string()
        } else {
            raw.to_string()
        };
        return ParsedPrompt {
            key: p.key.to_string(),
            title,
            category: p.category.to_string(),
            primary_color,
            secondary_color,
            snow,
            moss,
            glow,
            default_scale: p.default_scale,
            anim_preset: p.anim_preset.to_string(),
        };
    }

    // 未命中預設時退回創意幾何雕像
    ParsedPrompt {
        key: "custom_monument".to_string(),
        title: if raw.is_empty() {
            "創意奇觀雕塑".to_string()
        } else {
            raw.to_string()
        },
        category: "prop".to_string(),
        primary_color,
        secondary_color,
        snow,
        moss,
        glow,
        default_scale: 1.0,
        anim_preset: if glow { "float" } else { "static" }.to_string(),
    }
}

/// 依據解析規格建立標準 3D 幾何藍圖 JSON
pub fn build_blueprint_for_spec(spec: &ParsedPrompt, slices: usize) -> Value {
    let h = slices.clamp(24, 64) as f32;
    let p_col = |def: &str| -> String { spec.primary_color.clone().unwrap_or_else(|| def.to_string()) };
    let s_col = |def: &str| -> String { spec.secondary_color.clone().unwrap_or_else(|| def.to_string()) };
    let title = &spec.title;

    match spec.key.as_str() {
        "castle" => {
            let stone = p_col("stone");
            let roof = s_col("blue");
            json!({
                "title": title,
                "size": [44, 44, h],
                "primitives": [
                    {"type": "box", "x0": 2, "y0": 2, "z0": 0, "x1": 42, "y1": 42, "z1": 2, "color": "grass", "material": "foliage"},
                    {"type": "box", "x0": 6, "y0": 6, "z0": 2, "x1": 38, "y1": 38, "z1": 22, "color": stone, "material": "stone"},
                    {"type": "clear_box", "x0": 11, "y0": 11, "z0": 2, "x1": 33, "y1": 33, "z1": 23},
                    {"type": "arch_cut", "cx": 21.5, "cy": 36.0, "z0": 2, "z1": 18, "half_span": 4.2, "depth": 5.0, "axis": "y"},
                    {"type": "box", "x0": 14, "y0": 38, "z0": 8, "x1": 17, "y1": 40, "z1": 12, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "cylinder", "cx": 8.0, "cy": 8.0, "z0": 2, "z1": 30, "rx": 4.8, "ry": 4.8, "color": stone, "material": "stone", "mirror_x": true},
                    {"type": "cone", "cx": 8.0, "cy": 8.0, "z0": 30, "z1": 39, "rx0": 5.4, "ry0": 5.4, "rx1": 1.2, "ry1": 1.2, "color": roof, "material": "fabric", "mirror_x": true},
                    {"type": "cylinder", "cx": 8.0, "cy": 35.0, "z0": 2, "z1": 30, "rx": 4.8, "ry": 4.8, "color": stone, "material": "stone", "mirror_x": true},
                    {"type": "cone", "cx": 8.0, "cy": 35.0, "z0": 30, "z1": 39, "rx0": 5.4, "ry0": 5.4, "rx1": 1.2, "ry1": 1.2, "color": roof, "material": "fabric", "mirror_x": true},
                    {"type": "box", "x0": 14, "y0": 14, "z0": 2, "x1": 30, "y1": 30, "z1": 34, "color": "marble", "material": "stone"},
                    {"type": "box", "x0": 19, "y0": 29, "z0": 18, "x1": 25, "y1": 31, "z1": 24, "color": "neon_yellow", "emissive": true},
                    {"type": "cone", "cx": 21.5, "cy": 21.5, "z0": 34, "z1": h - 2.0, "rx0": 8.2, "ry0": 8.2, "rx1": 1.4, "ry1": 1.4, "color": roof, "material": "fabric"},
                    {"type": "box", "x0": 21, "y0": 21, "z0": h - 4.0, "x1": 27, "y1": 23, "z1": h, "color": "red"}
                ]
            })
        }
        "clock_tower" => {
            let wall = p_col("sandstone");
            let roof = s_col("dark_stone");
            json!({
                "title": title,
                "size": [32, 32, 48],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 4, "z0": 0, "x1": 28, "y1": 28, "z1": 5, "color": "stone", "material": "stone"},
                    {"type": "box", "x0": 9, "y0": 9, "z0": 5, "x1": 23, "y1": 23, "z1": 27, "color": wall, "material": "stone"},
                    {"type": "box", "x0": 8, "y0": 8, "z0": 27, "x1": 24, "y1": 24, "z1": 35, "color": "gold", "material": "metal"},
                    {"type": "box", "x0": 12, "y0": 23, "z0": 28, "x1": 20, "y1": 25, "z1": 34, "color": "neon_yellow", "emissive": true},
                    {"type": "box", "x0": 12, "y0": 7, "z0": 28, "x1": 20, "y1": 9, "z1": 34, "color": "neon_yellow", "emissive": true},
                    {"type": "box", "x0": 23, "y0": 12, "z0": 28, "x1": 25, "y1": 20, "z1": 34, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "pyramid", "cx": 15.5, "cy": 15.5, "z0": 35, "z1": 45, "w0": 15.0, "l0": 15.0, "w1": 2.0, "l1": 2.0, "color": roof, "material": "metal"},
                    {"type": "cylinder", "cx": 15.5, "cy": 15.5, "z0": 44, "z1": 48, "rx": 1.4, "ry": 1.4, "color": "gold", "emissive": true}
                ]
            })
        }
        "cottage" => {
            let wall = p_col("wood");
            let roof = s_col("red");
            json!({
                "title": title,
                "size": [38, 38, 44],
                "primitives": [
                    {"type": "box", "x0": 3, "y0": 3, "z0": 0, "x1": 35, "y1": 35, "z1": 2, "color": "grass", "material": "foliage"},
                    {"type": "box", "x0": 7, "y0": 7, "z0": 2, "x1": 31, "y1": 31, "z1": 5, "color": "stone", "material": "stone"},
                    {"type": "box", "x0": 8, "y0": 8, "z0": 5, "x1": 30, "y1": 30, "z1": 22, "color": wall, "material": "wood"},
                    {"type": "box", "x0": 16, "y0": 29, "z0": 5, "x1": 22, "y1": 32, "z1": 17, "color": "dark_wood", "material": "wood"},
                    {"type": "box", "x0": 10, "y0": 29, "z0": 9, "x1": 14, "y1": 31, "z1": 14, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "box", "x0": 7, "y0": 15, "z0": 9, "x1": 9, "y1": 22, "z1": 16, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "pyramid", "cx": 18.5, "cy": 18.5, "z0": 22, "z1": 38, "w0": 26.0, "l0": 26.0, "w1": 3.0, "l1": 22.0, "color": roof, "material": "stone"},
                    {"type": "box", "x0": 24, "y0": 10, "z0": 22, "x1": 28, "y1": 14, "z1": 40, "color": "stone", "material": "stone"},
                    {"type": "sphere", "cx": 26.0, "cy": 12.0, "cz": 41.5, "rx": 2.2, "ry": 2.2, "rz": 2.0, "color": "white"}
                ]
            })
        }
        "skyscraper" => {
            let wall = p_col("sky");
            let accent = s_col("neon_cyan");
            json!({
                "title": title,
                "size": [34, 34, 48],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 4, "z0": 0, "x1": 30, "y1": 30, "z1": 6, "color": "dark_stone", "material": "stone"},
                    {"type": "box", "x0": 7, "y0": 7, "z0": 6, "x1": 27, "y1": 27, "z1": 38, "color": wall, "material": "glass"},
                    {"type": "box", "x0": 9, "y0": 9, "z0": 38, "x1": 25, "y1": 25, "z1": 44, "color": "silver", "material": "metal"},
                    {"type": "box", "x0": 10, "y0": 26, "z0": 10, "x1": 14, "y1": 28, "z1": 35, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "box", "x0": 6, "y0": 13, "z0": 12, "x1": 8, "y1": 21, "z1": 33, "color": accent, "emissive": true, "mirror_x": true},
                    {"type": "box", "x0": 14, "y0": 14, "z0": 44, "x1": 20, "y1": 20, "z1": 48, "color": accent, "emissive": true}
                ]
            })
        }
        "shop" => {
            let wall = p_col("marble");
            let awning = s_col("vermilion");
            json!({
                "title": title,
                "size": [40, 36, 42],
                "primitives": [
                    {"type": "box", "x0": 2, "y0": 2, "z0": 0, "x1": 38, "y1": 34, "z1": 2, "color": "stone", "material": "stone"},
                    {"type": "box", "x0": 6, "y0": 6, "z0": 2, "x1": 34, "y1": 27, "z1": 28, "color": wall, "material": "stone"},
                    {"type": "box", "x0": 10, "y0": 26, "z0": 4, "x1": 30, "y1": 29, "z1": 18, "color": "neon_yellow", "emissive": true},
                    {"type": "box", "x0": 7, "y0": 26, "z0": 19, "x1": 33, "y1": 33, "z1": 23, "color": awning, "material": "fabric"},
                    {"type": "box", "x0": 11, "y0": 25, "z0": 28, "x1": 29, "y1": 29, "z1": 35, "color": "neon_pink", "emissive": true},
                    {"type": "box", "x0": 8, "y0": 8, "z0": 28, "x1": 32, "y1": 26, "z1": 30, "color": "dark_gray", "material": "metal"}
                ]
            })
        }
        "barn" => {
            let wall = p_col("red");
            let trim = s_col("white");
            json!({
                "title": title,
                "size": [40, 44, 42],
                "primitives": [
                    {"type": "box", "x0": 3, "y0": 3, "z0": 0, "x1": 37, "y1": 41, "z1": 2, "color": "grass", "material": "foliage"},
                    {"type": "box", "x0": 7, "y0": 7, "z0": 2, "x1": 33, "y1": 37, "z1": 22, "color": wall, "material": "wood"},
                    {"type": "box", "x0": 14, "y0": 36, "z0": 2, "x1": 26, "y1": 38, "z1": 15, "color": trim, "material": "wood"},
                    {"type": "box", "x0": 18, "y0": 37, "z0": 16, "x1": 22, "y1": 39, "z1": 20, "color": "neon_yellow", "emissive": true},
                    {"type": "pyramid", "cx": 19.5, "cy": 21.5, "z0": 22, "z1": 38, "w0": 30.0, "l0": 34.0, "w1": 5.0, "l1": 32.0, "stepped": true, "steps": 6, "color": "dark_wood", "material": "wood"}
                ]
            })
        }
        "windmill" => {
            let tower = p_col("marble");
            let sail = s_col("white");
            json!({
                "title": title,
                "size": [42, 42, 48],
                "primitives": [
                    {"type": "cylinder", "cx": 20.5, "cy": 20.5, "z0": 0, "z1": 4, "rx": 12.0, "ry": 12.0, "color": "grass", "material": "foliage"},
                    {"type": "cone", "cx": 20.5, "cy": 20.5, "z0": 4, "z1": 32, "rx0": 9.0, "ry0": 9.0, "rx1": 5.8, "ry1": 5.8, "color": tower, "material": "stone"},
                    {"type": "cone", "cx": 20.5, "cy": 20.5, "z0": 32, "z1": 42, "rx0": 7.0, "ry0": 7.0, "rx1": 1.5, "ry1": 1.5, "color": "red", "material": "stone"},
                    {"type": "box", "x0": 18, "y0": 29, "z0": 4, "x1": 23, "y1": 31, "z1": 12, "color": "dark_wood"},
                    {"type": "box", "x0": 19, "y0": 29, "z0": 24, "x1": 22, "y1": 31, "z1": 28, "color": "neon_yellow", "emissive": true},
                    {"type": "box", "x0": 19, "y0": 27, "z0": 28, "x1": 22, "y1": 34, "z1": 31, "color": "dark_wood", "material": "wood"},
                    {"type": "line", "p0": [20.5, 33.0, 29.5], "p1": [7.0, 33.0, 43.0], "radius": 1.6, "color": sail, "material": "fabric", "mirror_x": true},
                    {"type": "line", "p0": [20.5, 33.0, 29.5], "p1": [7.0, 33.0, 16.0], "radius": 1.6, "color": sail, "material": "fabric", "mirror_x": true}
                ]
            })
        }
        "school" => {
            let brick = p_col("sandstone");
            let roof = s_col("vermilion");
            json!({
                "title": title,
                "size": [48, 36, 44],
                "primitives": [
                    {"type": "box", "x0": 2, "y0": 2, "z0": 0, "x1": 46, "y1": 34, "z1": 2, "color": "stone", "material": "stone"},
                    {"type": "box", "x0": 4, "y0": 8, "z0": 2, "x1": 44, "y1": 26, "z1": 22, "color": brick, "material": "stone"},
                    {"type": "box", "x0": 8, "y0": 25, "z0": 7, "x1": 14, "y1": 27, "z1": 13, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "box", "x0": 8, "y0": 25, "z0": 15, "x1": 14, "y1": 27, "z1": 20, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "box", "x0": 19, "y0": 10, "z0": 2, "x1": 29, "y1": 29, "z1": 31, "color": "marble", "material": "stone"},
                    {"type": "box", "x0": 22, "y0": 28, "z0": 2, "x1": 26, "y1": 30, "z1": 11, "color": "dark_wood"},
                    {"type": "box", "x0": 22, "y0": 28, "z0": 24, "x1": 26, "y1": 30, "z1": 28, "color": "neon_yellow", "emissive": true},
                    {"type": "pyramid", "cx": 23.5, "cy": 18.0, "z0": 31, "z1": 41, "w0": 12.0, "l0": 14.0, "w1": 2.0, "l1": 2.0, "color": roof, "material": "stone"},
                    {"type": "box", "x0": 3, "y0": 7, "z0": 22, "x1": 45, "y1": 27, "z1": 25, "color": roof, "material": "stone"}
                ]
            })
        }
        "lighthouse" => {
            let stripe = p_col("red");
            let white = s_col("white");
            json!({
                "title": title,
                "size": [38, 38, 48],
                "primitives": [
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 0, "z1": 2, "rx": 16.0, "ry": 16.0, "color": "water", "material": "glass"},
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 1, "z1": 6, "rx": 11.5, "ry": 11.0, "color": "dark_stone", "material": "stone"},
                    {"type": "cone", "cx": 18.5, "cy": 18.5, "z0": 6, "z1": 14, "rx0": 7.8, "ry0": 7.8, "rx1": 6.8, "ry1": 6.8, "color": stripe, "material": "stone"},
                    {"type": "cone", "cx": 18.5, "cy": 18.5, "z0": 14, "z1": 22, "rx0": 6.8, "ry0": 6.8, "rx1": 5.8, "ry1": 5.8, "color": white, "material": "stone"},
                    {"type": "cone", "cx": 18.5, "cy": 18.5, "z0": 22, "z1": 30, "rx0": 5.8, "ry0": 5.8, "rx1": 5.0, "ry1": 5.0, "color": stripe, "material": "stone"},
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 30, "z1": 33, "rx": 6.6, "ry": 6.6, "color": "dark_gray", "material": "metal"},
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 33, "z1": 40, "rx": 4.4, "ry": 4.4, "color": "neon_yellow", "emissive": true},
                    {"type": "cone", "cx": 18.5, "cy": 18.5, "z0": 40, "z1": 47, "rx0": 5.5, "ry0": 5.5, "rx1": 1.2, "ry1": 1.2, "color": stripe, "material": "metal"}
                ]
            })
        }
        "torii" => {
            let main = p_col("vermilion");
            json!({
                "title": title,
                "size": [44, 24, 42],
                "primitives": [
                    {"type": "cylinder", "cx": 11.0, "cy": 11.5, "z0": 0, "z1": 6, "rx": 3.2, "ry": 3.2, "color": "dark_stone", "material": "stone", "mirror_x": true},
                    {"type": "cylinder", "cx": 11.0, "cy": 11.5, "z0": 6, "z1": 35, "rx": 2.3, "ry": 2.3, "color": main, "material": "wood", "mirror_x": true},
                    {"type": "box", "x0": 6, "y0": 9, "z0": 25, "x1": 38, "y1": 14, "z1": 28, "color": main, "material": "wood"},
                    {"type": "box", "x0": 3, "y0": 8, "z0": 33, "x1": 41, "y1": 15, "z1": 37, "color": main, "material": "wood"},
                    {"type": "box", "x0": 2, "y0": 8, "z0": 37, "x1": 42, "y1": 15, "z1": 39, "color": "black", "material": "stone"},
                    {"type": "box", "x0": 19, "y0": 9, "z0": 28, "x1": 25, "y1": 14, "z1": 33, "color": "gold", "material": "metal"},
                    {"type": "box", "x0": 10, "y0": 16, "z0": 0, "x1": 13, "y1": 19, "z1": 6, "color": "neon_yellow", "emissive": true, "mirror_x": true}
                ]
            })
        }
        "fountain" => {
            let stone = p_col("marble");
            let water = s_col("neon_cyan");
            json!({
                "title": title,
                "size": [38, 38, 34],
                "primitives": [
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 0, "z1": 5, "rx": 14.5, "ry": 14.5, "color": stone, "material": "stone"},
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 2, "z1": 6, "rx": 12.2, "ry": 12.2, "color": "water", "material": "glass"},
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 4, "z1": 18, "rx": 3.2, "ry": 3.2, "color": stone, "material": "stone"},
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 14, "z1": 17, "rx": 7.5, "ry": 7.5, "color": stone, "material": "stone"},
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 18, "z1": 28, "rx": 2.0, "ry": 2.0, "color": water, "emissive": true},
                    {"type": "sphere", "cx": 18.5, "cy": 18.5, "cz": 29.0, "rx": 4.2, "ry": 4.2, "rz": 2.8, "color": water, "emissive": true}
                ]
            })
        }
        "pagoda" => {
            let wall = p_col("vermilion");
            let roof = s_col("dark_gray");
            json!({
                "title": title,
                "size": [40, 40, 48],
                "primitives": [
                    {"type": "box", "x0": 6, "y0": 6, "z0": 0, "x1": 34, "y1": 34, "z1": 4, "color": "stone", "material": "stone"},
                    {"type": "box", "x0": 11, "y0": 11, "z0": 4, "x1": 29, "y1": 29, "z1": 13, "color": wall, "material": "wood"},
                    {"type": "pyramid", "cx": 19.5, "cy": 19.5, "z0": 13, "z1": 19, "w0": 27.0, "l0": 27.0, "w1": 14.0, "l1": 14.0, "color": roof, "material": "stone"},
                    {"type": "box", "x0": 13, "y0": 13, "z0": 19, "x1": 27, "y1": 27, "z1": 27, "color": wall, "material": "wood"},
                    {"type": "pyramid", "cx": 19.5, "cy": 19.5, "z0": 27, "z1": 33, "w0": 23.0, "l0": 23.0, "w1": 10.0, "l1": 10.0, "color": roof, "material": "stone"},
                    {"type": "box", "x0": 15, "y0": 15, "z0": 33, "x1": 25, "y1": 25, "z1": 39, "color": "neon_yellow", "emissive": true},
                    {"type": "pyramid", "cx": 19.5, "cy": 19.5, "z0": 39, "z1": 44, "w0": 18.0, "l0": 18.0, "w1": 3.0, "l1": 3.0, "color": roof, "material": "stone"},
                    {"type": "cylinder", "cx": 19.5, "cy": 19.5, "z0": 43, "z1": 48, "rx": 1.3, "ry": 1.3, "color": "gold", "emissive": true}
                ]
            })
        }
        "knight" => {
            let armor = p_col("blue");
            let plume = s_col("red");
            json!({
                "title": title,
                "size": [34, 32, 42],
                "primitives": [
                    {"type": "box", "x0": 12, "y0": 13, "z0": 0, "x1": 16, "y1": 19, "z1": 4, "color": "dark_wood", "mirror_x": true},
                    {"type": "box", "x0": 12, "y0": 13, "z0": 4, "x1": 16, "y1": 18, "z1": 13, "color": "dark_gray", "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 11, "y0": 12, "z0": 13, "x1": 23, "y1": 20, "z1": 25, "color": armor, "material": "metal"},
                    {"type": "box", "x0": 15, "y0": 19, "z0": 13, "x1": 19, "y1": 22, "z1": 16, "color": "gold", "material": "metal"},
                    {"type": "box", "x0": 7, "y0": 13, "z0": 14, "x1": 11, "y1": 18, "z1": 24, "color": "silver", "material": "metal", "mirror_x": true},
                    {"type": "sphere", "cx": 16.5, "cy": 16.0, "cz": 30.0, "rx": 5.5, "ry": 5.2, "rz": 5.5, "color": "silver", "material": "metal"},
                    {"type": "box", "x0": 14, "y0": 20, "z0": 29, "x1": 19, "y1": 22, "z1": 31, "color": "neon_cyan", "emissive": true},
                    {"type": "box", "x0": 15, "y0": 11, "z0": 35, "x1": 19, "y1": 19, "z1": 41, "color": plume, "material": "fabric"},
                    {"type": "box", "x0": 5, "y0": 18, "z0": 17, "x1": 8, "y1": 20, "z1": 38, "color": "silver", "material": "metal"},
                    {"type": "box", "x0": 24, "y0": 14, "z0": 12, "x1": 27, "y1": 21, "z1": 24, "color": "gold", "material": "metal"}
                ]
            })
        }
        "mage" => {
            let robe = p_col("purple");
            let trim = s_col("gold");
            json!({
                "title": title,
                "size": [34, 32, 44],
                "primitives": [
                    {"type": "cone", "cx": 16.5, "cy": 15.5, "z0": 0, "z1": 24, "rx0": 7.5, "ry0": 6.5, "rx1": 5.5, "ry1": 4.8, "color": robe, "material": "fabric"},
                    {"type": "cone", "cx": 16.5, "cy": 15.5, "z0": 0, "z1": 3, "rx0": 7.8, "ry0": 6.8, "rx1": 7.2, "ry1": 6.2, "color": trim, "material": "metal"},
                    {"type": "box", "x0": 8, "y0": 13, "z0": 14, "x1": 11, "y1": 18, "z1": 22, "color": robe, "material": "fabric", "mirror_x": true},
                    {"type": "sphere", "cx": 16.5, "cy": 15.5, "cz": 28.0, "rx": 5.2, "ry": 5.0, "rz": 5.0, "color": "skin"},
                    {"type": "box", "x0": 13, "y0": 19, "z0": 28, "x1": 15, "y1": 21, "z1": 30, "color": "neon_cyan", "emissive": true, "mirror_x": true},
                    {"type": "cylinder", "cx": 16.5, "cy": 15.5, "z0": 32, "z1": 34, "rx": 8.0, "ry": 8.0, "color": robe, "material": "fabric"},
                    {"type": "cone", "cx": 16.5, "cy": 15.5, "z0": 34, "z1": 44, "rx0": 5.5, "ry0": 5.5, "rx1": 1.3, "ry1": 1.3, "color": robe, "material": "fabric"},
                    {"type": "box", "x0": 5, "y0": 17, "z0": 4, "x1": 7, "y1": 19, "z1": 32, "color": "wood", "material": "wood"},
                    {"type": "sphere", "cx": 6.0, "cy": 18.0, "cz": 34.0, "rx": 2.5, "ry": 2.5, "rz": 2.5, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "student" => {
            let uniform = p_col("navy");
            let hair = s_col("brown");
            json!({
                "title": title,
                "size": [30, 30, 40],
                "primitives": [
                    {"type": "box", "x0": 11, "y0": 12, "z0": 0, "x1": 14, "y1": 17, "z1": 12, "color": "dark_gray", "mirror_x": true},
                    {"type": "box", "x0": 10, "y0": 11, "z0": 12, "x1": 20, "y1": 18, "z1": 23, "color": uniform, "material": "fabric"},
                    {"type": "box", "x0": 13, "y0": 17, "z0": 19, "x1": 17, "y1": 19, "z1": 22, "color": "red", "shaded": false},
                    {"type": "box", "x0": 11, "y0": 8, "z0": 13, "x1": 19, "y1": 11, "z1": 22, "color": "orange", "material": "fabric"},
                    {"type": "box", "x0": 7, "y0": 12, "z0": 13, "x1": 10, "y1": 17, "z1": 22, "color": uniform, "mirror_x": true},
                    {"type": "sphere", "cx": 14.5, "cy": 14.5, "cz": 28.0, "rx": 5.2, "ry": 5.0, "rz": 5.2, "color": "skin"},
                    {"type": "sphere", "cx": 14.5, "cy": 13.5, "cz": 30.0, "rx": 5.4, "ry": 5.0, "rz": 4.8, "color": hair, "material": "fur"},
                    {"type": "box", "x0": 11, "y0": 18, "z0": 27, "x1": 13, "y1": 20, "z1": 29, "color": "black", "shaded": false, "mirror_x": true}
                ]
            })
        }
        "farmer" => {
            let overalls = p_col("blue");
            let hat = s_col("yellow");
            json!({
                "title": title,
                "size": [32, 30, 40],
                "primitives": [
                    {"type": "box", "x0": 11, "y0": 12, "z0": 0, "x1": 15, "y1": 18, "z1": 12, "color": overalls, "material": "fabric", "mirror_x": true},
                    {"type": "box", "x0": 10, "y0": 11, "z0": 12, "x1": 22, "y1": 19, "z1": 23, "color": "red", "material": "fabric"},
                    {"type": "box", "x0": 11, "y0": 12, "z0": 12, "x1": 21, "y1": 20, "z1": 20, "color": overalls, "material": "fabric"},
                    {"type": "box", "x0": 7, "y0": 13, "z0": 14, "x1": 10, "y1": 17, "z1": 22, "color": "skin", "mirror_x": true},
                    {"type": "sphere", "cx": 15.5, "cy": 15.0, "cz": 28.0, "rx": 5.2, "ry": 5.0, "rz": 5.0, "color": "skin"},
                    {"type": "cylinder", "cx": 15.5, "cy": 15.0, "z0": 31, "z1": 33, "rx": 9.0, "ry": 9.0, "color": hat, "material": "fabric"},
                    {"type": "cylinder", "cx": 15.5, "cy": 15.0, "z0": 33, "z1": 37, "rx": 5.5, "ry": 5.5, "color": hat, "material": "fabric"},
                    {"type": "box", "x0": 5, "y0": 16, "z0": 2, "x1": 7, "y1": 18, "z1": 28, "color": "wood", "material": "wood"}
                ]
            })
        }
        "citizen" => {
            let jacket = p_col("orange");
            let pants = s_col("navy");
            json!({
                "title": title,
                "size": [30, 30, 40],
                "primitives": [
                    {"type": "box", "x0": 11, "y0": 12, "z0": 0, "x1": 14, "y1": 17, "z1": 12, "color": pants, "material": "fabric", "mirror_x": true},
                    {"type": "box", "x0": 10, "y0": 11, "z0": 12, "x1": 20, "y1": 18, "z1": 23, "color": jacket, "material": "fabric"},
                    {"type": "box", "x0": 7, "y0": 12, "z0": 13, "x1": 10, "y1": 17, "z1": 22, "color": jacket, "mirror_x": true},
                    {"type": "sphere", "cx": 14.5, "cy": 14.5, "cz": 28.0, "rx": 5.2, "ry": 5.0, "rz": 5.0, "color": "skin"},
                    {"type": "sphere", "cx": 14.5, "cy": 13.5, "cz": 30.0, "rx": 5.4, "ry": 5.0, "rz": 4.6, "color": "black", "material": "fur"},
                    {"type": "box", "x0": 11, "y0": 18, "z0": 27, "x1": 13, "y1": 20, "z1": 29, "color": "black", "shaded": false, "mirror_x": true}
                ]
            })
        }
        "astronaut" => {
            let suit = p_col("white");
            let trim = s_col("orange");
            json!({
                "title": title,
                "size": [34, 32, 42],
                "primitives": [
                    {"type": "box", "x0": 11, "y0": 12, "z0": 0, "x1": 15, "y1": 18, "z1": 12, "color": suit, "material": "fabric", "mirror_x": true},
                    {"type": "box", "x0": 10, "y0": 11, "z0": 12, "x1": 24, "y1": 20, "z1": 25, "color": suit, "material": "fabric"},
                    {"type": "box", "x0": 11, "y0": 6, "z0": 12, "x1": 23, "y1": 11, "z1": 25, "color": trim, "material": "metal"},
                    {"type": "box", "x0": 7, "y0": 12, "z0": 14, "x1": 10, "y1": 18, "z1": 23, "color": suit, "mirror_x": true},
                    {"type": "sphere", "cx": 16.5, "cy": 15.5, "cz": 31.0, "rx": 6.4, "ry": 6.2, "rz": 6.0, "color": suit, "material": "metal"},
                    {"type": "box", "x0": 13, "y0": 19, "z0": 29, "x1": 21, "y1": 23, "z1": 34, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "lion" => {
            let fur = p_col("gold");
            let mane = s_col("brown");
            json!({
                "title": title,
                "size": [38, 48, 36],
                "primitives": [
                    {"type": "cylinder", "cx": 12.5, "cy": 14.0, "z0": 0, "z1": 14, "rx": 2.8, "ry": 2.8, "color": fur, "material": "fur", "mirror_x": true},
                    {"type": "cylinder", "cx": 12.5, "cy": 32.0, "z0": 0, "z1": 14, "rx": 2.8, "ry": 2.8, "color": fur, "material": "fur", "mirror_x": true},
                    {"type": "sphere", "cx": 18.5, "cy": 23.0, "cz": 14.0, "rx": 7.8, "ry": 12.0, "rz": 6.8, "color": fur, "material": "fur"},
                    {"type": "sphere", "cx": 18.5, "cy": 32.0, "cz": 21.0, "rx": 9.5, "ry": 6.8, "rz": 9.5, "color": mane, "material": "mane"},
                    {"type": "sphere", "cx": 18.5, "cy": 36.0, "cz": 22.0, "rx": 5.5, "ry": 5.2, "rz": 5.2, "color": fur, "material": "fur"},
                    {"type": "box", "x0": 17, "y0": 40, "z0": 19, "x1": 21, "y1": 43, "z1": 22, "color": "black", "shaded": false},
                    {"type": "box", "x0": 14, "y0": 39, "z0": 22, "x1": 16, "y1": 42, "z1": 24, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "line", "p0": [18.5, 11.0, 15.0], "p1": [18.5, 5.0, 21.0], "radius": 1.3, "color": mane, "material": "mane"}
                ]
            })
        }
        "shiba" => {
            let fur = p_col("orange");
            json!({
                "title": title,
                "size": [30, 38, 30],
                "primitives": [
                    {"type": "box", "x0": 10, "y0": 10, "z0": 0, "x1": 13, "y1": 13, "z1": 9, "color": "white", "material": "fur", "mirror_x": true},
                    {"type": "box", "x0": 10, "y0": 24, "z0": 0, "x1": 13, "y1": 27, "z1": 9, "color": "white", "material": "fur", "mirror_x": true},
                    {"type": "sphere", "cx": 14.5, "cy": 18.5, "cz": 12.0, "rx": 5.8, "ry": 9.5, "rz": 5.5, "color": fur, "material": "fur"},
                    {"type": "sphere", "cx": 14.5, "cy": 26.5, "cz": 18.5, "rx": 5.5, "ry": 5.5, "rz": 5.2, "color": fur, "material": "fur"},
                    {"type": "box", "x0": 12, "y0": 29, "z0": 15, "x1": 18, "y1": 33, "z1": 19, "color": "white", "material": "fur"},
                    {"type": "box", "x0": 14, "y0": 32, "z0": 17, "x1": 16, "y1": 34, "z1": 19, "color": "black", "shaded": false},
                    {"type": "pyramid", "cx": 10.5, "cy": 25.0, "z0": 21, "z1": 27, "w0": 3.2, "l0": 3.0, "w1": 1.0, "l1": 1.0, "color": fur, "mirror_x": true},
                    {"type": "sphere", "cx": 14.5, "cy": 9.0, "cz": 18.0, "rx": 2.4, "ry": 3.0, "rz": 2.8, "color": fur, "material": "fur"}
                ]
            })
        }
        "sheep" => {
            let wool = p_col("white");
            json!({
                "title": title,
                "size": [30, 36, 28],
                "primitives": [
                    {"type": "box", "x0": 10, "y0": 11, "z0": 0, "x1": 13, "y1": 14, "z1": 8, "color": "dark_gray", "mirror_x": true},
                    {"type": "box", "x0": 10, "y0": 22, "z0": 0, "x1": 13, "y1": 25, "z1": 8, "color": "dark_gray", "mirror_x": true},
                    {"type": "sphere", "cx": 14.5, "cy": 18.0, "cz": 13.0, "rx": 7.5, "ry": 9.0, "rz": 7.0, "color": wool, "material": "fur"},
                    {"type": "sphere", "cx": 14.5, "cy": 25.5, "cz": 17.5, "rx": 4.2, "ry": 4.5, "rz": 4.0, "color": "black"},
                    {"type": "sphere", "cx": 14.5, "cy": 24.5, "cz": 21.0, "rx": 3.5, "ry": 3.5, "rz": 2.4, "color": wool, "material": "fur"},
                    {"type": "box", "x0": 11, "y0": 29, "z0": 17, "x1": 13, "y1": 31, "z1": 19, "color": "white", "shaded": false, "mirror_x": true}
                ]
            })
        }
        "horse" => {
            let coat = p_col("brown");
            let mane = s_col("dark_wood");
            json!({
                "title": title,
                "size": [32, 46, 38],
                "primitives": [
                    {"type": "box", "x0": 11, "y0": 11, "z0": 0, "x1": 14, "y1": 14, "z1": 15, "color": coat, "material": "fur", "mirror_x": true},
                    {"type": "box", "x0": 11, "y0": 30, "z0": 0, "x1": 14, "y1": 33, "z1": 15, "color": coat, "material": "fur", "mirror_x": true},
                    {"type": "sphere", "cx": 15.5, "cy": 22.0, "cz": 16.5, "rx": 5.5, "ry": 12.0, "rz": 5.5, "color": coat, "material": "fur"},
                    {"type": "line", "p0": [15.5, 29.0, 19.0], "p1": [15.5, 34.0, 29.0], "radius": 3.4, "color": coat, "material": "fur"},
                    {"type": "box", "x0": 13, "y0": 32, "z0": 26, "x1": 19, "y1": 42, "z1": 32, "color": coat, "material": "fur"},
                    {"type": "line", "p0": [15.5, 27.0, 22.0], "p1": [15.5, 32.5, 33.5], "radius": 1.6, "color": mane, "material": "mane"},
                    {"type": "line", "p0": [15.5, 10.0, 18.0], "p1": [15.5, 5.0, 10.0], "radius": 1.6, "color": mane, "material": "mane"}
                ]
            })
        }
        "duck" => {
            let body = p_col("white");
            json!({
                "title": title,
                "size": [26, 32, 24],
                "primitives": [
                    {"type": "box", "x0": 9, "y0": 13, "z0": 0, "x1": 12, "y1": 18, "z1": 4, "color": "orange", "mirror_x": true},
                    {"type": "sphere", "cx": 12.5, "cy": 15.0, "cz": 8.0, "rx": 5.5, "ry": 8.0, "rz": 5.0, "color": body, "material": "fur"},
                    {"type": "sphere", "cx": 12.5, "cy": 20.5, "cz": 15.0, "rx": 3.8, "ry": 4.0, "rz": 4.0, "color": body, "material": "fur"},
                    {"type": "box", "x0": 10, "y0": 23, "z0": 13, "x1": 16, "y1": 28, "z1": 16, "color": "orange"},
                    {"type": "box", "x0": 10, "y0": 22, "z0": 16, "x1": 12, "y1": 24, "z1": 18, "color": "black", "shaded": false, "mirror_x": true}
                ]
            })
        }
        "bird" => {
            let feather = p_col("sky");
            json!({
                "title": title,
                "size": [34, 30, 24],
                "primitives": [
                    {"type": "sphere", "cx": 16.5, "cy": 15.0, "cz": 10.0, "rx": 4.2, "ry": 7.0, "rz": 4.0, "color": feather, "material": "fur"},
                    {"type": "sphere", "cx": 16.5, "cy": 21.0, "cz": 13.0, "rx": 3.4, "ry": 3.4, "rz": 3.4, "color": feather, "material": "fur"},
                    {"type": "box", "x0": 3, "y0": 11, "z0": 11, "x1": 14, "y1": 18, "z1": 14, "color": "blue", "material": "fabric", "mirror_x": true},
                    {"type": "box", "x0": 15, "y0": 23, "z0": 12, "x1": 19, "y1": 27, "z1": 14, "color": "orange"}
                ]
            })
        }
        "sakura" => {
            let petal = p_col("sakura");
            json!({
                "title": title,
                "size": [38, 38, 46],
                "primitives": [
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 0, "z1": 2, "rx": 9.5, "ry": 9.5, "color": "grass", "material": "foliage"},
                    {"type": "cone", "cx": 18.5, "cy": 18.5, "z0": 1, "z1": 26, "rx0": 4.0, "ry0": 4.0, "rx1": 2.2, "ry1": 2.2, "color": "wood", "material": "wood"},
                    {"type": "sphere", "cx": 18.5, "cy": 18.5, "cz": 28.0, "rx": 11.0, "ry": 11.0, "rz": 9.5, "color": petal, "material": "foliage"},
                    {"type": "sphere", "cx": 13.0, "cy": 21.0, "cz": 25.0, "rx": 7.5, "ry": 7.5, "rz": 6.8, "color": petal, "material": "foliage", "mirror_x": true},
                    {"type": "sphere", "cx": 18.5, "cy": 14.0, "cz": 30.0, "rx": 7.0, "ry": 7.0, "rz": 6.5, "color": petal, "material": "foliage"},
                    {"type": "sphere", "cx": 18.5, "cy": 18.5, "cz": 37.5, "rx": 7.2, "ry": 7.2, "rz": 6.5, "color": petal, "material": "foliage"},
                    {"type": "box", "x0": 12, "y0": 23, "z0": 2, "x1": 14, "y1": 25, "z1": 3, "color": petal, "mirror_x": true}
                ]
            })
        }
        "pine" => {
            let leaf = p_col("emerald");
            json!({
                "title": title,
                "size": [34, 34, 48],
                "primitives": [
                    {"type": "cylinder", "cx": 16.5, "cy": 16.5, "z0": 0, "z1": 2, "rx": 8.0, "ry": 8.0, "color": "grass", "material": "foliage"},
                    {"type": "cylinder", "cx": 16.5, "cy": 16.5, "z0": 1, "z1": 14, "rx": 2.8, "ry": 2.8, "color": "dark_wood", "material": "wood"},
                    {"type": "cone", "cx": 16.5, "cy": 16.5, "z0": 11, "z1": 25, "rx0": 11.5, "ry0": 11.5, "rx1": 4.0, "ry1": 4.0, "color": leaf, "material": "foliage"},
                    {"type": "cone", "cx": 16.5, "cy": 16.5, "z0": 21, "z1": 35, "rx0": 9.2, "ry0": 9.2, "rx1": 2.8, "ry1": 2.8, "color": leaf, "material": "foliage"},
                    {"type": "cone", "cx": 16.5, "cy": 16.5, "z0": 31, "z1": 46, "rx0": 6.8, "ry0": 6.8, "rx1": 1.2, "ry1": 1.2, "color": leaf, "material": "foliage"}
                ]
            })
        }
        "oak" => {
            let leaf = p_col("green");
            json!({
                "title": title,
                "size": [38, 38, 46],
                "primitives": [
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 0, "z1": 2, "rx": 9.5, "ry": 9.5, "color": "grass", "material": "foliage"},
                    {"type": "cone", "cx": 18.5, "cy": 18.5, "z0": 1, "z1": 25, "rx0": 4.2, "ry0": 4.2, "rx1": 2.4, "ry1": 2.4, "color": "wood", "material": "wood"},
                    {"type": "sphere", "cx": 18.5, "cy": 18.5, "cz": 28.0, "rx": 11.0, "ry": 11.0, "rz": 9.5, "color": leaf, "material": "foliage"},
                    {"type": "sphere", "cx": 13.5, "cy": 21.5, "cz": 25.0, "rx": 7.5, "ry": 7.5, "rz": 6.8, "color": leaf, "material": "foliage", "mirror_x": true},
                    {"type": "sphere", "cx": 18.5, "cy": 13.5, "cz": 30.0, "rx": 7.0, "ry": 7.0, "rz": 6.5, "color": leaf, "material": "foliage"},
                    {"type": "sphere", "cx": 18.5, "cy": 18.5, "cz": 37.5, "rx": 7.2, "ry": 7.2, "rz": 6.5, "color": leaf, "material": "foliage"}
                ]
            })
        }
        "sunflower" => {
            let petal = p_col("yellow");
            json!({
                "title": title,
                "size": [28, 28, 36],
                "primitives": [
                    {"type": "cylinder", "cx": 13.5, "cy": 13.5, "z0": 0, "z1": 2, "rx": 7.0, "ry": 7.0, "color": "grass", "material": "foliage"},
                    {"type": "cylinder", "cx": 13.5, "cy": 13.5, "z0": 2, "z1": 24, "rx": 1.6, "ry": 1.6, "color": "green", "material": "foliage"},
                    {"type": "box", "x0": 7, "y0": 12, "z0": 10, "x1": 13, "y1": 16, "z1": 13, "color": "green", "material": "foliage", "mirror_x": true},
                    {"type": "sphere", "cx": 13.5, "cy": 15.0, "cz": 27.0, "rx": 6.8, "ry": 3.0, "rz": 6.8, "color": petal, "material": "foliage"},
                    {"type": "sphere", "cx": 13.5, "cy": 16.5, "cz": 27.0, "rx": 3.8, "ry": 2.4, "rz": 3.8, "color": "dark_wood"}
                ]
            })
        }
        "wheat" => {
            let grain = p_col("gold");
            json!({
                "title": title,
                "size": [32, 32, 24],
                "primitives": [
                    {"type": "box", "x0": 2, "y0": 2, "z0": 0, "x1": 30, "y1": 30, "z1": 2, "color": "brown", "material": "stone"},
                    {"type": "box", "x0": 5, "y0": 5, "z0": 2, "x1": 9, "y1": 9, "z1": 18, "color": grain, "material": "foliage", "mirror_x": true},
                    {"type": "box", "x0": 12, "y0": 7, "z0": 2, "x1": 20, "y1": 12, "z1": 20, "color": grain, "material": "foliage"},
                    {"type": "box", "x0": 6, "y0": 14, "z0": 2, "x1": 11, "y1": 19, "z1": 19, "color": grain, "material": "foliage", "mirror_x": true},
                    {"type": "box", "x0": 13, "y0": 17, "z0": 2, "x1": 19, "y1": 23, "z1": 21, "color": grain, "material": "foliage"},
                    {"type": "box", "x0": 5, "y0": 23, "z0": 2, "x1": 10, "y1": 27, "z1": 18, "color": grain, "material": "foliage", "mirror_x": true}
                ]
            })
        }
        "mushroom" => {
            let cap = p_col("red");
            json!({
                "title": title,
                "size": [36, 36, 38],
                "primitives": [
                    {"type": "cylinder", "cx": 17.5, "cy": 17.5, "z0": 0, "z1": 2, "rx": 8.5, "ry": 8.5, "color": "grass", "material": "foliage"},
                    {"type": "cone", "cx": 17.5, "cy": 17.5, "z0": 1, "z1": 21, "rx0": 5.0, "ry0": 5.0, "rx1": 3.8, "ry1": 3.8, "color": "marble", "material": "stone"},
                    {"type": "sphere", "cx": 17.5, "cy": 21.0, "cz": 21.5, "rx": 12.5, "ry": 12.5, "rz": 11.5, "color": cap, "material": "fur"},
                    {"type": "clear_box", "x0": 0, "y0": 0, "z0": 0, "x1": 36, "y1": 36, "z1": 20},
                    {"type": "cone", "cx": 17.5, "cy": 17.5, "z0": 1, "z1": 20, "rx0": 5.0, "ry0": 5.0, "rx1": 3.8, "ry1": 3.8, "color": "marble"},
                    {"type": "sphere", "cx": 11.0, "cy": 22.0, "cz": 28.0, "rx": 2.2, "ry": 2.2, "rz": 1.8, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "sphere", "cx": 17.5, "cy": 28.5, "cz": 27.5, "rx": 2.2, "ry": 2.2, "rz": 1.8, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        "car" => {
            let body = p_col("red");
            let stripe = s_col("white");
            json!({
                "title": title,
                "size": [30, 50, 26],
                "primitives": [
                    {"type": "box", "x0": 3, "y0": 8, "z0": 0, "x1": 6, "y1": 16, "z1": 8, "color": "black", "mirror_x": true},
                    {"type": "box", "x0": 3, "y0": 34, "z0": 0, "x1": 6, "y1": 42, "z1": 8, "color": "black", "mirror_x": true},
                    {"type": "box", "x0": 5, "y0": 4, "z0": 3, "x1": 25, "y1": 46, "z1": 12, "color": body, "material": "metal"},
                    {"type": "box", "x0": 13, "y0": 4, "z0": 11, "x1": 17, "y1": 46, "z1": 13, "color": stripe},
                    {"type": "box", "x0": 7, "y0": 14, "z0": 12, "x1": 23, "y1": 32, "z1": 19, "color": "sky", "material": "glass"},
                    {"type": "box", "x0": 8, "y0": 15, "z0": 18, "x1": 22, "y1": 30, "z1": 21, "color": body, "material": "metal"},
                    {"type": "box", "x0": 6, "y0": 4, "z0": 15, "x1": 24, "y1": 9, "z1": 17, "color": body, "material": "metal"},
                    {"type": "box", "x0": 7, "y0": 45, "z0": 5, "x1": 11, "y1": 48, "z1": 9, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "box", "x0": 7, "y0": 2, "z0": 6, "x1": 12, "y1": 5, "z1": 9, "color": "neon_pink", "emissive": true, "mirror_x": true}
                ]
            })
        }
        "bus" => {
            let body = p_col("yellow");
            let stripe = s_col("blue");
            json!({
                "title": title,
                "size": [30, 52, 34],
                "primitives": [
                    {"type": "box", "x0": 3, "y0": 9, "z0": 0, "x1": 6, "y1": 17, "z1": 8, "color": "black", "mirror_x": true},
                    {"type": "box", "x0": 3, "y0": 35, "z0": 0, "x1": 6, "y1": 43, "z1": 8, "color": "black", "mirror_x": true},
                    {"type": "box", "x0": 5, "y0": 4, "z0": 4, "x1": 25, "y1": 48, "z1": 28, "color": body, "material": "metal"},
                    {"type": "box", "x0": 5, "y0": 4, "z0": 11, "x1": 25, "y1": 48, "z1": 14, "color": stripe},
                    {"type": "box", "x0": 4, "y0": 9, "z0": 16, "x1": 26, "y1": 44, "z1": 24, "color": "sky", "material": "glass"},
                    {"type": "box", "x0": 7, "y0": 47, "z0": 16, "x1": 23, "y1": 49, "z1": 24, "color": "sky", "material": "glass"},
                    {"type": "box", "x0": 7, "y0": 47, "z0": 6, "x1": 11, "y1": 49, "z1": 9, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "box", "x0": 10, "y0": 14, "z0": 28, "x1": 20, "y1": 34, "z1": 31, "color": "white", "material": "metal"}
                ]
            })
        }
        "train" => {
            let hull = p_col("black");
            let trim = s_col("red");
            json!({
                "title": title,
                "size": [30, 52, 36],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 5, "z0": 0, "x1": 26, "y1": 47, "z1": 7, "color": trim, "material": "metal"},
                    {"type": "cylinder", "cx": 14.5, "cy": 28.0, "z0": 7, "z1": 38, "rx": 6.5, "ry": 6.5, "color": hull, "material": "metal"},
                    {"type": "box", "x0": 7, "y0": 18, "z0": 7, "x1": 23, "y1": 44, "z1": 22, "color": hull, "material": "metal"},
                    {"type": "box", "x0": 6, "y0": 5, "z0": 7, "x1": 24, "y1": 18, "z1": 30, "color": hull, "material": "metal"},
                    {"type": "box", "x0": 7, "y0": 8, "z0": 20, "x1": 23, "y1": 19, "z1": 26, "color": "neon_yellow", "emissive": true},
                    {"type": "cylinder", "cx": 14.5, "cy": 38.0, "z0": 20, "z1": 33, "rx": 3.0, "ry": 3.0, "color": "dark_gray", "material": "metal"},
                    {"type": "box", "x0": 12, "y0": 43, "z0": 14, "x1": 18, "y1": 46, "z1": 19, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        "carriage" => {
            let body = p_col("red");
            let wood = s_col("wood");
            json!({
                "title": title,
                "size": [34, 58, 36],
                "primitives": [
                    // 前方拉車駿馬（y: 34..54，車頭朝 +Y）
                    {"type": "box", "x0": 13, "y0": 37, "z0": 0, "x1": 15, "y1": 40, "z1": 11, "color": "white", "material": "fur", "mirror_x": true},
                    {"type": "box", "x0": 13, "y0": 47, "z0": 0, "x1": 15, "y1": 50, "z1": 11, "color": "white", "material": "fur", "mirror_x": true},
                    {"type": "sphere", "cx": 16.5, "cy": 43.5, "cz": 12.5, "rx": 4.2, "ry": 8.0, "rz": 4.2, "color": "white", "material": "fur"},
                    {"type": "line", "p0": [16.5, 48.0, 14.5], "p1": [16.5, 52.0, 22.0], "radius": 2.6, "color": "white", "material": "fur"},
                    {"type": "box", "x0": 14, "y0": 50, "z0": 20, "x1": 19, "y1": 56, "z1": 25, "color": "white", "material": "fur"},
                    {"type": "line", "p0": [16.5, 46.5, 17.0], "p1": [16.5, 50.5, 25.0], "radius": 1.3, "color": "gold", "material": "mane"},
                    // 木質拉車轅桿連接車身與馬匹
                    {"type": "line", "p0": [11.0, 27.0, 9.0], "p1": [13.0, 44.0, 11.5], "radius": 1.0, "color": wood, "material": "wood", "mirror_x": true},
                    // 後方古典車廂與四輪（y: 4..30）
                    {"type": "cylinder", "cx": 6.5, "cy": 10.0, "z0": 0, "z1": 10, "rx": 2.2, "ry": 5.0, "color": "dark_wood", "material": "wood", "mirror_x": true},
                    {"type": "cylinder", "cx": 6.5, "cy": 24.0, "z0": 0, "z1": 10, "rx": 2.2, "ry": 5.0, "color": "dark_wood", "material": "wood", "mirror_x": true},
                    {"type": "box", "x0": 8, "y0": 5, "z0": 5, "x1": 26, "y1": 29, "z1": 9, "color": wood, "material": "wood"},
                    // 車夫前座
                    {"type": "box", "x0": 10, "y0": 24, "z0": 9, "x1": 24, "y1": 30, "z1": 15, "color": wood, "material": "wood"},
                    // 華麗車廂主體、金邊與車窗
                    {"type": "box", "x0": 8, "y0": 6, "z0": 9, "x1": 26, "y1": 24, "z1": 25, "color": body, "material": "wood"},
                    {"type": "box", "x0": 7, "y0": 10, "z0": 15, "x1": 27, "y1": 20, "z1": 21, "color": "sky", "material": "glass"},
                    {"type": "box", "x0": 7, "y0": 5, "z0": 25, "x1": 27, "y1": 25, "z1": 29, "color": "gold", "material": "metal"},
                    {"type": "box", "x0": 9, "y0": 7, "z0": 29, "x1": 25, "y1": 23, "z1": 33, "color": body, "material": "fabric"},
                    // 車側夜光提燈
                    {"type": "box", "x0": 7, "y0": 22, "z0": 16, "x1": 9, "y1": 25, "z1": 20, "color": "neon_yellow", "emissive": true, "mirror_x": true}
                ]
            })
        }
        "sailboat" => {
            let wood = p_col("wood");
            let sail = s_col("white");
            json!({
                "title": title,
                "size": [34, 46, 44],
                "primitives": [
                    {"type": "box", "x0": 9, "y0": 7, "z0": 0, "x1": 25, "y1": 36, "z1": 8, "color": wood, "material": "wood"},
                    {"type": "pyramid", "cx": 16.5, "cy": 39.0, "z0": 0, "z1": 9, "w0": 14.0, "l0": 12.0, "w1": 3.0, "l1": 3.0, "color": wood, "material": "wood"},
                    {"type": "box", "x0": 10, "y0": 7, "z0": 8, "x1": 24, "y1": 15, "z1": 14, "color": "dark_wood", "material": "wood"},
                    {"type": "box", "x0": 15, "y0": 5, "z0": 10, "x1": 18, "y1": 7, "z1": 13, "color": "neon_yellow", "emissive": true},
                    {"type": "cylinder", "cx": 16.5, "cy": 22.0, "z0": 8, "z1": 42, "rx": 1.6, "ry": 1.6, "color": "dark_wood", "material": "wood"},
                    {"type": "box", "x0": 6, "y0": 22, "z0": 12, "x1": 27, "y1": 26, "z1": 36, "color": sail, "material": "fabric"},
                    {"type": "box", "x0": 16, "y0": 22, "z0": 38, "x1": 24, "y1": 24, "z1": 43, "color": "red"}
                ]
            })
        }
        "balloon" => {
            let dome = p_col("red");
            let stripe = s_col("yellow");
            json!({
                "title": title,
                "size": [36, 36, 48],
                "primitives": [
                    {"type": "box", "x0": 14, "y0": 14, "z0": 0, "x1": 22, "y1": 22, "z1": 7, "color": "wood", "material": "wood"},
                    {"type": "line", "p0": [14.0, 14.0, 7.0], "p1": [13.0, 13.0, 17.0], "radius": 1.1, "color": "dark_wood", "mirror_x": true},
                    {"type": "line", "p0": [14.0, 21.0, 7.0], "p1": [13.0, 22.0, 17.0], "radius": 1.1, "color": "dark_wood", "mirror_x": true},
                    {"type": "sphere", "cx": 17.5, "cy": 17.5, "cz": 14.0, "rx": 2.2, "ry": 2.2, "rz": 2.2, "color": "neon_yellow", "emissive": true},
                    {"type": "cone", "cx": 17.5, "cy": 17.5, "z0": 16, "z1": 26, "rx0": 5.5, "ry0": 5.5, "rx1": 12.0, "ry1": 12.0, "color": stripe, "material": "fabric"},
                    {"type": "sphere", "cx": 17.5, "cy": 17.5, "cz": 33.0, "rx": 13.5, "ry": 13.5, "rz": 13.5, "color": dome, "material": "fabric"},
                    {"type": "cylinder", "cx": 17.5, "cy": 17.5, "z0": 29, "z1": 36, "rx": 13.8, "ry": 13.8, "color": stripe, "material": "fabric"}
                ]
            })
        }
        "spaceship" => {
            let hull = p_col("white");
            let accent = s_col("blue");
            json!({
                "title": title,
                "size": [42, 46, 30],
                "primitives": [
                    {"type": "box", "x0": 15, "y0": 8, "z0": 4, "x1": 27, "y1": 38, "z1": 14, "color": hull, "material": "metal"},
                    {"type": "pyramid", "cx": 20.5, "cy": 41.0, "z0": 4, "z1": 12, "w0": 12.0, "l0": 10.0, "w1": 3.0, "l1": 3.0, "color": hull, "material": "metal"},
                    {"type": "box", "x0": 4, "y0": 9, "z0": 5, "x1": 38, "y1": 22, "z1": 9, "color": accent, "material": "metal"},
                    {"type": "box", "x0": 4, "y0": 10, "z0": 6, "x1": 6, "y1": 24, "z1": 13, "color": "neon_cyan", "emissive": true, "mirror_x": true},
                    {"type": "sphere", "cx": 20.5, "cy": 25.0, "cz": 15.0, "rx": 4.5, "ry": 6.8, "rz": 3.8, "color": "neon_cyan", "emissive": true},
                    {"type": "box", "x0": 15, "y0": 3, "z0": 6, "x1": 18, "y1": 8, "z1": 10, "color": "neon_cyan", "emissive": true, "mirror_x": true}
                ]
            })
        }
        "lamp" => {
            let post = p_col("dark_gray");
            json!({
                "title": title,
                "size": [20, 20, 38],
                "primitives": [
                    {"type": "cylinder", "cx": 9.5, "cy": 9.5, "z0": 0, "z1": 5, "rx": 3.6, "ry": 3.6, "color": post, "material": "metal"},
                    {"type": "cylinder", "cx": 9.5, "cy": 9.5, "z0": 5, "z1": 28, "rx": 1.6, "ry": 1.6, "color": post, "material": "metal"},
                    {"type": "box", "x0": 7, "y0": 7, "z0": 28, "x1": 13, "y1": 13, "z1": 35, "color": "neon_yellow", "emissive": true},
                    {"type": "pyramid", "cx": 9.5, "cy": 9.5, "z0": 35, "z1": 38, "w0": 8.0, "l0": 8.0, "w1": 2.0, "l1": 2.0, "color": post, "material": "metal"}
                ]
            })
        }
        "campfire" => {
            json!({
                "title": title,
                "size": [32, 32, 30],
                "primitives": [
                    {"type": "cylinder", "cx": 15.5, "cy": 15.5, "z0": 0, "z1": 4, "rx": 11.5, "ry": 11.5, "hollow": true, "wall": 3.0, "color": "stone", "material": "stone"},
                    {"type": "line", "p0": [8.0, 8.0, 2.5], "p1": [23.0, 23.0, 6.5], "radius": 2.0, "color": "dark_wood", "material": "wood"},
                    {"type": "line", "p0": [8.0, 23.0, 2.5], "p1": [23.0, 8.0, 6.5], "radius": 2.0, "color": "wood", "material": "wood"},
                    {"type": "cone", "cx": 15.5, "cy": 15.5, "z0": 5, "z1": 28, "rx0": 6.5, "ry0": 6.5, "rx1": 1.4, "ry1": 1.4, "color": "orange", "emissive": true},
                    {"type": "cone", "cx": 15.5, "cy": 15.5, "z0": 5, "z1": 21, "rx0": 4.2, "ry0": 4.2, "rx1": 1.2, "ry1": 1.2, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        "gate" => {
            let bar = p_col("red");
            json!({
                "title": title,
                "size": [40, 20, 28],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 7, "z0": 0, "x1": 9, "y1": 13, "z1": 18, "color": "dark_gray", "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 5, "y0": 8, "z0": 18, "x1": 8, "y1": 12, "z1": 22, "color": "neon_pink", "emissive": true, "mirror_x": true},
                    {"type": "box", "x0": 6, "y0": 9, "z0": 10, "x1": 18, "y1": 11, "z1": 13, "color": bar, "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 16, "y0": 9, "z0": 10, "x1": 24, "y1": 11, "z1": 13, "color": "white", "material": "metal"}
                ]
            })
        }
        "crystal" => {
            let gem = p_col("neon_cyan");
            json!({
                "title": title,
                "size": [34, 34, 42],
                "primitives": [
                    {"type": "cylinder", "cx": 16.5, "cy": 16.5, "z0": 0, "z1": 5, "rx": 11.5, "ry": 11.5, "color": "dark_stone", "material": "stone"},
                    {"type": "pyramid", "cx": 16.5, "cy": 16.5, "z0": 4, "z1": 40, "w0": 9.0, "l0": 9.0, "w1": 1.8, "l1": 1.8, "color": gem, "material": "crystal", "emissive": true},
                    {"type": "pyramid", "cx": 10.5, "cy": 19.0, "z0": 3, "z1": 28, "w0": 5.5, "l0": 5.5, "w1": 1.4, "l1": 1.4, "color": gem, "material": "crystal", "emissive": true, "mirror_x": true},
                    {"type": "pyramid", "cx": 17.5, "cy": 10.5, "z0": 3, "z1": 31, "w0": 5.8, "l0": 5.8, "w1": 1.4, "l1": 1.4, "color": gem, "material": "crystal", "emissive": true}
                ]
            })
        }
        "bench" => {
            let wood = p_col("wood");
            json!({
                "title": title,
                "size": [34, 20, 20],
                "primitives": [
                    {"type": "box", "x0": 5, "y0": 5, "z0": 0, "x1": 8, "y1": 15, "z1": 7, "color": "dark_gray", "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 4, "y0": 6, "z0": 7, "x1": 30, "y1": 15, "z1": 9, "color": wood, "material": "wood"},
                    {"type": "box", "x0": 4, "y0": 5, "z0": 9, "x1": 30, "y1": 7, "z1": 17, "color": wood, "material": "wood"}
                ]
            })
        }
        "chest" => {
            let wood = p_col("wood");
            let gold = s_col("gold");
            json!({
                "title": title,
                "size": [34, 26, 26],
                "primitives": [
                    {"type": "box", "x0": 5, "y0": 4, "z0": 0, "x1": 29, "y1": 22, "z1": 14, "color": wood, "material": "wood"},
                    {"type": "box", "x0": 5, "y0": 4, "z0": 0, "x1": 8, "y1": 22, "z1": 15, "color": gold, "material": "metal", "mirror_x": true},
                    {"type": "pyramid", "cx": 16.5, "cy": 12.5, "z0": 14, "z1": 24, "w0": 24.0, "l0": 18.0, "w1": 24.0, "l1": 6.0, "color": wood, "material": "wood"},
                    {"type": "box", "x0": 14, "y0": 21, "z0": 10, "x1": 20, "y1": 24, "z1": 17, "color": gold, "material": "metal"},
                    {"type": "box", "x0": 16, "y0": 22, "z0": 12, "x1": 18, "y1": 25, "z1": 15, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "throne" => {
            let gold = p_col("gold");
            let velvet = s_col("red");
            json!({
                "title": title,
                "size": [32, 30, 42],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 4, "z0": 0, "x1": 28, "y1": 26, "z1": 4, "color": "marble", "material": "stone"},
                    {"type": "box", "x0": 8, "y0": 7, "z0": 4, "x1": 24, "y1": 22, "z1": 14, "color": gold, "material": "metal"},
                    {"type": "box", "x0": 10, "y0": 9, "z0": 13, "x1": 22, "y1": 21, "z1": 16, "color": velvet, "material": "fabric"},
                    {"type": "box", "x0": 7, "y0": 9, "z0": 14, "x1": 10, "y1": 22, "z1": 20, "color": gold, "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 8, "y0": 6, "z0": 14, "x1": 24, "y1": 10, "z1": 36, "color": gold, "material": "metal"},
                    {"type": "box", "x0": 11, "y0": 8, "z0": 16, "z1": 32, "x1": 21, "y1": 11, "color": velvet, "material": "fabric"},
                    {"type": "sphere", "cx": 15.5, "cy": 8.0, "cz": 38.0, "rx": 3.5, "ry": 2.5, "rz": 3.5, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "chandelier" => {
            let frame = p_col("gold");
            json!({
                "title": title,
                "size": [32, 32, 34],
                "primitives": [
                    {"type": "cylinder", "cx": 15.5, "cy": 15.5, "z0": 0, "z1": 24, "rx": 2.0, "ry": 2.0, "color": frame, "material": "metal"},
                    {"type": "cylinder", "cx": 15.5, "cy": 15.5, "z0": 20, "z1": 24, "rx": 11.0, "ry": 11.0, "hollow": true, "wall": 2.5, "color": frame, "material": "metal"},
                    {"type": "box", "x0": 5, "y0": 14, "z0": 24, "x1": 8, "y1": 17, "z1": 31, "color": "neon_yellow", "emissive": true, "mirror_x": true},
                    {"type": "box", "x0": 14, "y0": 5, "z0": 24, "x1": 17, "y1": 8, "z1": 31, "color": "neon_yellow", "emissive": true},
                    {"type": "box", "x0": 14, "y0": 23, "z0": 24, "x1": 17, "y1": 26, "z1": 31, "color": "neon_yellow", "emissive": true},
                    {"type": "sphere", "cx": 15.5, "cy": 15.5, "cz": 27.0, "rx": 3.2, "ry": 3.2, "rz": 3.5, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "pillar" => {
            let stone = p_col("marble");
            json!({
                "title": title,
                "size": [24, 24, 48],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 4, "z0": 0, "x1": 20, "y1": 20, "z1": 6, "color": stone, "material": "stone"},
                    {"type": "cylinder", "cx": 11.5, "cy": 11.5, "z0": 6, "z1": 42, "rx": 5.2, "ry": 5.2, "color": stone, "material": "stone"},
                    {"type": "box", "x0": 4, "y0": 4, "z0": 42, "x1": 20, "y1": 20, "z1": 47, "color": "gold", "material": "metal"}
                ]
            })
        }
        "statue" => {
            let stone = p_col("marble");
            json!({
                "title": title,
                "size": [28, 28, 44],
                "primitives": [
                    {"type": "box", "x0": 5, "y0": 5, "z0": 0, "x1": 23, "y1": 23, "z1": 10, "color": "dark_stone", "material": "stone"},
                    {"type": "cylinder", "cx": 13.5, "cy": 13.5, "z0": 10, "z1": 28, "rx": 5.5, "ry": 4.8, "color": stone, "material": "stone"},
                    {"type": "sphere", "cx": 13.5, "cy": 13.5, "cz": 34.0, "rx": 4.5, "ry": 4.5, "rz": 4.5, "color": stone, "material": "stone"},
                    {"type": "box", "x0": 6, "y0": 15, "z0": 18, "x1": 9, "y1": 17, "z1": 40, "color": "gold", "material": "metal"}
                ]
            })
        }
        "counter" => {
            let wood = p_col("wood");
            let top = s_col("marble");
            json!({
                "title": title,
                "size": [40, 24, 28],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 6, "z0": 0, "x1": 36, "y1": 20, "z1": 16, "color": wood, "material": "wood"},
                    {"type": "box", "x0": 3, "y0": 5, "z0": 16, "x1": 37, "y1": 21, "z1": 19, "color": top, "material": "stone"},
                    {"type": "box", "x0": 14, "y0": 10, "z0": 19, "x1": 24, "y1": 16, "z1": 26, "color": "dark_gray", "material": "metal"},
                    {"type": "box", "x0": 15, "y0": 15, "z0": 21, "x1": 23, "y1": 17, "z1": 25, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "shelf" => {
            let frame = p_col("wood");
            json!({
                "title": title,
                "size": [38, 20, 36],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 4, "z0": 0, "x1": 7, "y1": 16, "z1": 34, "color": frame, "material": "wood", "mirror_x": true},
                    {"type": "box", "x0": 6, "y0": 4, "z0": 2, "x1": 32, "y1": 16, "z1": 5, "color": frame, "material": "wood"},
                    {"type": "box", "x0": 6, "y0": 4, "z0": 14, "x1": 32, "y1": 16, "z1": 17, "color": frame, "material": "wood"},
                    {"type": "box", "x0": 6, "y0": 4, "z0": 26, "x1": 32, "y1": 16, "z1": 29, "color": frame, "material": "wood"},
                    {"type": "box", "x0": 9, "y0": 7, "z0": 5, "x1": 14, "y1": 14, "z1": 13, "color": "red"},
                    {"type": "box", "x0": 17, "y0": 7, "z0": 5, "x1": 23, "y1": 14, "z1": 13, "color": "sky"},
                    {"type": "box", "x0": 10, "y0": 7, "z0": 17, "x1": 16, "y1": 14, "z1": 25, "color": "yellow"},
                    {"type": "box", "x0": 20, "y0": 7, "z0": 17, "x1": 27, "y1": 14, "z1": 25, "color": "green"}
                ]
            })
        }
        "vending" => {
            let body = p_col("red");
            json!({
                "title": title,
                "size": [28, 22, 42],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 4, "z0": 0, "x1": 24, "y1": 18, "z1": 40, "color": body, "material": "metal"},
                    {"type": "box", "x0": 7, "y0": 16, "z0": 15, "x1": 21, "y1": 19, "z1": 36, "color": "neon_cyan", "emissive": true},
                    {"type": "box", "x0": 7, "y0": 16, "z0": 4, "x1": 18, "y1": 19, "z1": 10, "color": "dark_gray", "material": "metal"},
                    {"type": "box", "x0": 19, "y0": 17, "z0": 17, "x1": 22, "y1": 19, "z1": 24, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        "display_case" => {
            let base = p_col("marble");
            json!({
                "title": title,
                "size": [38, 22, 28],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 4, "z0": 0, "x1": 34, "y1": 18, "z1": 12, "color": base, "material": "stone"},
                    {"type": "box", "x0": 5, "y0": 5, "z0": 12, "x1": 33, "y1": 17, "z1": 25, "color": "sky", "material": "glass"},
                    {"type": "sphere", "cx": 12.0, "cy": 11.0, "cz": 16.0, "rx": 3.0, "ry": 3.0, "rz": 3.0, "color": "sakura"},
                    {"type": "sphere", "cx": 24.0, "cy": 11.0, "cz": 16.0, "rx": 3.0, "ry": 3.0, "rz": 3.0, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        "conveyor" => {
            let frame = p_col("dark_gray");
            json!({
                "title": title,
                "size": [44, 24, 22],
                "primitives": [
                    {"type": "box", "x0": 5, "y0": 6, "z0": 0, "x1": 9, "y1": 18, "z1": 10, "color": frame, "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 3, "y0": 5, "z0": 10, "x1": 41, "y1": 19, "z1": 14, "color": "yellow", "material": "metal"},
                    {"type": "box", "x0": 4, "y0": 7, "z0": 13, "x1": 40, "y1": 17, "z1": 15, "color": "black"},
                    {"type": "box", "x0": 10, "y0": 8, "z0": 15, "x1": 16, "y1": 16, "z1": 21, "color": "wood", "material": "wood"},
                    {"type": "box", "x0": 26, "y0": 8, "z0": 15, "x1": 32, "y1": 16, "z1": 21, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "robot_arm" => {
            let body = p_col("orange");
            json!({
                "title": title,
                "size": [32, 32, 42],
                "primitives": [
                    {"type": "cylinder", "cx": 15.5, "cy": 15.5, "z0": 0, "z1": 8, "rx": 7.5, "ry": 7.5, "color": "dark_gray", "material": "metal"},
                    {"type": "line", "p0": [15.5, 15.5, 7.0], "p1": [15.5, 10.0, 26.0], "radius": 2.6, "color": body, "material": "metal"},
                    {"type": "line", "p0": [15.5, 10.0, 26.0], "p1": [15.5, 24.0, 34.0], "radius": 2.2, "color": body, "material": "metal"},
                    {"type": "sphere", "cx": 15.5, "cy": 10.0, "cz": 26.0, "rx": 3.2, "ry": 3.2, "rz": 3.2, "color": "silver", "material": "metal"},
                    {"type": "box", "x0": 13, "y0": 23, "z0": 32, "x1": 18, "y1": 28, "z1": 36, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "furnace" => {
            let hull = p_col("dark_stone");
            json!({
                "title": title,
                "size": [36, 34, 44],
                "primitives": [
                    {"type": "box", "x0": 5, "y0": 5, "z0": 0, "x1": 31, "y1": 29, "z1": 30, "color": hull, "material": "metal"},
                    {"type": "box", "x0": 11, "y0": 26, "z0": 5, "x1": 25, "y1": 30, "z1": 17, "color": "orange", "emissive": true},
                    {"type": "box", "x0": 14, "y0": 27, "z0": 7, "x1": 22, "y1": 31, "z1": 14, "color": "neon_yellow", "emissive": true},
                    {"type": "cylinder", "cx": 18.0, "cy": 14.0, "z0": 30, "z1": 43, "rx": 4.5, "ry": 4.5, "color": "dark_gray", "material": "metal"}
                ]
            })
        }
        "crate" => {
            let wood = p_col("wood");
            json!({
                "title": title,
                "size": [30, 30, 28],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 5, "z0": 0, "x1": 15, "y1": 16, "z1": 12, "color": wood, "material": "wood"},
                    {"type": "box", "x0": 16, "y0": 6, "z0": 0, "x1": 26, "y1": 17, "z1": 11, "color": "dark_wood", "material": "wood"},
                    {"type": "box", "x0": 7, "y0": 16, "z0": 0, "x1": 19, "y1": 26, "z1": 12, "color": wood, "material": "wood"},
                    {"type": "box", "x0": 9, "y0": 9, "z0": 12, "x1": 21, "y1": 21, "z1": 24, "color": wood, "material": "wood"}
                ]
            })
        }
        "forklift" => {
            let body = p_col("yellow");
            json!({
                "title": title,
                "size": [28, 42, 34],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 9, "z0": 0, "x1": 7, "y1": 15, "z1": 6, "color": "black", "mirror_x": true},
                    {"type": "box", "x0": 4, "y0": 26, "z0": 0, "x1": 7, "y1": 32, "z1": 6, "color": "black", "mirror_x": true},
                    {"type": "box", "x0": 6, "y0": 6, "z0": 3, "x1": 22, "y1": 28, "z1": 14, "color": body, "material": "metal"},
                    {"type": "box", "x0": 7, "y0": 10, "z0": 14, "x1": 21, "y1": 24, "z1": 26, "color": "dark_gray", "material": "metal"},
                    {"type": "box", "x0": 8, "y0": 28, "z0": 2, "x1": 11, "y1": 31, "z1": 31, "color": "dark_gray", "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 9, "y0": 31, "z0": 3, "x1": 12, "y1": 41, "z1": 6, "color": "silver", "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 12, "y0": 15, "z0": 26, "x1": 16, "y1": 19, "z1": 30, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        "office_desk" => {
            let wood = p_col("wood");
            json!({
                "title": title,
                "size": [38, 24, 30],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 5, "z0": 0, "x1": 8, "y1": 19, "z1": 13, "color": "dark_gray", "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 3, "y0": 4, "z0": 13, "x1": 35, "y1": 20, "z1": 16, "color": wood, "material": "wood"},
                    {"type": "box", "x0": 12, "y0": 7, "z0": 16, "x1": 26, "y1": 10, "z1": 27, "color": "black", "material": "metal"},
                    {"type": "box", "x0": 13, "y0": 9, "z0": 17, "x1": 25, "y1": 11, "z1": 26, "color": "neon_cyan", "emissive": true},
                    {"type": "box", "x0": 13, "y0": 13, "z0": 16, "x1": 24, "y1": 17, "z1": 17, "color": "silver"}
                ]
            })
        }
        "office_chair" => {
            let seat = p_col("navy");
            json!({
                "title": title,
                "size": [24, 24, 32],
                "primitives": [
                    {"type": "cylinder", "cx": 11.5, "cy": 11.5, "z0": 0, "z1": 3, "rx": 7.0, "ry": 7.0, "color": "black", "material": "metal"},
                    {"type": "cylinder", "cx": 11.5, "cy": 11.5, "z0": 3, "z1": 11, "rx": 1.8, "ry": 1.8, "color": "silver", "material": "metal"},
                    {"type": "box", "x0": 6, "y0": 6, "z0": 11, "x1": 18, "y1": 18, "z1": 14, "color": seat, "material": "fabric"},
                    {"type": "box", "x0": 6, "y0": 5, "z0": 14, "x1": 18, "y1": 8, "z1": 29, "color": seat, "material": "fabric"}
                ]
            })
        }
        "bookshelf" => {
            let wood = p_col("dark_wood");
            json!({
                "title": title,
                "size": [36, 18, 40],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 4, "z0": 0, "x1": 32, "y1": 14, "z1": 38, "color": wood, "material": "wood"},
                    {"type": "clear_box", "x0": 7, "y0": 7, "z0": 4, "x1": 29, "y1": 15, "z1": 15},
                    {"type": "clear_box", "x0": 7, "y0": 7, "z0": 18, "x1": 29, "y1": 15, "z1": 33},
                    {"type": "box", "x0": 8, "y0": 8, "z0": 4, "x1": 13, "y1": 13, "z1": 13, "color": "red"},
                    {"type": "box", "x0": 14, "y0": 8, "z0": 4, "x1": 20, "y1": 13, "z1": 14, "color": "blue"},
                    {"type": "box", "x0": 9, "y0": 8, "z0": 18, "x1": 16, "y1": 13, "z1": 28, "color": "green"},
                    {"type": "box", "x0": 18, "y0": 8, "z0": 18, "x1": 25, "y1": 13, "z1": 30, "color": "gold"}
                ]
            })
        }
        "meeting_table" => {
            let top = p_col("marble");
            json!({
                "title": title,
                "size": [48, 30, 22],
                "primitives": [
                    {"type": "cylinder", "cx": 13.0, "cy": 14.5, "z0": 0, "z1": 14, "rx": 3.5, "ry": 3.5, "color": "dark_gray", "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 5, "y0": 6, "z0": 14, "x1": 43, "y1": 24, "z1": 17, "color": top, "material": "stone"},
                    {"type": "box", "x0": 19, "y0": 12, "z0": 17, "x1": 29, "y1": 18, "z1": 20, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "water_cooler" => {
            json!({
                "title": title,
                "size": [22, 22, 36],
                "primitives": [
                    {"type": "box", "x0": 6, "y0": 6, "z0": 0, "x1": 16, "y1": 16, "z1": 22, "color": "white", "material": "metal"},
                    {"type": "box", "x0": 8, "y0": 14, "z0": 15, "x1": 14, "y1": 17, "z1": 18, "color": "neon_cyan", "emissive": true},
                    {"type": "cylinder", "cx": 10.5, "cy": 10.5, "z0": 22, "z1": 34, "rx": 4.2, "ry": 4.2, "color": "sky", "material": "glass"}
                ]
            })
        }
        "blackboard" => {
            let frame = p_col("wood");
            json!({
                "title": title,
                "size": [46, 20, 36],
                "primitives": [
                    {"type": "box", "x0": 6, "y0": 4, "z0": 0, "x1": 40, "y1": 16, "z1": 4, "color": frame, "material": "wood"},
                    {"type": "box", "x0": 4, "y0": 5, "z0": 11, "x1": 42, "y1": 9, "z1": 34, "color": frame, "material": "wood"},
                    {"type": "box", "x0": 6, "y0": 7, "z0": 13, "x1": 40, "y1": 10, "z1": 32, "color": "emerald", "material": "flat"},
                    {"type": "box", "x0": 10, "y0": 9, "z0": 20, "x1": 22, "y1": 11, "z1": 23, "color": "white", "shaded": false}
                ]
            })
        }
        "school_desk" => {
            let top = p_col("wood");
            json!({
                "title": title,
                "size": [28, 28, 24],
                "primitives": [
                    {"type": "box", "x0": 5, "y0": 14, "z0": 0, "x1": 8, "y1": 23, "z1": 13, "color": "gray", "material": "metal", "mirror_x": true},
                    {"type": "box", "x0": 4, "y0": 13, "z0": 13, "x1": 24, "y1": 24, "z1": 16, "color": top, "material": "wood"},
                    {"type": "box", "x0": 8, "y0": 4, "z0": 0, "x1": 20, "y1": 11, "z1": 9, "color": "blue", "material": "wood"},
                    {"type": "box", "x0": 8, "y0": 3, "z0": 9, "x1": 20, "y1": 6, "z1": 17, "color": "blue", "material": "wood"}
                ]
            })
        }
        "sofa" => {
            let fabric = p_col("sky");
            json!({
                "title": title,
                "size": [42, 26, 26],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 5, "z0": 0, "x1": 38, "y1": 22, "z1": 11, "color": fabric, "material": "fabric"},
                    {"type": "box", "x0": 4, "y0": 4, "z0": 11, "x1": 38, "y1": 10, "z1": 23, "color": fabric, "material": "fabric"},
                    {"type": "box", "x0": 4, "y0": 5, "z0": 11, "x1": 9, "y1": 22, "z1": 17, "color": fabric, "material": "fabric", "mirror_x": true},
                    {"type": "box", "x0": 11, "y0": 11, "z0": 11, "x1": 18, "y1": 16, "z1": 16, "color": "yellow", "material": "fabric", "mirror_x": true}
                ]
            })
        }
        "bed" => {
            let quilt = p_col("pink");
            json!({
                "title": title,
                "size": [38, 44, 24],
                "primitives": [
                    {"type": "box", "x0": 5, "y0": 5, "z0": 0, "x1": 33, "y1": 40, "z1": 7, "color": "wood", "material": "wood"},
                    {"type": "box", "x0": 5, "y0": 5, "z0": 7, "x1": 33, "y1": 9, "z1": 20, "color": "dark_wood", "material": "wood"},
                    {"type": "box", "x0": 7, "y0": 9, "z0": 7, "x1": 31, "y1": 38, "z1": 12, "color": "white", "material": "fabric"},
                    {"type": "box", "x0": 7, "y0": 18, "z0": 10, "x1": 31, "y1": 39, "z1": 14, "color": quilt, "material": "fabric"},
                    {"type": "box", "x0": 9, "y0": 10, "z0": 12, "x1": 17, "y1": 16, "z1": 15, "color": "yellow", "material": "fabric", "mirror_x": true}
                ]
            })
        }
        "tv_stand" => {
            let stand = p_col("dark_wood");
            json!({
                "title": title,
                "size": [40, 20, 32],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 5, "z0": 0, "x1": 36, "y1": 15, "z1": 11, "color": stand, "material": "wood"},
                    {"type": "box", "x0": 7, "y0": 8, "z0": 13, "x1": 33, "y1": 12, "z1": 30, "color": "black", "material": "metal"},
                    {"type": "box", "x0": 9, "y0": 10, "z0": 15, "x1": 31, "y1": 13, "z1": 28, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "dining_table" => {
            let wood = p_col("wood");
            json!({
                "title": title,
                "size": [38, 38, 22],
                "primitives": [
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 0, "z1": 13, "rx": 3.5, "ry": 3.5, "color": "dark_wood", "material": "wood"},
                    {"type": "cylinder", "cx": 18.5, "cy": 18.5, "z0": 13, "z1": 16, "rx": 12.0, "ry": 12.0, "color": wood, "material": "wood"},
                    {"type": "box", "x0": 4, "y0": 15, "z0": 0, "x1": 10, "y1": 22, "z1": 10, "color": "red", "material": "fabric", "mirror_x": true},
                    {"type": "box", "x0": 16, "y0": 16, "z0": 16, "x1": 21, "y1": 21, "z1": 20, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        "kitchen_stove" => {
            let cabinet = p_col("white");
            json!({
                "title": title,
                "size": [42, 24, 28],
                "primitives": [
                    {"type": "box", "x0": 4, "y0": 5, "z0": 0, "x1": 38, "y1": 19, "z1": 16, "color": cabinet, "material": "wood"},
                    {"type": "box", "x0": 3, "y0": 4, "z0": 16, "x1": 39, "y1": 20, "z1": 19, "color": "marble", "material": "stone"},
                    {"type": "box", "x0": 8, "y0": 17, "z0": 5, "x1": 17, "y1": 20, "z1": 13, "color": "dark_gray", "material": "metal"},
                    {"type": "cylinder", "cx": 12.5, "cy": 12.0, "z0": 19, "z1": 21, "rx": 3.2, "ry": 3.2, "color": "orange", "emissive": true},
                    {"type": "box", "x0": 24, "y0": 8, "z0": 19, "x1": 34, "y1": 16, "z1": 20, "color": "sky", "material": "glass"}
                ]
            })
        }
        "potted_plant" => {
            let leaf = p_col("green");
            json!({
                "title": title,
                "size": [26, 26, 36],
                "primitives": [
                    {"type": "cone", "cx": 12.5, "cy": 12.5, "z0": 0, "z1": 11, "rx0": 4.8, "ry0": 4.8, "rx1": 6.5, "ry1": 6.5, "color": "orange", "material": "stone"},
                    {"type": "cylinder", "cx": 12.5, "cy": 12.5, "z0": 10, "z1": 20, "rx": 2.0, "ry": 2.0, "color": "wood", "material": "wood"},
                    {"type": "sphere", "cx": 12.5, "cy": 12.5, "cz": 24.0, "rx": 8.0, "ry": 8.0, "rz": 7.5, "color": leaf, "material": "foliage"},
                    {"type": "sphere", "cx": 9.0, "cy": 14.0, "cz": 29.0, "rx": 4.5, "ry": 4.5, "rz": 4.5, "color": leaf, "material": "foliage", "mirror_x": true}
                ]
            })
        }
        "wooden_door" => {
            let door_col = p_col("wood");
            let frame_col = s_col("stone");
            json!({
                "title": title,
                "size": [30, 18, 40],
                "primitives": [
                    // 門口石階與門框
                    {"type": "box", "x0": 4, "y0": 5, "z0": 0, "x1": 26, "y1": 15, "z1": 3, "color": frame_col, "material": "stone"},
                    {"type": "box", "x0": 5, "y0": 6, "z0": 3, "x1": 9, "y1": 12, "z1": 34, "color": frame_col, "material": "stone", "mirror_x": true},
                    {"type": "box", "x0": 5, "y0": 6, "z0": 32, "x1": 25, "y1": 12, "z1": 37, "color": frame_col, "material": "stone"},
                    // 雙開木門主體與中央門縫裝飾
                    {"type": "box", "x0": 9, "y0": 7, "z0": 3, "x1": 21, "y1": 11, "z1": 32, "color": door_col, "material": "wood"},
                    {"type": "box", "x0": 14, "y0": 10, "z0": 3, "x1": 16, "y1": 12, "z1": 32, "color": "dark_wood", "material": "wood"},
                    // 雙側金色門把（朝 +Y 正面）
                    {"type": "box", "x0": 11, "y0": 11, "z0": 15, "x1": 13, "y1": 13, "z1": 19, "color": "gold", "material": "metal", "mirror_x": true},
                    // 門楣暖黃夜光門燈
                    {"type": "box", "x0": 13, "y0": 11, "z0": 34, "x1": 17, "y1": 14, "z1": 38, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        "portal_gate" => {
            let stone = p_col("dark_stone");
            let glow_col = s_col("neon_cyan");
            json!({
                "title": title,
                "size": [36, 18, 44],
                "primitives": [
                    // 符文底座與雙石柱
                    {"type": "box", "x0": 3, "y0": 4, "z0": 0, "x1": 33, "y1": 15, "z1": 4, "color": stone, "material": "stone"},
                    {"type": "box", "x0": 5, "y0": 6, "z0": 4, "x1": 10, "y1": 12, "z1": 36, "color": stone, "material": "stone", "mirror_x": true},
                    {"type": "box", "x0": 4, "y0": 5, "z0": 34, "x1": 32, "y1": 13, "z1": 40, "color": stone, "material": "stone"},
                    {"type": "box", "x0": 14, "y0": 7, "z0": 40, "x1": 22, "y1": 11, "z1": 43, "color": "gold", "material": "metal"},
                    // 兩側符文點綴
                    {"type": "box", "x0": 6, "y0": 11, "z0": 16, "x1": 9, "y1": 13, "z1": 24, "color": glow_col, "emissive": true, "mirror_x": true},
                    // 中央發光傳送光幕
                    {"type": "box", "x0": 10, "y0": 8, "z0": 4, "x1": 26, "y1": 10, "z1": 34, "color": glow_col, "material": "crystal", "emissive": true}
                ]
            })
        }
        "office_lady" => {
            let blazer = p_col("sakura");
            let skirt = s_col("navy");
            let hair = "brown";
            json!({
                "title": title,
                "size": [30, 30, 40],
                "primitives": [
                    // 高跟鞋與雙腿
                    {"type": "box", "x0": 11, "y0": 12, "z0": 0, "x1": 14, "y1": 17, "z1": 4, "color": skirt, "material": "fabric", "mirror_x": true},
                    {"type": "box", "x0": 11, "y0": 12, "z0": 4, "x1": 14, "y1": 16, "z1": 10, "color": "skin", "mirror_x": true},
                    // 深藍窄裙
                    {"type": "box", "x0": 10, "y0": 11, "z0": 9, "x1": 20, "y1": 18, "z1": 16, "color": skirt, "material": "fabric"},
                    // 白襯衫與合身西裝外套
                    {"type": "box", "x0": 10, "y0": 11, "z0": 16, "x1": 20, "y1": 18, "z1": 24, "color": blazer, "material": "fabric"},
                    {"type": "box", "x0": 13, "y0": 17, "z0": 18, "x1": 17, "y1": 19, "z1": 24, "color": "white", "material": "fabric"},
                    // 雙臂與右側通勤托特包
                    {"type": "box", "x0": 7, "y0": 12, "z0": 14, "x1": 10, "y1": 17, "z1": 23, "color": blazer, "material": "fabric", "mirror_x": true},
                    {"type": "box", "x0": 21, "y0": 11, "z0": 7, "x1": 25, "y1": 19, "z1": 15, "color": "wood", "material": "fabric"},
                    // 頭部與馬尾長髮
                    {"type": "sphere", "cx": 14.5, "cy": 14.5, "cz": 28.5, "rx": 5.0, "ry": 4.8, "rz": 4.9, "color": "skin"},
                    {"type": "sphere", "cx": 14.5, "cy": 13.0, "cz": 30.5, "rx": 5.3, "ry": 4.8, "rz": 4.8, "color": hair, "material": "fur"},
                    {"type": "box", "x0": 12, "y0": 7, "z0": 19, "x1": 18, "y1": 11, "z1": 31, "color": hair, "material": "fur"},
                    // 雙眼
                    {"type": "box", "x0": 11, "y0": 18, "z0": 27, "x1": 13, "y1": 20, "z1": 29, "color": "black", "shaded": false, "mirror_x": true}
                ]
            })
        }
        "bus_stop" => {
            let shelter = p_col("sky");
            let pole = s_col("blue");
            json!({
                "title": title,
                "size": [40, 24, 42],
                "primitives": [
                    // 水泥站台地基
                    {"type": "box", "x0": 2, "y0": 4, "z0": 0, "x1": 38, "y1": 20, "z1": 3, "color": "stone", "material": "stone"},
                    // 候車亭背板玻璃與支柱
                    {"type": "box", "x0": 5, "y0": 5, "z0": 3, "x1": 25, "y1": 8, "z1": 27, "color": shelter, "material": "glass"},
                    {"type": "box", "x0": 4, "y0": 4, "z0": 3, "x1": 7, "y1": 9, "z1": 29, "color": "dark_gray", "material": "metal"},
                    {"type": "box", "x0": 23, "y0": 4, "z0": 3, "x1": 26, "y1": 9, "z1": 29, "color": "dark_gray", "material": "metal"},
                    // 遮陽雨棚
                    {"type": "box", "x0": 3, "y0": 4, "z0": 28, "x1": 28, "y1": 18, "z1": 32, "color": pole, "material": "metal"},
                    // 候車長椅
                    {"type": "box", "x0": 8, "y0": 9, "z0": 3, "x1": 22, "y1": 14, "z1": 10, "color": "wood", "material": "wood"},
                    // 右側發光公車站牌柱
                    {"type": "cylinder", "cx": 33.0, "cy": 12.0, "z0": 3, "z1": 32, "rx": 1.6, "ry": 1.6, "color": "silver", "material": "metal"},
                    {"type": "box", "x0": 29, "y0": 10, "z0": 25, "x1": 37, "y1": 15, "z1": 37, "color": pole, "material": "metal"},
                    {"type": "box", "x0": 30, "y0": 14, "z0": 27, "x1": 36, "y1": 16, "z1": 35, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        "coffee_bar" => {
            let counter = p_col("dark_wood");
            let machine = s_col("silver");
            json!({
                "title": title,
                "size": [40, 24, 34],
                "primitives": [
                    // 吧台底座與大理石檯面
                    {"type": "box", "x0": 4, "y0": 5, "z0": 0, "x1": 36, "y1": 19, "z1": 15, "color": counter, "material": "wood"},
                    {"type": "box", "x0": 3, "y0": 4, "z0": 15, "x1": 37, "y1": 20, "z1": 18, "color": "marble", "material": "stone"},
                    // 義式咖啡機本體與指示燈
                    {"type": "box", "x0": 8, "y0": 7, "z0": 18, "x1": 21, "y1": 15, "z1": 30, "color": machine, "material": "metal"},
                    {"type": "box", "x0": 10, "y0": 14, "z0": 25, "x1": 19, "y1": 16, "z1": 28, "color": "neon_yellow", "emissive": true},
                    // 馬克杯與點心盤
                    {"type": "cylinder", "cx": 27.5, "cy": 13.0, "z0": 18, "z1": 24, "rx": 2.4, "ry": 2.4, "color": "white", "material": "stone"},
                    {"type": "cylinder", "cx": 32.5, "cy": 12.5, "z0": 18, "z1": 21, "rx": 2.8, "ry": 2.8, "color": "gold", "material": "stone"}
                ]
            })
        }
        "printer" => {
            let body = p_col("white");
            let trim = s_col("dark_gray");
            json!({
                "title": title,
                "size": [34, 28, 34],
                "primitives": [
                    // 下層紙匣底座
                    {"type": "box", "x0": 5, "y0": 6, "z0": 0, "x1": 29, "y1": 22, "z1": 14, "color": trim, "material": "metal"},
                    // 主體白色印表機身
                    {"type": "box", "x0": 4, "y0": 5, "z0": 14, "x1": 30, "y1": 23, "z1": 27, "color": body, "material": "metal"},
                    // 頂部掃描蓋板
                    {"type": "box", "x0": 5, "y0": 6, "z0": 27, "x1": 27, "y1": 22, "z1": 30, "color": "silver", "material": "metal"},
                    // 右側已列印白紙輸出托盤
                    {"type": "box", "x0": 26, "y0": 9, "z0": 20, "x1": 33, "y1": 19, "z1": 23, "color": "white", "shaded": false},
                    // 前方發光觸控面板
                    {"type": "box", "x0": 8, "y0": 22, "z0": 21, "x1": 19, "y1": 25, "z1": 25, "color": "neon_cyan", "emissive": true}
                ]
            })
        }
        "sink_vanity" => {
            let cabinet = p_col("wood");
            let basin = s_col("sky");
            json!({
                "title": title,
                "size": [36, 22, 40],
                "primitives": [
                    // 浴室收納木櫃
                    {"type": "box", "x0": 5, "y0": 5, "z0": 0, "x1": 31, "y1": 18, "z1": 14, "color": cabinet, "material": "wood"},
                    // 大理石檯面與淺藍洗手盆
                    {"type": "box", "x0": 4, "y0": 4, "z0": 14, "x1": 32, "y1": 19, "z1": 17, "color": "marble", "material": "stone"},
                    {"type": "box", "x0": 10, "y0": 8, "z0": 17, "x1": 26, "y1": 17, "z1": 19, "color": basin, "material": "glass"},
                    // 銀色水龍頭
                    {"type": "box", "x0": 16, "y0": 7, "z0": 17, "x1": 20, "y1": 11, "z1": 23, "color": "silver", "material": "metal"},
                    // 後方明鏡與頂部暖黃化妝鏡燈
                    {"type": "box", "x0": 7, "y0": 5, "z0": 21, "x1": 29, "y1": 8, "z1": 36, "color": "ice", "material": "glass"},
                    {"type": "box", "x0": 11, "y0": 7, "z0": 35, "x1": 25, "y1": 10, "z1": 38, "color": "neon_yellow", "emissive": true}
                ]
            })
        }
        _ => {
            // 自由詞彙保底奇觀雕像（同樣保留完全可編輯藍圖）
            let main = p_col("jade");
            let accent = s_col("neon_cyan");
            json!({
                "title": title,
                "size": [34, 34, 40],
                "primitives": [
                    {"type": "cylinder", "cx": 16.5, "cy": 16.5, "z0": 0, "z1": 5, "rx": 12.0, "ry": 12.0, "color": "dark_stone", "material": "stone"},
                    {"type": "pyramid", "cx": 16.5, "cy": 16.5, "z0": 5, "z1": 22, "w0": 14.0, "l0": 14.0, "w1": 8.0, "l1": 8.0, "stepped": true, "steps": 5, "color": main, "material": "stone"},
                    {"type": "sphere", "cx": 16.5, "cy": 16.5, "cz": 29.0, "rx": 5.5, "ry": 5.5, "rz": 5.5, "color": accent, "material": "crystal", "emissive": true},
                    {"type": "box", "x0": 9, "y0": 15, "z0": 5, "x1": 11, "y1": 18, "z1": 24, "color": "gold", "material": "metal", "mirror_x": true}
                ]
            })
        }
    }
}

/// 依任意中文/英文描述生成並烘焙完整 Sprite-Stack 資產
pub fn generate_from_prompt(prompt: &str, slices: usize, seed: u64) -> Result<BakedStack, String> {
    let spec = parse_prompt(prompt);
    let bp = build_blueprint_for_spec(&spec, slices);
    let (title, mut canvas) = render_blueprint_canvas(&bp, Some(slices), true, true)?;
    if spec.moss {
        canvas.apply_moss(seed);
    }
    if spec.snow {
        canvas.apply_snow_cap(seed + 1);
    }
    let bp_str = serde_json::to_string_pretty(&bp).ok();
    canvas.bake(
        title,
        spec.category,
        bp_str,
        spec.default_scale,
        spec.anim_preset,
        false,
        false,
    )
}

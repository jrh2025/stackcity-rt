//! 《StackCity RT（像素疊界）》三語系多語國際化（i18n）系統。
//! 支援 繁體中文（zh-Hant）、English（en）、日本語（ja），可自動偵測系統語系或由使用者即時切換，
//! 並透過 WeakMap 無損快取原始文案，同時相容主程式與獨立單檔純遊戲網頁版。

export const SUPPORTED_LANGS = [
  { code: "zh-Hant", label: "🌐 繁體中文" },
  { code: "en", label: "🌐 English" },
  { code: "ja", label: "🌐 日本語" },
];

const LANG_STORAGE_KEY = "stackcity_lang";

// 精確對照字典（以繁體中文原始文案為鍵）
const DICT = {
  // ===== 頁首標題與頂部導覽列 =====
  "StackCity RT（像素疊界）— Sprite-Stacking 微縮城市導演沙盒": {
    en: "StackCity RT — Sprite-Stacking 3D Sandbox & Game Director",
    ja: "StackCity RT — スプライトスタッキング 3Dサンドボックス",
  },
  "像素疊界": { en: "Voxel Sandbox", ja: "ボクセル箱庭" },
  "像素疊界・遊玩版": { en: "Player Edition", ja: "プレイヤー版" },
  "切換本地世界": { en: "Switch Local World", ja: "ワールド切替" },
  "重新命名目前世界": { en: "Rename Current World", ja: "現在のワールド名を変更" },
  "＋ 新建世界": { en: "+ New World", ja: "＋ 新規ワールド" },
  "建立新戶外或室內場景": { en: "Create a new outdoor or indoor scene", ja: "屋外・屋内の新しいシーンを作成" },
  "🗺️ 擴展場景": { en: "🗺️ Resize Map", ja: "🗺️ マップ拡張" },
  "擴大或調整目前場景地圖尺寸": { en: "Expand or resize the current world grid", ja: "現在のマップサイズを拡張・調整" },
  "✏️ 編輯模式": { en: "✏️ Edit Mode", ja: "✏️ 編集モード" },
  "👁️ 訪客參觀中": { en: "👁️ Play / Visitor", ja: "👁️ プレイモード" },
  "切換編輯模式與好友參觀模式": { en: "Toggle between Edit Mode and Visitor Play Mode", ja: "編集モードとプレイモードを切替" },
  "暫停／播放模擬": { en: "Pause / Play Simulation", ja: "一時停止／再生" },
  "模擬時間流速": { en: "Simulation Time Speed", ja: "時間経過スピード" },
  "時鐘暫停": { en: "Clock Paused", ja: "時間停止" },
  "1x 時光": { en: "1x Speed", ja: "1倍速" },
  "4x 加速": { en: "4x Speed", ja: "4倍速" },
  "12x 快轉": { en: "12x Fast", ja: "12倍速" },
  "拖曳調整 24 小時日夜光影": { en: "Drag to adjust 24-hour day/night lighting", ja: "ドラッグで24時間の昼夜ライティングを調整" },
  "切換天氣特效": { en: "Switch Weather Effects", ja: "天候エフェクトを切替" },
  "☀️ 晴": { en: "☀️ Clear", ja: "☀️ 晴れ" },
  "🌧️ 雨": { en: "🌧️ Rain", ja: "🌧️ 雨" },
  "❄️ 雪": { en: "❄️ Snow", ja: "❄️ 雪" },
  "🧑 玩家角色": { en: "🧑 Avatar", ja: "🧑 アバター" },
  "🧑 換造型": { en: "🧑 Avatar", ja: "🧑 外見変更" },
  "選定並自訂你的 3D 玩家角色外觀與名字": { en: "Customize your 3D player avatar and name", ja: "3Dプレイヤーのアバターと名前を設定" },
  "更換玩家化身造型": { en: "Change Player Avatar", ja: "プレイヤーの外見を変更" },
  "🚶 街道漫步": { en: "🚶 Street Walk", ja: "🚶 街歩き" },
  "以你的玩家角色進入第三人稱街道漫步": { en: "Enter third-person Street Walk mode", ja: "三人称視点の街歩きモードへ" },
  "進入第三人稱街道漫步": { en: "Enter third-person Street Walk", ja: "街歩きモードを開始" },
  "🎵 音樂": { en: "🎵 Music", ja: "🎵 音楽" },
  "選擇多風格背景音樂或上傳自訂音樂": { en: "Select BGM style or upload custom audio", ja: "BGMスタイルの選択や独自音源のアップロード" },
  "開啟／靜音全部聲音": { en: "Mute / Unmute All Audio", ja: "音声ミュート切替" },
  "🔄 重置": { en: "🔄 Reset", ja: "🔄 リセット" },
  "清除遊玩進度重玩，或回復世界初始狀態": { en: "Reset play progress, restore initial world, or delete", ja: "プレイ進行のリセットやワールド初期化・削除" },
  "🔄 重玩本關": { en: "🔄 Replay", ja: "🔄 やり直す" },
  "清除本機進度重玩本關": { en: "Reset local progress and replay level", ja: "進行状況をリセットして最初から遊ぶ" },
  "📸 快照": { en: "📸 Snapshots", ja: "📸 スナップ" },
  "建立或還原 SQLite 世界快照": { en: "Create or restore SQLite snapshots", ja: "スナップショットの作成・復元" },
  "🤝 分享": { en: "🤝 Share", ja: "🤝 共有" },
  "匯出或匯入好友的 .stackworld.db 世界檔": { en: "Share full .stackworld.db or standalone .html game", ja: "ワールド(.stackworld.db)またはゲーム単体HTMLを共有" },
  "◧ 左欄": { en: "◧ Left", ja: "◧ 左パネル" },
  "◨ 右欄": { en: "◨ Right", ja: "◨ 右パネル" },
  "切換全螢幕沉浸體驗": { en: "Toggle Fullscreen", ja: "全画面表示の切替" },
  "切換介面語言 / Language": { en: "Switch Language", ja: "表示言語を切替" },
  "🎮 純玩家模式（已鎖定編輯）": { en: "🎮 Pure Player Mode (Read-Only)", ja: "🎮 プレイヤー専用モード（編集ロック中）" },

  // ===== 左側操作工具與資產庫 =====
  "操作工具": { en: "Tools", ja: "操作ツール" },
  "◀ 收合": { en: "◀ Hide", ja: "◀ 折りたたむ" },
  "▶ 造物工具": { en: "▶ Tools", ja: "▶ ツール" },
  "屬性／事件 ◀": { en: "Inspector ◀", ja: "属性／イベント ◀" },
  "🖱️ 選取": { en: "🖱️ Select", ja: "🖱️ 選択" },
  "✋ 拖曳": { en: "✋ Pan", ja: "✋ 移動" },
  "📦 投放": { en: "📦 Place", ja: "📦 配置" },
  "🌿 生態筆刷": { en: "🌿 Eco Brush", ja: "🌿 エコブラシ" },
  "⛰️ 地形": { en: "⛰️ Terrain", ja: "⛰️ 地形" },
  "🛣️ 鋪路": { en: "🛣️ Road", ja: "🛣️ 道路" },
  "〰️ 畫路徑": { en: "〰️ Path", ja: "〰️ ルート" },
  "⭕ 漫遊區": { en: "⭕ Wander", ja: "⭕ 散策エリア" },
  "🗑️ 刪除": { en: "🗑️ Erase", ja: "🗑️ 削除" },

  // 工具子選項
  "左鍵拖曳畫布即可平移場景；中鍵或 Space + 左鍵也可隨時拖曳。": {
    en: "Drag canvas with left click to pan; middle button or Space+Drag also works.",
    ja: "左ドラッグで画面を移動できます（中ボタンやSpace+左ドラッグも有効）。",
  },
  "⬆️ 抬高": { en: "⬆️ Raise", ja: "⬆️ 隆起" },
  "⬇️ 降低": { en: "⬇️ Lower", ja: "⬇️ 沈降" },
  "🌊 水體": { en: "🌊 Water", ja: "🌊 水面" },
  "🟩 草地": { en: "🟩 Grass", ja: "🟩 草原" },
  "🟨 沙地": { en: "🟨 Sand", ja: "🟨 砂地" },
  "⬜ 石地": { en: "⬜ Stone", ja: "⬜ 石畳" },
  "🟥 紅磚": { en: "🟥 Brick", ja: "🟥 赤レンガ" },
  "❄️ 雪地": { en: "❄️ Snow", ja: "❄️ 雪原" },
  "🪵 木地板": { en: "🪵 Wood Floor", ja: "🪵 木床" },
  "◽ 地磚": { en: "◽ Tile Floor", ja: "◽ タイル床" },
  "🟥 紅地毯": { en: "🟥 Red Carpet", ja: "🟥 赤カーペット" },
  "🟦 藍地毯": { en: "🟦 Blue Carpet", ja: "🟦 青カーペット" },
  "⬛ 水泥地": { en: "⬛ Concrete", ja: "⬛ コンクリート" },
  "🧱 石牆": { en: "🧱 Stone Wall", ja: "🧱 石壁" },
  "🏛️ 白牆": { en: "🏛️ Cream Wall", ja: "🏛️ 白壁" },
  "🪟 玻璃牆": { en: "🪟 Glass Wall", ja: "🪟 ガラス壁" },
  "🛣️ 柏油大道": { en: "🛣️ Asphalt Road", ja: "🛣️ アスファルト" },
  "🪨 石板道": { en: "🪨 Stone Path", ja: "🪨 石畳の道" },
  "🥾 泥土小徑": { en: "🥾 Dirt Trail", ja: "🥾 土の小道" },
  "🌉 木橋": { en: "🌉 Wood Bridge", ja: "🌉 木の橋" },
  "✖️ 清除道路": { en: "✖️ Clear Road", ja: "✖️ 道路消去" },
  "投放角度": { en: "Place Yaw", ja: "配置向き" },
  "投放縮放": { en: "Place Scale", ja: "配置倍率" },
  "🌲 森林混林": { en: "🌲 Mixed Forest", ja: "🌲 森林ミックス" },
  "🌻 花田農野": { en: "🌻 Flower Meadow", ja: "🌻 花畑・草原" },
  "🐑 牧場動物": { en: "🐑 Pasture Herd", ja: "🐑 牧場動物" },
  "🏘️ 城鎮聚落": { en: "🏘️ Town Cluster", ja: "🏘️ 街並み・家屋" },
  "🪑 室內家具": { en: "🪑 Indoor Decor", ja: "🪑 室内家具" },
  "✨ 自訂多選池": { en: "✨ Custom Pool", ja: "✨ カスタム選択" },
  "半徑": { en: "Radius", ja: "半径" },
  "密度": { en: "Density", ja: "密度" },
  "最小間距": { en: "Min Spacing", ja: "最小間隔" },
  "隨機縮放": { en: "Random Scale", ja: "ランダム倍率" },
  "隨機旋轉": { en: "Random Yaw", ja: "ランダム回転" },
  "避開水域與道路": { en: "Avoid Water & Roads", ja: "水域・道路を回避" },
  "點選要隨機撒出的物件（預設選中多個植物）：": {
    en: "Select objects for random placement:",
    ja: "ランダム配置するオブジェクトを選択：",
  },
  "正在畫移動路徑，點擊地圖新增節點。": {
    en: "Drawing path — click map to add waypoints.",
    ja: "ルート描画中：マップをクリックして経由点を追加。",
  },
  "✅ 完成路徑": { en: "✅ Finish Path", ja: "✅ ルート確定" },
  "✖ 取消": { en: "✖ Cancel", ja: "✖ キャンセル" },
  "點擊地圖位置將置入半徑 4 格的自由漫遊圈，隨後可將動物或人物加入其中。": {
    en: "Click map to place a 4-tile wander zone, then assign characters/animals.",
    ja: "クリック地点に半径4マスの散策エリアを作成します。",
  },
  "點擊物件、路徑節點或道路即可移除。": {
    en: "Click any object, path node, or road to remove it.",
    ja: "オブジェクト・経路・道路をクリックして削除します。",
  },

  // 資產庫分類
  "Sprite-Stack 資產庫": { en: "Sprite-Stack Library", ja: "アセットライブラリ" },
  "✨ 造物工坊": { en: "✨ Studio", ja: "✨ 造形スタジオ" },
  "全部": { en: "All", ja: "すべて" },
  "建築": { en: "Building", ja: "建物" },
  "人物": { en: "Character", ja: "人物" },
  "動物": { en: "Animal", ja: "動物" },
  "植物": { en: "Plant", ja: "植物" },
  "載具": { en: "Vehicle", ja: "乗り物" },
  "道具/家具": { en: "Prop/Decor", ja: "道具・家具" },
  "取消": { en: "Cancel", ja: "キャンセル" },
  "確定刪除": { en: "Confirm Delete", ja: "削除確定" },
  "🔥 確定移除": { en: "🔥 Confirm Remove", ja: "🔥 削除を実行" },
  "🔥 確定永久刪除": { en: "🔥 Delete Forever", ja: "🔥 完全に削除" },

  // ===== 中央畫布 HUD =====
  "載入中...": { en: "Loading...", ja: "読み込み中..." },
  "🧭 重設視角": { en: "🧭 Reset View", ja: "🧭 視点リセット" },
  "重設視角 (R)": { en: "Reset Camera (R)", ja: "視点リセット (R)" },
  "返回上帝視角": { en: "Exit Walk View", ja: "俯瞰視点へ戻る" },
  "WASD／方向鍵：操控玩家移動・空白鍵：跳躍・F 或雙擊物件：任務互動・右鍵旋轉・中鍵／✋拖曳：移動畫面": {
    en: "WASD/Arrows: Move Player · Space: Jump · F / Double-click: Interact · Right-drag: Orbit · Middle/Pan: Move Camera",
    ja: "WASD/矢印: 移動 · Space: ジャンプ · F/ダブルクリック: 調べる · 右ドラッグ: 回転 · 中ボタン: 平行移動",
  },
  "WASD／方向鍵：操控玩家移動・空白鍵：跳躍・F 或點擊物件：任務互動・右鍵旋轉・中鍵／Space+左鍵：平移畫面": {
    en: "WASD/Arrows: Move Player · Space: Jump · F or Click Object: Interact · Right-drag: Orbit · Middle/Space+Drag: Pan",
    ja: "WASD/矢印: 移動 · Space: ジャンプ · Fまたはクリック: 調べる · 右ドラッグ: 回転 · 中ボタン/Space+左ドラッグ: 移動",
  },
  "CityRT 街道漫步": { en: "CityRT Street Walk", ja: "CityRT 街歩き" },
  "｜WASD 移動（A左/D右）、按住拖曳轉向、點擊或按 F 與身旁物件互動、空白鍵跳躍": {
    en: " | WASD Move · Drag to Look · Click or [F] to Interact · Space: Jump",
    ja: "｜WASD移動・ドラッグで視点回転・クリックまたは[F]で調べる・Spaceでジャンプ",
  },

  // ===== 右側面板（屬性 / 路徑 / 事件 / 任務） =====
  "🎛️ 屬性": { en: "🎛️ Props", ja: "🎛️ 属性" },
  "〰️ 路徑": { en: "〰️ Paths", ja: "〰️ 経路" },
  "🎬 事件": { en: "🎬 Events", ja: "🎬 イベント" },
  "🏆 任務": { en: "🏆 Quests", ja: "🏆 クエスト" },
  "未選取任何場景物件": { en: "No Object Selected", ja: "オブジェクト未選択" },
  "在左側選擇「🖱️ 選取」工具並點擊 3D 世界中的任何建築、人物、動物、植物、家具或載具，即可獨立調整長寬高 (X/Y/Z)、旋轉、浮空高度、解除附掛、直接指派路徑與事件或設定任務考驗！": {
    en: "Use the Select tool and click any building, NPC, animal, plant, or vehicle in the 3D scene to edit XYZ scale, rotation, elevation, mount detach, paths, events, or quest encounters.",
    ja: "左の「選択」ツールで3Dシーン内の建物・人物・動物・乗り物等をクリックすると、XYZサイズ・回転・高度・乗降解除・移動経路・イベント・クエスト設定を編集できます。",
  },
  "基礎屬性與三軸尺寸": { en: "Transform & XYZ Dimensions", ja: "基本属性・XYZサイズ" },
  "名稱": { en: "Name", ja: "名前" },
  "座標 X": { en: "Pos X", ja: "座標 X" },
  "座標 Y": { en: "Pos Y", ja: "座標 Y" },
  "高度 Z": { en: "Elev Z", ja: "高度 Z" },
  "朝向角度": { en: "Yaw Angle", ja: "向き角度" },
  "三軸尺寸（長寬高）": { en: "3D Scale (W / D / H)", ja: "3軸スケール (幅・奥行・高)" },
  "🔒 固定等比例": { en: "🔒 Lock Ratio", ja: "🔒 比率固定" },
  "重設 1:1": { en: "Reset 1:1", ja: "1:1に戻す" },
  "動畫模式": { en: "Animation", ja: "アニメーション" },
  "自動預設": { en: "Auto Preset", ja: "自動プリセット" },
  "待機微動": { en: "Idle", ja: "待機" },
  "步行擺動": { en: "Walk", ja: "歩行" },
  "行駛震動": { en: "Drive", ja: "走行" },
  "上下漂浮": { en: "Float", ja: "浮遊" },
  "自體旋轉": { en: "Spin", ja: "回転" },
  "靜止不動": { en: "None", ja: "静止" },
  "夜間發光": { en: "Night Glow", ja: "夜間発光" },
  "啟用": { en: "Enabled", ja: "有効" },
  "乘載／附掛": { en: "Mount / Parent", ja: "搭乗／連結" },
  "（無附掛・獨立地面物件）": { en: "(None — Independent)", ja: "（なし・独立オブジェクト）" },
  "🔓 立即解除附掛": { en: "🔓 Detach Now", ja: "🔓 連結を解除" },
  "💾 套用並儲存屬性": { en: "💾 Save Properties", ja: "💾 属性を保存" },
  "👁️ 跟隨視角": { en: "👁️ Follow Cam", ja: "👁️ 追従カメラ" },
  "複製": { en: "Duplicate", ja: "複製" },
  "刪除物件": { en: "Delete Object", ja: "削除" },
  "🎯 任務互動與考驗設定": { en: "🎯 Quest Interaction & Encounter", ja: "🎯 クエスト会話・試練設定" },
  "互動類型": { en: "Interaction Type", ja: "タイプ" },
  "無任務互動": { en: "None", ja: "なし" },
  "💬 線索／情報 NPC": { en: "💬 Clue / Info NPC", ja: "💬 ヒント・情報NPC" },
  "❓ 選擇題考官": { en: "❓ Quiz Challenger", ja: "❓ クイズ出題者" },
  "⚠️ 搶劫／攔路反派": { en: "⚠️ Robber / Ambush", ja: "⚠️ 強盗・待ち伏せ" },
  "🛡️ 夥伴支援補給": { en: "🛡️ Ally Support", ja: "🛡️ 仲間からの支援" },
  "💎 寶藏／關鍵物件": { en: "💎 Treasure / Relic", ja: "💎 宝箱・重要アイテム" },
  "🕵️ 案件嫌疑犯": { en: "🕵️ Case Suspect", ja: "🕵️ 事件の容疑者" },
  "線索": { en: "Clue", ja: "ヒント" },
  "考驗": { en: "Quiz", ja: "クイズ" },
  "劫匪": { en: "Ambush", ja: "強盗" },
  "支援": { en: "Support", ja: "支援" },
  "寶藏": { en: "Treasure", ja: "宝箱" },
  "嫌疑人": { en: "Suspect", ja: "容疑者" },
  "打聽線索": { en: "Ask for Clue", ja: "話を聞く" },
  "接受考驗": { en: "Take Quiz", ja: "クイズに挑戦" },
  "應對攔截": { en: "Handle Ambush", ja: "対処する" },
  "領取支援": { en: "Get Support", ja: "支援を受ける" },
  "開啟寶藏": { en: "Open Treasure", ja: "宝箱を開ける" },
  "盤查指認": { en: "Question / Accuse", ja: "尋問・告発" },
  "互動": { en: "Interact", ja: "調べる" },

  // 路徑與漫遊區
  "＋ 在地圖畫路徑": { en: "+ Draw Path", ja: "＋ マップに描く" },
  "尚未設定移動路徑。選取物件後點上方按鈕即可繪製！": {
    en: "No movement paths yet. Select an object and click above to draw!",
    ja: "移動ルートがありません。オブジェクトを選んで上ボタンで作成できます。",
  },
  "（未指派物件）": { en: "(Unassigned)", ja: "（未割り当て）" },
  "移動物件": { en: "Target Object", ja: "移動対象" },
  "模式／速": { en: "Loop / Speed", ja: "モード／速度" },
  "循環 Loop": { en: "Loop", ja: "ループ" },
  "往返 PingPong": { en: "Ping-Pong", ja: "往復" },
  "單程 Once": { en: "Once", ja: "片道" },
  "＋ 放置漫遊圈": { en: "+ Place Zone", ja: "＋ エリア設置" },
  "放一個圓形區域即可讓裡面的動物或市民自動自由散步。": {
    en: "Place a circular zone to let animals or citizens wander freely inside.",
    ja: "円形エリアを置くと、中の動物や住民が自由に歩き回ります。",
  },
  "加入選定": { en: "Add Selected", ja: "選択を追加" },
  "＋ 將目前選取物件加入漫遊": { en: "+ Add Selected Object", ja: "＋ 選択中を追加" },

  // 事件導演與任務 HUD
  "收合": { en: "Hide", ja: "収納" },
  "展開": { en: "Show", ja: "展開" },
  "筆記": { en: "Notes", ja: "メモ" },
  "重置": { en: "Reset", ja: "リセット" },
  "設定": { en: "Setup", ja: "設定" },
  "事件互動": { en: "Encounter", ja: "イベント会話" },
  "來自場景物件": { en: "From World Object", ja: "ワールド内のオブジェクト" },
  "🎉 恭喜完成關卡目標！": { en: "🎉 Level Objective Complete!", ja: "🎉 クリアおめでとうございます！" },
  "太厲害了！你已順利達成這個世界的所有挑戰目標！": {
    en: "Awesome! You have completed all challenges in this world!",
    ja: "お見事！このワールドのすべての目標を達成しました！",
  },
  "🔄 清除進度再玩一次": { en: "🔄 Replay Level", ja: "🔄 最初からやり直す" },
  "🎉 繼續探索世界": { en: "🎉 Keep Exploring", ja: "🎉 探索を続ける" },
  "收起線索筆記": { en: "Hide Clue Notes", ja: "ヒントメモを閉じる" },
  "尚未蒐集到線索筆記，多與場上 NPC 或物件互動吧！": {
    en: "No clues collected yet. Interact with NPCs and objects in the world!",
    ja: "まだヒントがありません。NPCやオブジェクトを調べてみましょう！",
  },

  // ===== 造物工坊 Modal =====
  "✨ Sprite-Stacking 造物工坊": { en: "✨ Sprite-Stacking Creation Studio", ja: "✨ スプライト造形スタジオ" },
  "純 Rust 即時體素烘焙：支援 68 種建築/角色/家具/機械原型、3D 積木藍圖、四視圖雕刻與套件匯入": {
    en: "Pure Rust real-time voxel baking: 68 presets, 3D brick blueprint editor, 4-view carver & AI generator.",
    ja: "Rust製リアルタイムボクセル生成：68種の内蔵モデル・3Dブロック設計・4面図彫刻・AI生成に対応。",
  },
  "🪄 快速造物": { en: "🪄 Quick Create", ja: "🪄 クイック生成" },
  "🧱 3D 積木藍圖": { en: "🧱 3D Blueprint", ja: "🧱 3D設計図" },
  "📚 資產管理／雕刻／匯入": { en: "📚 Assets / Carve / Import", ja: "📚 資産管理／彫刻／取込" },
  "🤖 AI 藍圖": { en: "🤖 AI Blueprint", ja: "🤖 AI設計図" },
  "輸入繁體中文／英文物件描述（支援建築、人物、家具、機械與雪景/夜光修飾）": {
    en: "Enter description (supports buildings, characters, furniture, snow/glow modifiers, e.g. Golden Throne)",
    ja: "生成したいオブジェクトを入力（英語・日本語・中国語キーワード対応）",
  },
  "例如：黃金王座、自動機械手臂、粉色沙發、雪中的城堡...": {
    en: "e.g., Golden Throne, Robot Arm, Pink Sofa, Snowy Castle...",
    ja: "例：Golden Throne, Snowy Castle, Sakura Tree...",
  },
  "⚡ 生成": { en: "⚡ Generate", ja: "⚡ 生成" },
  "切片層數": { en: "Slices", ja: "スライス層数" },
  "6 大類 68 種內建精選原型（點擊立即生成）": {
    en: "68 Built-in Voxel Presets across 6 Categories (Click to Generate)",
    ja: "6カテゴリ・全68種類の内蔵プリセット（クリックで即生成）",
  },
  "✏️ 正在編輯既有物件": { en: "✏️ Editing Existing Asset", ja: "✏️ 既存アセットを編集中" },
  "💾 覆寫儲存原物件": { en: "💾 Overwrite Original", ja: "💾 上書き保存" },
  "✨ 另存新物件": { en: "✨ Save as New", ja: "✨ 別名で保存" },
  "✕ 結束編輯": { en: "✕ Cancel Edit", ja: "✕ 編集終了" },
  "物件名稱": { en: "Asset Name", ja: "オブジェクト名" },
  "輸入物件名稱": { en: "Enter asset name", ja: "オブジェクト名を入力" },
  "畫布長 W": { en: "Width W", ja: "幅 W" },
  "畫布寬 D": { en: "Depth D", ja: "奥行 D" },
  "高度層 H": { en: "Height H", ja: "高さ H" },
  "＋ 方塊": { en: "+ Box", ja: "＋ 直方体" },
  "＋ 圓柱": { en: "+ Cylinder", ja: "＋ 円柱" },
  "＋ 球體": { en: "+ Sphere", ja: "＋ 球体" },
  "＋ 圓錐": { en: "+ Cone", ja: "＋ 円錐" },
  "＋ 角錐": { en: "+ Pyramid", ja: "＋ 四角錐" },
  "－ 挖空": { en: "- Cutout", ja: "－ くり抜き" },
  "⚡ 自動即時預覽": { en: "⚡ Live Preview", ja: "⚡ 自動プレビュー" },
  "👁️ 預覽": { en: "👁️ Preview", ja: "👁️ プレビュー" },
  "💾 確認儲存至資產庫": { en: "💾 Save to Library", ja: "💾 ライブラリに保存" },
  "● 草稿尚未確認儲存（尚未寫入個人資產庫）": {
    en: "● Unsaved draft (not yet written to personal asset library)",
    ja: "● 未保存の下書き（ライブラリにはまだ保存されていません）",
  },
  "檢視／直接編輯原始 Blueprint JSON": {
    en: "View / Edit Raw Blueprint JSON",
    ja: "生の設計図JSONを表示／編集",
  },
  "1. 🗂️ 個人跨世界資產庫（所有世界共用・可編輯／載入／刪除）": {
    en: "1. 🗂️ Personal Cross-World Library (Shared across worlds · Edit / Load / Delete)",
    ja: "1. 🗂️ 個人クロスワールドライブラリ（全ワールド共有・編集／読込／削除）",
  },
  "個人生成、AI 建模或匯入的物件皆收錄於此，可跨世界載入、改名、進入積木藍圖細調或永久刪除。": {
    en: "Custom, AI, and imported assets are stored here. Load across worlds, rename, fine-tune bricks, or delete.",
    ja: "自作・AI・取込アセットはここに保存され、どのワールドにも読み込み・編集・削除できます。",
  },
  "2. 🌍 本世界資產清單（可編輯改名／細調積木／從本世界移除）": {
    en: "2. 🌍 Current World Assets (Rename / Tune Blueprint / Remove)",
    ja: "2. 🌍 このワールドのアセット一覧（名前変更／設計図編集／削除）",
  },
  "3. 四視圖像素雕刻（TOP, SIDE, FRONT, BACK）": {
    en: "3. 4-View Pixel Carver (TOP, SIDE, FRONT, BACK)",
    ja: "3. 4面図ピクセル彫刻（上面・側面・正面・背面）",
  },
  "圓柱修圓": { en: "Round Cylindrical", ja: "円柱丸め処理" },
  "🗿 開始雕刻": { en: "🗿 Carve", ja: "🗿 彫刻開始" },
  "4. 匯入既有 Sprite-Stacking 套件（.zip / blueprint.json / slices.png）": {
    en: "4. Import Existing Sprite-Stacking Package (.zip / blueprint.json / slices.png)",
    ja: "4. 既存スプライトスタッキング素材を取り込み (.zip / json / png)",
  },
  "📦 匯入套件": { en: "📦 Import Asset", ja: "📦 素材を取込" },
  "支援 Google Gemma 4、Google Gemini（2.5 / 3 主流模型）及 OpenAI 相容端點；支援一鍵查詢帳號可用模型清單，亦可自由輸入最新模型名稱並於本機記憶／移除 API Key。": {
    en: "Supports Google Gemma 4, Gemini (2.5 / 3), and OpenAI-compatible endpoints. Query available models for your key or type any custom model ID, with local key save/remove.",
    ja: "Google Gemma 4・Gemini（2.5 / 3系）・OpenAI互換エンドポイントに対応。利用可能モデルの一覧取得や任意モデル名の指定、APIキーの保存／削除ができます。",
  },
  "模型供應商": { en: "AI Provider", ja: "AIプロバイダー" },
  "Google Gemini（2.5 / 3 主流模型）": { en: "Google Gemini (2.5 / 3 Models)", ja: "Google Gemini (2.5 / 3 主流モデル)" },
  "Google Gemma 4（開放模型）": { en: "Google Gemma 4 (Open Weights)", ja: "Google Gemma 4（オープンモデル）" },
  "OpenAI 相容端點（GPT / Ollama）": { en: "OpenAI-Compatible (GPT / Ollama)", ja: "OpenAI互換 (GPT / Ollama)" },
  "模型名稱（可自由輸入或點選下方建議）": { en: "Model Name (Editable or Quick-Pick)", ja: "モデル名（自由入力または下の候補を選択）" },
  "🔍 查可用模型": { en: "🔍 List Models", ja: "🔍 モデル一覧取得" },
  "常用模型快捷選填：": { en: "Quick Suggestions:", ja: "おすすめモデル：" },
  "🔑 API Key 管理（依供應商個別記憶於本機瀏覽器）": {
    en: "🔑 API Key Manager (Saved per provider in localStorage)",
    ja: "🔑 APIキー管理（プロバイダー別にブラウザへ保存）",
  },
  "尚未記憶": { en: "Not Saved", ja: "未保存" },
  "請輸入 API Key（若已設系統環境變數可留空）": {
    en: "Enter API Key (leave blank if env var is set)",
    ja: "APIキーを入力（環境変数設定済みの場合は空欄可）",
  },
  "💾 記憶金鑰": { en: "💾 Save Key", ja: "💾 キーを保存" },
  "🗑️ 移除金鑰": { en: "🗑️ Remove Key", ja: "🗑️ キーを削除" },
  "送出 AI 建模時自動記憶目前輸入的 API Key 與模型名稱": {
    en: "Auto-save API Key and model name when generating",
    ja: "生成時にAPIキーとモデル名を自動記憶する",
  },
  "描述你想像的任何獨特 3D 物件": { en: "Describe any unique 3D object you imagine", ja: "作りたい3Dオブジェクトを自由に説明してください" },
  "例如：浮空蒸氣龐克鐘錶飛船、發光水晶噴泉...": {
    en: "e.g., Floating steampunk airship, glowing crystal fountain...",
    ja: "例：空飛ぶスチームパンク飛行船、光るクリスタルの噴水...",
  },
  "🚀 AI 建模": { en: "🚀 AI Generate", ja: "🚀 AIで生成" },
  "360° 旋轉轉盤預覽": { en: "360° Turntable Preview", ja: "360° 回転プレビュー" },
  "白晝": { en: "Day", ja: "昼" },
  "黃昏": { en: "Sunset", ja: "夕方" },
  "夜景發光": { en: "Night Glow", ja: "夜景発光" },
  "尚未選擇或生成物件，請從左側生成或挑選原型。": {
    en: "No object generated yet. Generate or pick a preset from the left.",
    ja: "まだオブジェクトがありません。左側から生成または選択してください。",
  },
  "✅ 加入資產庫並開始投放": { en: "✅ Save & Start Placing", ja: "✅ ライブラリに追加して配置" },
  "➕ 載入世界": { en: "+ Load to World", ja: "＋ ワールドに読込" },
  "✏️ 編輯": { en: "✏️ Edit", ja: "✏️ 編集" },
  "🗑️ 刪除": { en: "🗑️ Delete", ja: "🗑️ 削除" },
  "💾 儲存改名": { en: "💾 Save Rename", ja: "💾 名前を保存" },
  "🧱 進入 3D 積木細調": { en: "🧱 Edit 3D Bricks", ja: "🧱 3Dブロック編集" },
  "發光": { en: "Glow", ja: "発光" },
  "X鏡像": { en: "Mirror X", ja: "Xミラー" },
  "位置 X": { en: "Pos X", ja: "位置 X" },
  "位置 Y": { en: "Pos Y", ja: "位置 Y" },
  "底高 Z": { en: "Base Z", ja: "底面 Z" },
  "中心 X": { en: "Center X", ja: "中心 X" },
  "中心 Y": { en: "Center Y", ja: "中心 Y" },
  "中心 Z": { en: "Center Z", ja: "中心 Z" },
  "長 W": { en: "Width W", ja: "幅 W" },
  "寬 D": { en: "Depth D", ja: "奥行 D" },
  "高 H": { en: "Height H", ja: "高さ H" },
  "半徑 X": { en: "Radius X", ja: "半径 X" },
  "半徑 Y": { en: "Radius Y", ja: "半径 Y" },
  "半徑 Z": { en: "Radius Z", ja: "半径 Z" },
  "壁厚": { en: "Wall", ja: "壁厚" },
  "底半徑": { en: "Base R", ja: "底半径" },
  "頂半徑": { en: "Top R", ja: "頂半径" },
  "底長 W": { en: "Base W", ja: "底幅 W" },
  "底寬 D": { en: "Base D", ja: "底奥行 D" },
  "半跨距": { en: "Half Span", ja: "半スパン" },
  "深度": { en: "Depth", ja: "奥行" },
  "半徑": { en: "Radius", ja: "半径" },

  // ===== 彈窗（新建 / 重新命名 / 分享 / 重置 / 音樂 / 玩家 / 地圖擴展） =====
  "🌍 建立新場景世界（戶外探索 ＋ 建築物內部空間）": {
    en: "🌍 Create New World (Outdoor & Indoor Scenes)",
    ja: "🌍 新規ワールド作成（屋外・屋内シーン）",
  },
  "世界名稱": { en: "World Name", ja: "ワールド名" },
  "創作者署名": { en: "Author Name", ja: "作成者名" },
  "世界簡介（選填）": { en: "Description (Optional)", ja: "紹介文（任意）" },
  "💾 儲存新名稱": { en: "💾 Save Name", ja: "💾 名前を保存" },
  "🌍 戶外宏觀探索場景（40×40）": { en: "🌍 Outdoor Exploration Worlds (40×40)", ja: "🌍 屋外探索シーン (40×40)" },
  "🏠 建築物內部立體剖面場景（24×24・含專屬家具與相遇事件）": {
    en: "🏠 Indoor Cutaway Interiors (24×24 · Furniture & Encounters)",
    ja: "🏠 建物内部の断面シーン (24×24・家具とイベント付き)",
  },
  "✨ 立即建立場景": { en: "✨ Create World Now", ja: "✨ ワールドを作成" },
  "🤝 分享我的遊戲與世界給好友": { en: "🤝 Share Your Game & World", ja: "🤝 ゲームとワールドを共有" },
  "⬇️ 下載完整分享檔（.stackworld.db）": { en: "⬇️ Download Full World (.stackworld.db)", ja: "⬇️ フル共有ファイル (.stackworld.db)" },
  "🎮 下載單純遊戲網頁檔（.html）": { en: "🎮 Download Play-Only HTML Game (.html)", ja: "🎮 ゲーム単体HTMLをダウンロード (.html)" },
  "👁️ 新分頁試玩": { en: "👁️ Preview Play", ja: "👁️ 別タブで試遊" },
  "🌍 匯入世界": { en: "🌍 Import World", ja: "🌍 ワールド取込" },
  "🔄 重置遊戲進度、初始狀態或刪除本遊戲": {
    en: "🔄 Reset Progress, Restore Initial State, or Delete World",
    ja: "🔄 進行状況リセット・初期化・ワールド削除",
  },
  "🎮 選項 A：清除遊玩進度（保留目前創作場景）": {
    en: "🎮 Option A: Reset Play Progress Only (Keep Your Edits)",
    ja: "🎮 オプションA：プレイ進行状況のみリセット（建築等は保持）",
  },
  "✨ 確認清除遊玩進度": { en: "✨ Confirm Reset Progress", ja: "✨ 進行状況をリセット" },
  "♻️ 選項 B：回復世界初始出廠狀態": {
    en: "♻️ Option B: Restore Factory Initial State",
    ja: "♻️ オプションB：初期状態へ完全リセット",
  },
  "♻️ 確認回復世界初始狀態": { en: "♻️ Confirm Restore Initial", ja: "♻️ 初期状態に戻す" },
  "🗑️ 選項 C：刪除此世界（永久刪除本遊戲）": {
    en: "🗑️ Option C: Delete This World Permanently",
    ja: "🗑️ オプションC：このワールドを完全に削除",
  },
  "🗑️ 刪除這個世界...": { en: "🗑️ Delete This World...", ja: "🗑️ このワールドを削除..." },
  "⚠️ 此操作無法復原！確定永久刪除此世界嗎？": {
    en: "⚠️ This cannot be undone! Permanently delete this world?",
    ja: "⚠️ 元に戻せません！本当にこのワールドを完全削除しますか？",
  },
  "🎵 背景音樂與自訂音樂設定": { en: "🎵 Background Music & Custom Audio", ja: "🎵 BGM・独自音楽設定" },
  "🔊 立即試聽目前風格": { en: "🔊 Preview Current BGM", ja: "🔊 現在のBGMを試聴" },
  "音樂音量": { en: "BGM Volume", ja: "BGM音量" },
  "⬆️ 上傳並播放": { en: "⬆️ Upload & Play", ja: "⬆️ アップロードして再生" },
  "🧑 我的玩家角色設定（化身探索這個世界）": { en: "🧑 Player Avatar Settings", ja: "🧑 プレイヤーアバター設定" },
  "🧑 選擇我的玩家角色造型": { en: "🧑 Choose Your Player Avatar", ja: "🧑 プレイヤーの外見を選択" },
  "玩家角色暱稱": { en: "Player Nickname", ja: "プレイヤー名" },
  "角色體型倍率": { en: "Avatar Scale", ja: "アバター倍率" },
  "📍 傳送到畫面中心": { en: "📍 Move to Center", ja: "📍 中央へ移動" },
  "🎥 鏡頭定位玩家": { en: "🎥 Focus Player", ja: "🎥 プレイヤーに注目" },
  "🚶 立刻以這個角色開始漫步！": { en: "🚶 Start Street Walk Now!", ja: "🚶 このキャラで街歩き開始！" },
  "🗺️ 擴展場景地圖尺寸": { en: "🗺️ Resize World Map", ja: "🗺️ マップサイズ拡張" },
  "📐 確認套用擴展": { en: "📐 Apply Map Resize", ja: "📐 サイズ拡張を適用" },
  "📸 SQLite 世界快照與還原": { en: "📸 SQLite Snapshots & Restore", ja: "📸 スナップショット管理" },
  "💾 建立快照": { en: "💾 Save Snapshot", ja: "💾 スナップ保存" },
  "⏪ 還原": { en: "⏪ Restore", ja: "⏪ 復元" },

  // 任務過關目標類型
  "🏆 積分達標制（執行任務／考驗累積積分）": {
    en: "🏆 Target Score (Earn points from encounters & quests)",
    ja: "🏆 スコア達成型（試練やミッションで得点獲得）",
  },
  "🚨 抓出真兇制（蒐集線索後從嫌疑犯中逮捕歹徒）": {
    en: "🚨 Catch the Culprit (Gather clues and arrest the suspect)",
    ja: "🚨 真犯人逮捕型（ヒントを集めて容疑者から犯人を特定）",
  },
  "💎 尋寶探索制（尋找指定數量的寶藏）": {
    en: "💎 Treasure Hunt (Find the target number of treasures)",
    ja: "💎 宝探し型（指定数の宝箱を発見）",
  },
  "❓ 知識解謎制（通過指定數量的難題考驗）": {
    en: "❓ Solve Quizzes (Pass the required number of challenges)",
    ja: "❓ クイズ踏破型（指定数のクイズ・試練をクリア）",
  },
  "📋 綜合清單制（完成規畫者自訂的多項條件）": {
    en: "📋 Custom Checklist (Complete custom objectives)",
    ja: "📋 総合チェックリスト型（複数のカスタム目標を達成）",
  },

  // 交通工具搭乘系統
  "皇家馬車": { en: "Royal Carriage", ja: "王室馬車" },
  "🚗 交通工具搭乘設定": { en: "🚗 Vehicle / Mount Settings", ja: "🚗 乗り物・騎乗設定" },
  "允許玩家按 G 搭乘": { en: "Allow boarding with [G]", ja: "[G]キーで搭乗を許可" },
  "搭乘模式": { en: "Mount Mode", ja: "搭乗モード" },
  "🎮 玩家親自駕駛／騎乘": { en: "🎮 Player Drive / Ride", ja: "🎮 プレイヤー操縦／騎乗" },
  "🚌 自動巡航乘客 (僅搭乘)": { en: "🚌 Auto-Cruise Passenger", ja: "🚌 自動巡回乗客 (搭乗のみ)" },
  "🔄 兩者皆可 (Tab 鍵切換)": { en: "🔄 Both (Tab to Switch)", ja: "🔄 両方対応 (Tabで切替)" },
  "乘載外觀": { en: "Seat Style", ja: "乗車スタイル" },
  "🐎 開放式騎乘 (顯示玩家)": { en: "🐎 Open Seat (Show Player)", ja: "🐎 オープン騎乗 (プレイヤー表示)" },
  "🚗 密閉車廂 (隱藏玩家)": { en: "🚗 Enclosed Cabin (Hide Player)", ja: "🚗 キャビン乗車 (プレイヤー非表示)" },
  "移動倍率": { en: "Speed Multiplier", ja: "移動速度倍率" },
  "座位高度": { en: "Seat Height", ja: "座席高さ" },
  "前後偏移": { en: "Seat Forward Offset", ja: "座席前後位置" },
  "勾選上方開關後，玩家靠近此物件按下 G 鍵即可駕駛或搭乘！": {
    en: "Enable to let the player board or drive this object with [G]!",
    ja: "オンにすると、プレイヤーが近くで G キーを押して搭乗・操縦できます！",
  },
  "🔄 切換駕駛／巡航 (Tab)": { en: "🔄 Drive / Cruise (Tab)", ja: "🔄 操縦／巡回切替 (Tab)" },
  "🚪 下車 (G)": { en: "🚪 Dismount (G)", ja: "🚪 降りる (G)" },
  "駕駛": { en: "Drive", ja: "運転" },
  "騎乘": { en: "Ride", ja: "騎乗" },
  "登船駕駛": { en: "Sail", ja: "乗船" },
  "搭乘巡航": { en: "Board Cruise", ja: "巡回に乗る" },
  "搭乘／駕駛": { en: "Board / Drive", ja: "搭乗／操縦" },
  "搭乘": { en: "Board", ja: "搭乗" },
  "WASD／方向鍵：移動・G：上下車／船／馬・空白鍵：跳躍／鳴笛・F 或雙擊物件：任務互動・右鍵旋轉・中鍵／✋拖曳：移動畫面": {
    en: "WASD/Arrows: Move · G: Board/Dismount · Space: Jump/Horn · F/Dbl-Click: Interact · Right-Drag: Orbit · Middle/Pan: Move Camera",
    ja: "WASD/矢印：移動・G：乗り降り・Space：ジャンプ/クラクション・F/ダブルクリック：会話・右ドラッグ：回転・中ボタン：視点移動",
  },
  "WASD／方向鍵：移動・G：上下車／船／馬・空白鍵：跳躍／鳴笛・F 或點擊物件：任務互動・右鍵旋轉・中鍵／Space+左鍵：平移畫面": {
    en: "WASD/Arrows: Move · G: Board/Dismount · Space: Jump/Horn · F/Click: Interact · Right-Drag: Orbit · Space+Drag: Pan",
    ja: "WASD/矢印：移動・G：乗り降り・Space：ジャンプ/クラクション・F/クリック：会話・右ドラッグ：回転・Space+ドラッグ：移動",
  },

  // 建築物「門」與跨世界探索系統
  "古典木門": { en: "Classic Wooden Door", ja: "クラシック木製ドア" },
  "星光傳送門": { en: "Starlight Portal", ja: "スターライトポータル" },
  "🚪 建築物出入口（門）設定": { en: "🚪 Building Door / Portal Settings", ja: "🚪 建物ドア・ポータル設定" },
  "啟用門傳送": { en: "Enable Door Portal", ja: "ドア移動を有効化" },
  "✨ 一鍵建立並連結全新室內世界（自動放回程門）": {
    en: "✨ 1-Click Create & Link Interior World (Auto Exit Door)",
    ja: "✨ ワンクリックで屋内ワールド作成＆双方向ドア接続",
  },
  "✨ 一鍵建立室內世界並建立雙向門": {
    en: "✨ Create Interior World & Link 2-Way Doors",
    ja: "✨ 屋内ワールドを作成し双方向ドアを接続",
  },
  "門的類型": { en: "Door Type", ja: "ドア種類" },
  "🚪 進入目標世界 (Enter)": { en: "🚪 Enter Target World", ja: "🚪 対象ワールドへ入る" },
  "🔙 離開返回原世界 (Exit)": { en: "🔙 Exit to Previous World", ja: "🔙 元のワールドへ戻る" },
  "門牌提示": { en: "Door Prompt Label", ja: "ドア案内ラベル" },
  "目標世界": { en: "Target World", ja: "移動先ワールド" },
  "落點座標": { en: "Spawn Pos", ja: "到着座標" },
  "門前距": { en: "Door Dist", ja: "ドア前距離" },
  "開門條件": { en: "Unlock Req", ja: "解錠条件" },
  "上鎖提示": { en: "Locked Msg", ja: "施錠メッセージ" },
  "💾 儲存門設定": { en: "💾 Save Door", ja: "💾 ドア保存" },
  "🚪 立即進門前往": { en: "🚪 Enter Door Now", ja: "🚪 今すぐドアに入る" },
  "正在穿越門扉前往新世界...": { en: "Stepping through the door...", ja: "ドアを通って新しいワールドへ移動中..." },
  "📦 下載多世界編輯包（.stackpack.zip）": {
    en: "📦 Download Multi-World Pack (.stackpack.zip)",
    ja: "📦 マルチワールドパック (.stackpack.zip) をダウンロード",
  },
  "🌍 匯入世界／專案": { en: "🌍 Import World / Pack", ja: "🌍 ワールド／パック読込" },

  // 社畜的一天 5 世界企劃與新增物件
  "粉領女上班族": { en: "Office Lady", ja: "オフィスレディ" },
  "公車站牌候車亭": { en: "Bus Stop Shelter", ja: "バス停" },
  "精品咖啡茶水吧": { en: "Espresso Coffee Bar", ja: "カフェバー" },
  "多功能事務印表機": { en: "Office Printer", ja: "複合プリンター" },
  "浴室洗手台鏡櫃": { en: "Bathroom Vanity Sink", ja: "洗面台ミラー" },
  "🌟 多世界完整企劃劇本（門 ＋ 交通工具跨世界聯動）": {
    en: "🌟 Multi-World Playable Campaign (Linked Doors & Transit)",
    ja: "🌟 マルチワールド連動シナリオ（ドア＆バス通勤）",
  },
  "💼 社畜的一天（5 世界聯動完整企劃）": {
    en: "💼 Office Worker's Day (5 Linked Worlds)",
    ja: "💼 社畜の一日（5ワールド連動シナリオ）",
  },
  "女上班族主角・溫馨住家 ↔ 208 號固定路線公車都市 ↔ 公司辦公室 ↔ 生活大賣場 ↔ 療癒餐廳": {
    en: "Female Office Worker · Home ↔ Bus Route City ↔ Office ↔ Mall ↔ Restaurant",
    ja: "OL主人公・自宅 ↔ 208号巡回バスの街 ↔ オフィス ↔ スーパー ↔ レストラン",
  },
};

// 動態文字樣板比對（繁體中文原句 -> { en, ja } 轉換函式）
const DYNAMIC_RULES = [
  {
    re: /^\[F\] 🚪 (.+)$/,
    en: (m) => `[F] 🚪 ${m[1]}`,
    ja: (m) => `[F] 🚪 ${m[1]}`,
  },
  {
    re: /^\[F\] 🔒 (.+)$/,
    en: (m) => `[F] 🔒 ${m[1]} (Locked)`,
    ja: (m) => `[F] 🔒 ${m[1]} (施錠中)`,
  },
  {
    re: /^\[G\] (.+?) (.+?)：(.+)$/,
    en: (m) => `[G] ${m[1]} ${tr(m[2], "en")}: ${m[3]}`,
    ja: (m) => `[G] ${m[1]} ${tr(m[2], "ja")}：${m[3]}`,
  },
  {
    re: /^移動路徑 \((\d+)\)$/,
    en: (m) => `Movement Paths (${m[1]})`,
    ja: (m) => `移動ルート (${m[1]})`,
  },
  {
    re: /^自由漫遊區域 \((\d+)\)$/,
    en: (m) => `Wander Zones (${m[1]})`,
    ja: (m) => `散策エリア (${m[1]})`,
  },
  {
    re: /^節點數：(\d+) 點$/,
    en: (m) => `Waypoints: ${m[1]}`,
    ja: (m) => `経由点：${m[1]} 個`,
  },
  {
    re: /^區內成員數：(\d+) 個$/,
    en: (m) => `Members in zone: ${m[1]}`,
    ja: (m) => `エリア内メンバー：${m[1]} 体`,
  },
  {
    re: /^目前尺寸：(\d+)\s*×\s*(\d+)\s*格$/,
    en: (m) => `Current size: ${m[1]} × ${m[2]} tiles`,
    ja: (m) => `現在のサイズ：${m[1]} × ${m[2]} マス`,
  },
  {
    re: /^(\d+)\s*\/\s*(\d+)\s*分$/,
    en: (m) => `${m[1]} / ${m[2]} pts`,
    ja: (m) => `${m[1]} / ${m[2]} 点`,
  },
  {
    re: /^\[F\] 或點擊(.+?)：(.+)$/,
    en: (m) => `[F] or Click ${tr(m[1], "en")}: ${m[2]}`,
    ja: (m) => `[F] またはクリックで${tr(m[1], "ja")}：${m[2]}`,
  },
  {
    re: /^\[F\] (.+?)：(.+)$/,
    en: (m) => `[F] ${tr(m[1], "en")}: ${m[2]}`,
    ja: (m) => `[F] ${tr(m[1], "ja")}：${m[2]}`,
  },
  {
    re: /^👆 點擊(.+?)：(.+)$/,
    en: (m) => `👆 Click ${tr(m[1], "en")}: ${m[2]}`,
    ja: (m) => `👆 クリックで${tr(m[1], "ja")}：${m[2]}`,
  },
  {
    re: /^⭐ (.+)$/,
    en: (m) => (m[1] === "冒險玩家" ? "⭐ Adventurer" : `⭐ ${m[1]}`),
    ja: (m) => (m[1] === "冒險玩家" ? "⭐ 冒険プレイヤー" : `⭐ ${m[1]}`),
  },
  {
    re: /^🏰 (.+?) \((\d+×\d+)\)・(\d+) 物件$/,
    en: (m) => `🏰 ${m[1]} (${m[2]}) · ${m[3]} objects`,
    ja: (m) => `🏰 ${m[1]} (${m[2]})・${m[3]} オブジェクト`,
  },
  {
    re: /^🎮 (.+?) \((\d+×\d+)\)$/,
    en: (m) => `🎮 ${m[1]} (${m[2]})`,
    ja: (m) => `🎮 ${m[1]} (${m[2]})`,
  },
  {
    re: /^確定從本世界資產庫移除「(.+?)」？（地圖上 (\d+) 個此物件也會同步移除）$/,
    en: (m) => `Remove "${m[1]}" from this world? (${m[2]} placed instance(s) will also be removed)`,
    ja: (m) => `「${m[1]}」をこのワールドから削除しますか？（マップ上の${m[2]}体も同時に削除されます）`,
  },
  {
    re: /^確定從本世界資產庫移除「(.+?)」？$/,
    en: (m) => `Remove "${m[1]}" from this world's library?`,
    ja: (m) => `「${m[1]}」をこのワールドのアセット一覧から削除しますか？`,
  },
  {
    re: /^世界已重新命名為「(.+?)」！$/,
    en: (m) => `World renamed to "${m[1]}"!`,
    ja: (m) => `ワールド名を「${m[1]}」に変更しました！`,
  },
  {
    re: /^已將玩家造型換成「(.+?)」！$/,
    en: (m) => `Player avatar switched to "${m[1]}"!`,
    ja: (m) => `アバターを「${m[1]}」に変更しました！`,
  },
  {
    re: /^玩家造型已更換為「(.+?)」！$/,
    en: (m) => `Player avatar changed to "${m[1]}"!`,
    ja: (m) => `プレイヤーの外見を「${m[1]}」に変更しました！`,
  },
  {
    re: /^已確認儲存「(.+?)」至個人與本世界資產庫！$/,
    en: (m) => `Saved "${m[1]}" to both personal and world libraries!`,
    ja: (m) => `「${m[1]}」を個人・ワールドライブラリに保存しました！`,
  },
  {
    re: /^✅ 已記憶 \((.+?)\)$/,
    en: (m) => `✅ Saved (${m[1]})`,
    ja: (m) => `✅ 保存済 (${m[1]})`,
  },
  {
    re: /^已重置遊玩進度並回到關卡起點，祝你挑戰成功！$|^✨ 已重置遊玩進度並回到關卡起點，祝你挑戰成功！$/,
    en: () => "✨ Progress reset! Back at the starting point — good luck!",
    ja: () => "✨ 進行状況をリセットしてスタート地点に戻りました！",
  },
];

let currentLang = detectInitialLang();
const listeners = new Set();
const originalTextMap = new WeakMap();
const originalAttrMap = new WeakMap();
let observer = null;
let isApplying = false;
let pendingFlush = false;

export function detectInitialLang() {
  try {
    const urlLang = new URLSearchParams(window.location.search).get("lang");
    if (urlLang && SUPPORTED_LANGS.some((l) => l.code === urlLang)) {
      return urlLang;
    }
  } catch { /* ignore */ }
  try {
    const saved = localStorage.getItem(LANG_STORAGE_KEY);
    if (saved && SUPPORTED_LANGS.some((l) => l.code === saved)) {
      return saved;
    }
  } catch { /* ignore */ }
  const nav = (navigator.language || "en").toLowerCase();
  if (nav.startsWith("zh")) return "zh-Hant";
  if (nav.startsWith("ja")) return "ja";
  return "en";
}

export function getLang() {
  return currentLang;
}

/// 將任意繁體中文介面字串翻譯為目標語系；找無對應時保留原文不毀損資料
export function tr(text, lang = currentLang) {
  if (text == null) return "";
  const raw = String(text);
  if (!lang || lang === "zh-Hant") return raw;

  const trimmed = raw.trim();
  if (!trimmed) return raw;

  const entry = DICT[trimmed];
  if (entry && entry[lang]) {
    const lead = raw.slice(0, raw.indexOf(trimmed));
    const trail = raw.slice(raw.indexOf(trimmed) + trimmed.length);
    return `${lead}${entry[lang]}${trail}`;
  }

  for (const rule of DYNAMIC_RULES) {
    const m = trimmed.match(rule.re);
    if (m && typeof rule[lang] === "function") {
      const lead = raw.slice(0, raw.indexOf(trimmed));
      const trail = raw.slice(raw.indexOf(trimmed) + trimmed.length);
      return `${lead}${rule[lang](m)}${trail}`;
    }
  }
  return raw;
}

function shouldSkipElement(el) {
  if (!el || el.nodeType !== Node.ELEMENT_NODE) return false;
  const tag = el.tagName;
  if (["SCRIPT", "STYLE", "CANVAS", "TEXTAREA"].includes(tag)) return true;
  if (el.hasAttribute("data-no-i18n")) return true;
  if (el.id === "worldSelect" || el.id === "langSelect") return false;
  return false;
}

/// 遞迴無損將 DOM 樹內文字與常用屬性套用為目標語系
export function applyI18nToTree(root = document.body, lang = currentLang) {
  if (!root) return;
  const wasApplying = isApplying;
  isApplying = true;
  try {
    const walk = (node) => {
      if (node.nodeType === Node.TEXT_NODE) {
        const parent = node.parentElement;
        if (!parent || shouldSkipElement(parent)) return;
        let orig = originalTextMap.get(node);
        if (orig === undefined) {
          orig = node.nodeValue;
          originalTextMap.set(node, orig);
        }
        const translated = lang === "zh-Hant" ? orig : tr(orig, lang);
        if (node.nodeValue !== translated) {
          node.nodeValue = translated;
        }
        return;
      }

      if (node.nodeType === Node.ELEMENT_NODE) {
        if (shouldSkipElement(node)) return;
        let attrRec = originalAttrMap.get(node);
        if (!attrRec) {
          attrRec = {};
          for (const attr of ["placeholder", "title", "aria-label"]) {
            if (node.hasAttribute(attr)) {
              attrRec[attr] = node.getAttribute(attr);
            }
          }
          originalAttrMap.set(node, attrRec);
        } else {
          // 若動態變更過原始屬性且目前為繁中，同步更新紀錄
          for (const attr of ["placeholder", "title", "aria-label"]) {
            if (node.hasAttribute(attr) && attrRec[attr] === undefined) {
              attrRec[attr] = node.getAttribute(attr);
            }
          }
        }
        for (const [attr, origVal] of Object.entries(attrRec)) {
          if (origVal == null) continue;
          const nextVal = lang === "zh-Hant" ? origVal : tr(origVal, lang);
          if (node.getAttribute(attr) !== nextVal) {
            node.setAttribute(attr, nextVal);
          }
        }
        for (let ch = node.firstChild; ch; ch = ch.nextSibling) {
          walk(ch);
        }
      }
    };

    walk(root);
    if (document.title) {
      if (!document.documentElement.dataset.origTitle) {
        document.documentElement.dataset.origTitle = document.title;
      }
      document.title = lang === "zh-Hant"
        ? document.documentElement.dataset.origTitle
        : tr(document.documentElement.dataset.origTitle, lang);
    }
    document.documentElement.lang = lang;
  } finally {
    isApplying = wasApplying;
  }
}

export function setLang(nextLang, persist = true) {
  const valid = SUPPORTED_LANGS.some((l) => l.code === nextLang) ? nextLang : "en";
  currentLang = valid;
  if (persist) {
    try {
      localStorage.setItem(LANG_STORAGE_KEY, valid);
    } catch { /* ignore */ }
  }
  const sel = document.getElementById("langSelect");
  if (sel && sel.value !== valid) sel.value = valid;

  applyI18nToTree(document.body, valid);
  for (const cb of listeners) {
    try { cb(valid); } catch { /* ignore */ }
  }
}

export function onLangChange(cb) {
  if (typeof cb === "function") listeners.add(cb);
  return () => listeners.delete(cb);
}

export function initI18n(selectElId = "langSelect") {
  currentLang = detectInitialLang();
  window.__stackCityI18n = {
    getLang,
    setLang,
    tr,
    onLangChange,
    applyI18nToTree,
  };

  const sel = document.getElementById(selectElId);
  if (sel) {
    sel.value = currentLang;
    sel.addEventListener("change", (e) => {
      setLang(e.target.value, true);
    });
  }

  applyI18nToTree(document.body, currentLang);

  if (!observer && typeof MutationObserver !== "undefined") {
    observer = new MutationObserver(() => {
      if (isApplying || currentLang === "zh-Hant" || pendingFlush) return;
      pendingFlush = true;
      queueMicrotask(() => {
        pendingFlush = false;
        if (!isApplying && currentLang !== "zh-Hant") {
          applyI18nToTree(document.body, currentLang);
        }
      });
    });
    observer.observe(document.body, {
      childList: true,
      subtree: true,
    });
  }
  return currentLang;
}

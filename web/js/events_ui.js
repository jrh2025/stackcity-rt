//! 右欄「路徑／漫遊管理」與「友善三步引導式事件導演編輯器」。

import { escapeHtml } from "./studio.js";

export function renderMotionPanel(container, app) {
  const paths = app.sim.paths;
  const zones = app.sim.wanderZones;
  const entities = app.sim.entities;

  let html = `
    <div class="prop-card">
      <div class="row-gap space-between">
        <strong>〰️ 移動路徑 (${paths.length})</strong>
        <button id="btnStartDrawPath" class="btn btn-sub">＋ 在地圖畫路徑</button>
      </div>`;

  if (paths.length === 0) {
    html += `<div class="hint-text">尚未設定移動路徑。選取物件後點上方按鈕即可繪製！</div>`;
  } else {
    paths.forEach((p, idx) => {
      const entOpts = `<option value="">（未指派物件）</option>` +
        entities.map((e) => `<option value="${e.id}" ${p.entity_id === e.id ? "selected" : ""}>${escapeHtml(e.name)}</option>`).join("");
      html += `
        <div class="prop-card" style="background:#162033;margin-top:4px;" data-path-idx="${idx}">
          <div class="row-gap space-between">
            <strong style="color:${p.color};">${escapeHtml(p.name)}</strong>
            <div class="row-gap">
              <label class="inline-field"><input type="checkbox" class="p-active" ${p.active ? "checked" : ""} /> 啟用</label>
              <button class="btn btn-danger p-del" style="padding:2px 6px;">✕</button>
            </div>
          </div>
          <div class="prop-row"><label>移動物件</label><select class="ctrl-select p-ent" style="flex:1;">${entOpts}</select></div>
          <div class="prop-row">
            <label>模式／速</label>
            <select class="ctrl-select ctrl-compact p-loop">
              <option value="loop" ${p.loop_mode === "loop" ? "selected" : ""}>循環 Loop</option>
              <option value="pingpong" ${p.loop_mode === "pingpong" ? "selected" : ""}>往返 PingPong</option>
              <option value="once" ${p.loop_mode === "once" ? "selected" : ""}>單程 Once</option>
            </select>
            <input type="number" class="ctrl-input p-speed" step="0.3" min="0.2" max="15" style="width:68px;" value="${p.speed}" />
          </div>
          <div class="hint-text">節點數：${p.waypoints.length} 點</div>
        </div>`;
    });
  }
  html += `</div>`;

  html += `
    <div class="prop-card">
      <div class="row-gap space-between">
        <strong>⭕ 自由漫遊區域 (${zones.length})</strong>
        <button id="btnStartDrawZone" class="btn btn-sub">＋ 放置漫遊圈</button>
      </div>`;

  if (zones.length === 0) {
    html += `<div class="hint-text">放一個圓形區域即可讓裡面的動物或市民自動自由散步。</div>`;
  } else {
    zones.forEach((z, idx) => {
      html += `
        <div class="prop-card" style="background:#162033;margin-top:4px;" data-zone-idx="${idx}">
          <div class="row-gap space-between">
            <strong style="color:${z.color};">${escapeHtml(z.name)}</strong>
            <button class="btn btn-danger z-del" style="padding:2px 6px;">✕</button>
          </div>
          <div class="prop-row"><label>半徑</label>
            <input type="range" class="z-rad" min="1.5" max="15" step="0.5" value="${z.radius}" />
            <span class="mono-badge">${z.radius}</span>
          </div>
          <div class="prop-row"><label>速度</label>
            <input type="range" class="z-spd" min="0.3" max="6" step="0.2" value="${z.speed}" />
            <span class="mono-badge">${z.speed}</span>
          </div>
          <div class="prop-row"><label>加入選定</label>
            <button class="btn btn-sub z-add-sel" style="flex:1;">＋ 將目前選取物件加入漫遊</button>
          </div>
          <div class="hint-text">區內成員數：${z.entity_ids.length} 個</div>
        </div>`;
    });
  }
  html += `</div>`;

  container.innerHTML = html;

  container.querySelector("#btnStartDrawPath").addEventListener("click", () => app.tools.setTool("path"));
  container.querySelector("#btnStartDrawZone").addEventListener("click", () => app.tools.setTool("zone"));

  container.querySelectorAll("[data-path-idx]").forEach((card) => {
    const p = paths[Number(card.dataset.pathIdx)];
    const save = () => app.savePath(p);
    card.querySelector(".p-active").addEventListener("change", (e) => { p.active = e.target.checked; save(); });
    card.querySelector(".p-ent").addEventListener("change", (e) => { p.entity_id = e.target.value || null; save(); });
    card.querySelector(".p-loop").addEventListener("change", (e) => { p.loop_mode = e.target.value; save(); });
    card.querySelector(".p-speed").addEventListener("change", (e) => { p.speed = Number(e.target.value) || 2.0; save(); });
    card.querySelector(".p-del").addEventListener("click", () => app.deletePath(p.id));
  });

  container.querySelectorAll("[data-zone-idx]").forEach((card) => {
    const z = zones[Number(card.dataset.zoneIdx)];
    card.querySelector(".z-rad").addEventListener("change", (e) => { z.radius = Number(e.target.value); app.saveZone(z); });
    card.querySelector(".z-spd").addEventListener("change", (e) => { z.speed = Number(e.target.value); app.saveZone(z); });
    card.querySelector(".z-add-sel").addEventListener("click", () => {
      const sid = app.renderer.selectedEntityId;
      if (!sid) return app.showToast("請先在畫布點擊選取一個人物或動物物件！", true);
      if (!z.entity_ids.includes(sid)) z.entity_ids.push(sid);
      app.saveZone(z);
    });
    card.querySelector(".z-del").addEventListener("click", () => app.deleteZone(z.id));
  });
}

// ---------- 友善故事事件導演（三步驟引導 + 一鍵範本 + 編輯修改 + 多動作堆疊 + 白話預覽） ----------

function getDefaultDraft(app) {
  const firstEnt = app.sim.entities[0]?.id || "";
  const firstChar = app.sim.entities.find((e) => app.assetMap.get(e.asset_id)?.category === "character")?.id || firstEnt;
  const cx = Math.round((app.worldState?.meta?.width || 40) * 0.5);
  const cy = Math.round((app.worldState?.meta?.height || 40) * 0.5);
  return {
    editingId: null,
    name: "點選見面驚喜",
    trigger_type: "click",
    trigger_config: {
      entity_id: firstChar,
      entity_a: "__player__",
      entity_b: firstChar,
      radius: 2.0,
      hour: 20,
      minute: 0,
      seconds: 12,
      x: cx,
      y: cy,
      mode: "night",
    },
    actions: [
      { type: "speech_bubble", entity_id: firstChar, text: "嗨～歡迎來到這個世界！✨", duration: 4.0 },
      { type: "play_sound", sound: "encounter" },
    ],
    cooldown_sec: 3.0,
  };
}

function buildScenarioTemplates(app) {
  const ents = app.sim.entities;
  const firstEnt = ents[0]?.id || "";
  const firstChar = ents.find((e) => app.assetMap.get(e.asset_id)?.category === "character")?.id || firstEnt;
  const selId = app.renderer.selectedEntityId || firstChar;
  const cx = Math.round((app.worldState?.meta?.width || 40) * 0.5);
  const cy = Math.round((app.worldState?.meta?.height || 40) * 0.5);

  return [
    {
      id: "greet_click",
      label: "💬 點擊問候",
      build: () => ({
        editingId: null,
        name: "點擊物件親切問候",
        trigger_type: "click",
        trigger_config: { entity_id: selId },
        actions: [
          { type: "speech_bubble", entity_id: selId, text: "你好呀！今天天氣真好～✨", duration: 4.0 },
          { type: "scale_tween", entity_id: selId, scale_mul: 1.25, duration: 0.6 },
          { type: "play_sound", sound: "interact" },
        ],
        cooldown_sec: 2.0,
      }),
    },
    {
      id: "player_welcome",
      label: "🤝 玩家迎賓",
      build: () => ({
        editingId: null,
        name: "玩家靠近自動迎賓",
        trigger_type: "encounter",
        trigger_config: { entity_a: "__player__", entity_b: selId, radius: 2.4 },
        actions: [
          { type: "speech_bubble", entity_id: selId, text: "歡迎回來！快來看看新建築吧！", duration: 4.5 },
          { type: "particle_burst", effect: "sparkle", entity_id: selId, x: cx, y: cy, z: 3, count: 24 },
          { type: "play_sound", sound: "encounter" },
        ],
        cooldown_sec: 5.0,
      }),
    },
    {
      id: "night_firework",
      label: "🎆 夜晚煙火秀",
      build: () => ({
        editingId: null,
        name: "晚上八點慶典煙火",
        trigger_type: "clock",
        trigger_config: { hour: 20, minute: 0 },
        actions: [
          { type: "particle_burst", effect: "firework", entity_id: selId || "__player__", x: cx, y: cy, z: 6, count: 48 },
          { type: "play_sound", sound: "firework" },
        ],
        cooldown_sec: 3.0,
      }),
    },
    {
      id: "night_lights",
      label: "💡 入夜全點燈",
      build: () => ({
        editingId: null,
        name: "黃昏自動點亮路燈",
        trigger_type: "day_night",
        trigger_config: { mode: "night" },
        actions: [
          { type: "emissive_toggle", tag: "night_light", on: true },
          { type: "play_sound", sound: "chime" },
        ],
        cooldown_sec: 4.0,
      }),
    },
    {
      id: "sakura_loop",
      label: "🌸 定時櫻花雨",
      build: () => ({
        editingId: null,
        name: "每 15 秒浪漫花雨",
        trigger_type: "interval",
        trigger_config: { seconds: 15 },
        actions: [
          { type: "particle_burst", effect: "sakura", entity_id: "__player__", x: cx, y: cy, z: 5, count: 36 },
          { type: "play_sound", sound: "chime" },
        ],
        cooldown_sec: 3.0,
      }),
    },
  ];
}

export function renderEventsPanel(container, app) {
  if (!app.eventDraft) {
    app.eventDraft = getDefaultDraft(app);
  }
  const draft = app.eventDraft;
  const rules = app.sim.events;
  const entities = app.sim.entities;
  const playerLabel = `⭐ 玩家（${escapeHtml(app.sim.player?.name || "冒險玩家")}）`;

  const makeActorOptions = (selectedId, includePlayer = true, emptyOpt = null) => {
    let out = emptyOpt ? `<option value="">${emptyOpt}</option>` : "";
    if (includePlayer) {
      out += `<option value="__player__" ${selectedId === "__player__" ? "selected" : ""}>${playerLabel}</option>`;
    }
    out += entities
      .map((e) => `<option value="${e.id}" ${selectedId === e.id ? "selected" : ""}>${escapeHtml(e.name)}</option>`)
      .join("");
    return out;
  };

  const templates = buildScenarioTemplates(app);

  let html = `
    <div class="prop-card">
      <div class="row-gap space-between">
        <strong>🤝 全域相遇自動互動</strong>
        <label class="inline-field">
          <input id="chkAutoEncounter" type="checkbox" ${app.sim.autoEncounterEnabled ? "checked" : ""} />
          <span>自動對話反應</span>
        </label>
      </div>
      <div class="hint-text">開啟後，玩家漫步靠近角色／動物，或兩移動物件相遇會自動打招呼並產生特效！</div>
    </div>

    <div class="ev-step-card">
      <div class="row-gap space-between" style="margin-bottom:4px;">
        <strong>⚡ 一鍵套用情境範本</strong>
        <span class="hint-text">點擊即填入下方</span>
      </div>
      <div class="ev-template-row">
        ${templates.map((tp) => `<button class="chip-btn ev-tpl-btn" data-tpl="${tp.id}">${tp.label}</button>`).join("")}
      </div>
    </div>

    <div class="ev-step-card" id="evWizardCard">
      <div class="row-gap space-between">
        <strong>${draft.editingId ? "✏️ 編輯事件規則" : "🎬 故事事件精靈"}</strong>
        ${draft.editingId ? `<button id="btnCancelEditEv" class="btn-mini">取消編輯</button>` : ""}
      </div>

      <div class="prop-row">
        <label>事件名稱</label>
        <input id="evDraftName" class="ctrl-input" placeholder="例如：遇見大法師放煙火" value="${escapeHtml(draft.name)}" />
      </div>

      <div class="ev-step-badge">步驟 1｜什麼時候發生？（觸發條件）</div>
      <div class="ev-trigger-grid">
        ${[
          ["click", "🖱️ 點擊物件"],
          ["encounter", "🤝 相遇距離"],
          ["clock", "⏰ 遊戲定時"],
          ["interval", "🔁 定時重複"],
          ["proximity", "📍 靠近座標"],
          ["day_night", "🌗 日夜交替"],
        ].map(([k, lbl]) => `
          <button class="chip-btn ev-trig-btn ${draft.trigger_type === k ? "active" : ""}" data-trig="${k}">${lbl}</button>
        `).join("")}
      </div>
      <div id="evTrigConfigBox" class="ev-sub-box"></div>

      <div class="ev-step-badge">步驟 2｜發生什麼事情？（可堆疊多個動作）</div>
      <div id="evActionsBox" class="ev-actions-list"></div>
      <button id="btnAddEvAction" class="btn btn-sub" style="width:100%;">＋ 再加一個動作（說話／煙火／櫻花／音效／天氣）</button>

      <div class="ev-step-badge">步驟 3｜白話預覽 & 執行</div>
      <div id="evPlainPreview" class="ev-preview-box"></div>

      <div class="row-gap">
        <button id="btnPreviewDraftEv" class="btn btn-sub" style="flex:1;">▶ 先試播看看</button>
        <button id="btnSaveDraftEv" class="btn btn-primary" style="flex:1.3;">${draft.editingId ? "💾 儲存修改" : "💾 儲存此規則"}</button>
      </div>
    </div>

    <div class="prop-card">
      <strong>📋 已建立規則 (${rules.length})</strong>
      <div id="evListWrap" style="display:flex;flex-direction:column;gap:6px;margin-top:4px;">`;

  if (rules.length === 0) {
    html += `<div class="hint-text">目前尚無自訂事件，可點擊上方「一鍵範本」快速建立！</div>`;
  } else {
    rules.forEach((r, idx) => {
      html += `
        <div class="prop-card" style="background:#162033;" data-ev-idx="${idx}">
          <div class="row-gap space-between">
            <strong style="font-size:12px;">${escapeHtml(r.name)}</strong>
            <div class="row-gap">
              <button class="btn-mini ev-test" title="立即試播">▶ 試播</button>
              <button class="btn-mini ev-edit" title="載入到精靈中編輯">✏️ 編輯</button>
              <input type="checkbox" class="ev-en" ${r.enabled ? "checked" : ""} title="啟用或停用此規則" />
              <button class="btn btn-danger ev-del" style="padding:2px 6px;">✕</button>
            </div>
          </div>
          <div class="hint-text">${describeRulePlain(r, app)}</div>
        </div>`;
    });
  }
  html += `</div></div>`;
  container.innerHTML = html;

  // ---------- 填充步驟 1（Trigger 參數區） ----------
  const trigBox = container.querySelector("#evTrigConfigBox");
  const cfg = draft.trigger_config || (draft.trigger_config = {});
  const t = draft.trigger_type;

  const refreshPreview = () => {
    const prevEl = container.querySelector("#evPlainPreview");
    if (prevEl) {
      prevEl.textContent = describeRulePlain(
        { trigger_type: draft.trigger_type, trigger_config: draft.trigger_config, actions: draft.actions },
        app,
      );
    }
  };

  if (t === "click") {
    trigBox.innerHTML = `
      <div class="prop-row">
        <label>被點擊物件</label>
        <select id="tpClickEnt" class="ctrl-select" style="flex:1;">${makeActorOptions(cfg.entity_id, false)}</select>
      </div>
      <div class="row-gap space-between">
        <span class="hint-text">提示：先點畫布物件，再按右側按鈕</span>
        <button id="btnPickSelTrig" class="btn-mini">🎯 帶入目前選中</button>
      </div>`;
    trigBox.querySelector("#tpClickEnt").addEventListener("change", (e) => {
      cfg.entity_id = e.target.value;
      refreshPreview();
    });
    trigBox.querySelector("#btnPickSelTrig").addEventListener("click", () => {
      if (!app.renderer.selectedEntityId) return app.showToast("請先在畫布選取一個物件！", true);
      cfg.entity_id = app.renderer.selectedEntityId;
      renderEventsPanel(container, app);
    });
  } else if (t === "encounter") {
    trigBox.innerHTML = `
      <div class="prop-row">
        <label>物件 A</label>
        <select id="tpEntA" class="ctrl-select" style="flex:1;">${makeActorOptions(cfg.entity_a || "__player__", true)}</select>
      </div>
      <div class="prop-row">
        <label>遇見 B</label>
        <select id="tpEntB" class="ctrl-select" style="flex:1;">${makeActorOptions(cfg.entity_b, true)}</select>
        <button id="btnPickSelTrigB" class="btn-mini" title="將目前選中物件設為 B">🎯</button>
      </div>
      <div class="prop-row">
        <label>感應距離</label>
        <input id="tpRad" type="range" min="0.8" max="6.0" step="0.2" value="${cfg.radius ?? 2.0}" />
        <span id="tpRadVal" class="mono-badge">${cfg.radius ?? 2.0} 格</span>
      </div>`;
    trigBox.querySelector("#tpEntA").addEventListener("change", (e) => { cfg.entity_a = e.target.value; refreshPreview(); });
    trigBox.querySelector("#tpEntB").addEventListener("change", (e) => { cfg.entity_b = e.target.value; refreshPreview(); });
    trigBox.querySelector("#tpRad").addEventListener("input", (e) => {
      cfg.radius = Number(e.target.value);
      trigBox.querySelector("#tpRadVal").textContent = `${cfg.radius} 格`;
      refreshPreview();
    });
    trigBox.querySelector("#btnPickSelTrigB").addEventListener("click", () => {
      if (!app.renderer.selectedEntityId) return app.showToast("請先在畫布選取一個物件！", true);
      cfg.entity_b = app.renderer.selectedEntityId;
      renderEventsPanel(container, app);
    });
  } else if (t === "clock") {
    trigBox.innerHTML = `
      <div class="row-gap">
        <span>每天</span>
        <input id="tpHour" type="number" min="0" max="23" value="${cfg.hour ?? 20}" class="ctrl-input" style="width:62px;" />
        <span>時</span>
        <input id="tpMin" type="number" min="0" max="59" value="${cfg.minute ?? 0}" class="ctrl-input" style="width:62px;" />
        <span>分觸發</span>
      </div>`;
    trigBox.querySelector("#tpHour").addEventListener("change", (e) => { cfg.hour = Number(e.target.value); refreshPreview(); });
    trigBox.querySelector("#tpMin").addEventListener("change", (e) => { cfg.minute = Number(e.target.value); refreshPreview(); });
  } else if (t === "interval") {
    trigBox.innerHTML = `
      <div class="row-gap">
        <span>每隔</span>
        <input id="tpSec" type="number" min="3" max="300" value="${cfg.seconds ?? 12}" class="ctrl-input" style="width:74px;" />
        <span>秒自動重複一次</span>
      </div>`;
    trigBox.querySelector("#tpSec").addEventListener("change", (e) => { cfg.seconds = Number(e.target.value); refreshPreview(); });
  } else if (t === "proximity") {
    trigBox.innerHTML = `
      <div class="prop-row">
        <label>移動角色</label>
        <select id="tpProxEnt" class="ctrl-select" style="flex:1;">${makeActorOptions(cfg.entity_id || "__player__", true)}</select>
      </div>
      <div class="row-gap">
        <span>接近 X:</span><input id="tpX" type="number" step="0.5" value="${Number(cfg.x ?? 20).toFixed(1)}" class="ctrl-input" style="width:62px;" />
        <span>Y:</span><input id="tpY" type="number" step="0.5" value="${Number(cfg.y ?? 20).toFixed(1)}" class="ctrl-input" style="width:62px;" />
        <button id="btnGrabPosTrig" class="btn-mini">📍 抓目前位置</button>
      </div>`;
    trigBox.querySelector("#tpProxEnt").addEventListener("change", (e) => { cfg.entity_id = e.target.value; refreshPreview(); });
    trigBox.querySelector("#tpX").addEventListener("change", (e) => { cfg.x = Number(e.target.value); refreshPreview(); });
    trigBox.querySelector("#tpY").addEventListener("change", (e) => { cfg.y = Number(e.target.value); refreshPreview(); });
    trigBox.querySelector("#btnGrabPosTrig").addEventListener("click", () => {
      const sel = app.sim.entities.find((x) => x.id === app.renderer.selectedEntityId) || app.sim.player;
      cfg.x = Number(sel.x.toFixed(1));
      cfg.y = Number(sel.y.toFixed(1));
      renderEventsPanel(container, app);
    });
  } else {
    trigBox.innerHTML = `
      <div class="prop-row">
        <label>切換時刻</label>
        <select id="tpMode" class="ctrl-select" style="flex:1;">
          <option value="night" ${cfg.mode !== "day" ? "selected" : ""}>🌆 入夜點燈時刻 (18:30)</option>
          <option value="day" ${cfg.mode === "day" ? "selected" : ""}>🌅 清晨天亮時刻 (06:00)</option>
        </select>
      </div>`;
    trigBox.querySelector("#tpMode").addEventListener("change", (e) => { cfg.mode = e.target.value; refreshPreview(); });
  }

  // ---------- 填充步驟 2（多動作卡片編輯器） ----------
  const actBox = container.querySelector("#evActionsBox");
  const actionLabels = [
    ["speech_bubble", "💬 跳出對話氣泡"],
    ["particle_burst", "🎆 施放煙火 / 櫻花 / 星光"],
    ["scale_tween", "✨ 物件彈跳一下"],
    ["emissive_toggle", "💡 切換夜燈發光"],
    ["play_sound", "🔊 播放互動音效"],
    ["set_weather", "🌤️ 切換天氣狀態"],
    ["set_time", "🕒 跳轉日夜時鐘"],
  ];

  actBox.innerHTML = "";
  (draft.actions || []).forEach((act, idx) => {
    const item = document.createElement("div");
    item.className = "ev-action-item";
    const typeOpts = actionLabels.map(([k, lbl]) =>
      `<option value="${k}" ${act.type === k ? "selected" : ""}>${lbl}</option>`).join("");

    let bodyHtml = "";
    if (act.type === "speech_bubble") {
      bodyHtml = `
        <div class="prop-row">
          <label>誰說話</label>
          <select class="ctrl-select a-ent" style="flex:1;">${makeActorOptions(act.entity_id || entities[0]?.id || "__player__", true)}</select>
        </div>
        <input class="ctrl-input a-text" placeholder="輸入氣泡對話文字" value="${escapeHtml(act.text || "好棒呀！✨")}" />`;
    } else if (act.type === "particle_burst") {
      const followId = act.entity_id || "";
      bodyHtml = `
        <div class="prop-row">
          <label>特效／位置</label>
          <select class="ctrl-select ctrl-compact a-eff">
            <option value="firework" ${act.effect === "firework" ? "selected" : ""}>🎆 煙火</option>
            <option value="sakura" ${act.effect === "sakura" ? "selected" : ""}>🌸 櫻花</option>
            <option value="sparkle" ${act.effect === "sparkle" ? "selected" : ""}>✨ 星光</option>
          </select>
          <select class="ctrl-select a-ent" style="flex:1;">${makeActorOptions(followId, true, "📍 固定於世界中央")}</select>
        </div>`;
    } else if (act.type === "scale_tween") {
      bodyHtml = `
        <div class="prop-row">
          <label>彈跳對象</label>
          <select class="ctrl-select a-ent" style="flex:1;">${makeActorOptions(act.entity_id || "__player__", true)}</select>
        </div>`;
    } else if (act.type === "emissive_toggle") {
      bodyHtml = `
        <div class="row-gap">
          <select class="ctrl-select a-on">
            <option value="true" ${act.on !== false ? "selected" : ""}>💡 開啟夜間發光</option>
            <option value="false" ${act.on === false ? "selected" : ""}>🌙 關閉夜間發光</option>
          </select>
          <input class="ctrl-input a-tag" style="flex:1;" placeholder="標籤（預設 night_light）" value="${escapeHtml(act.tag || "night_light")}" />
        </div>`;
    } else if (act.type === "play_sound") {
      bodyHtml = `
        <select class="ctrl-select a-sound">
          <option value="encounter" ${act.sound === "encounter" ? "selected" : ""}>✨ 歡快相遇</option>
          <option value="firework" ${act.sound === "firework" ? "selected" : ""}>🎆 煙火慶典</option>
          <option value="bell" ${act.sound === "bell" ? "selected" : ""}>🔔 鐘聲鳴響</option>
          <option value="chime" ${act.sound === "chime" ? "selected" : ""}>🎐 風鈴清響</option>
          <option value="horn" ${act.sound === "horn" ? "selected" : ""}>🎺 可愛喇叭</option>
          <option value="bird" ${act.sound === "bird" ? "selected" : ""}>🐦 清脆鳥鳴</option>
        </select>`;
    } else if (act.type === "set_weather") {
      bodyHtml = `
        <select class="ctrl-select a-weather">
          <option value="clear" ${act.weather === "clear" ? "selected" : ""}>☀️ 放晴天氣</option>
          <option value="rain" ${act.weather === "rain" ? "selected" : ""}>🌧️ 下雨細雨</option>
          <option value="snow" ${act.weather === "snow" ? "selected" : ""}>❄️ 飄雪紛飛</option>
        </select>`;
    } else if (act.type === "set_time") {
      bodyHtml = `
        <div class="prop-row">
          <label>跳轉至</label>
          <input type="number" min="0" max="23" class="ctrl-input a-hour" style="width:72px;" value="${act.hour ?? 20}" />
          <span>點整</span>
        </div>`;
    }

    item.innerHTML = `
      <div class="row-gap space-between">
        <div class="row-gap" style="flex:1;">
          <span class="mono-badge">#${idx + 1}</span>
          <select class="ctrl-select a-type" style="flex:1;">${typeOpts}</select>
        </div>
        <button class="btn-mini a-del" title="移除此動作">✕</button>
      </div>
      ${bodyHtml}`;

    item.querySelector(".a-type").addEventListener("change", (e) => {
      const nextType = e.target.value;
      const defaultEnt = app.renderer.selectedEntityId || entities[0]?.id || "__player__";
      const cx = Math.round((app.worldState?.meta?.width || 40) * 0.5);
      const cy = Math.round((app.worldState?.meta?.height || 40) * 0.5);
      const templatesByType = {
        speech_bubble: { type: "speech_bubble", entity_id: defaultEnt, text: "好巧呀～✨", duration: 4.0 },
        particle_burst: { type: "particle_burst", effect: "firework", entity_id: defaultEnt, x: cx, y: cy, z: 5, count: 40 },
        scale_tween: { type: "scale_tween", entity_id: defaultEnt, scale_mul: 1.25, duration: 0.6 },
        emissive_toggle: { type: "emissive_toggle", tag: "night_light", on: true },
        play_sound: { type: "play_sound", sound: "chime" },
        set_weather: { type: "set_weather", weather: "snow" },
        set_time: { type: "set_time", hour: 20 },
      };
      draft.actions[idx] = templatesByType[nextType] || { type: nextType };
      renderEventsPanel(container, app);
    });

    item.querySelector(".a-del").addEventListener("click", () => {
      if (draft.actions.length <= 1) return app.showToast("事件至少需保留 1 個動作！", true);
      draft.actions.splice(idx, 1);
      renderEventsPanel(container, app);
    });

    item.querySelector(".a-ent")?.addEventListener("change", (e) => {
      act.entity_id = e.target.value || undefined;
      refreshPreview();
    });
    item.querySelector(".a-text")?.addEventListener("input", (e) => {
      act.text = e.target.value;
      refreshPreview();
    });
    item.querySelector(".a-eff")?.addEventListener("change", (e) => {
      act.effect = e.target.value;
      refreshPreview();
    });
    item.querySelector(".a-on")?.addEventListener("change", (e) => {
      act.on = e.target.value === "true";
      refreshPreview();
    });
    item.querySelector(".a-tag")?.addEventListener("input", (e) => {
      act.tag = e.target.value;
      refreshPreview();
    });
    item.querySelector(".a-sound")?.addEventListener("change", (e) => {
      act.sound = e.target.value;
      refreshPreview();
    });
    item.querySelector(".a-weather")?.addEventListener("change", (e) => {
      act.weather = e.target.value;
      refreshPreview();
    });
    item.querySelector(".a-hour")?.addEventListener("change", (e) => {
      act.hour = Number(e.target.value);
      refreshPreview();
    });

    actBox.appendChild(item);
  });

  refreshPreview();

  // ---------- 全域與動作按鈕綁定 ----------
  container.querySelector("#chkAutoEncounter").addEventListener("change", (e) => {
    app.sim.autoEncounterEnabled = e.target.checked;
  });

  container.querySelectorAll(".ev-tpl-btn").forEach((b) => {
    b.addEventListener("click", () => {
      const tpl = templates.find((x) => x.id === b.dataset.tpl);
      if (!tpl) return;
      app.eventDraft = tpl.build();
      renderEventsPanel(container, app);
      app.showToast(`已套用「${tpl.label}」範本，可直接試播或儲存！`);
    });
  });

  container.querySelector("#evDraftName").addEventListener("input", (e) => {
    draft.name = e.target.value;
  });

  container.querySelectorAll(".ev-trig-btn").forEach((b) => {
    b.addEventListener("click", () => {
      draft.trigger_type = b.dataset.trig;
      renderEventsPanel(container, app);
    });
  });

  container.querySelector("#btnAddEvAction").addEventListener("click", () => {
    const cx = Math.round((app.worldState?.meta?.width || 40) * 0.5);
    const cy = Math.round((app.worldState?.meta?.height || 40) * 0.5);
    draft.actions.push({
      type: "particle_burst",
      effect: "sparkle",
      entity_id: app.renderer.selectedEntityId || "__player__",
      x: cx,
      y: cy,
      z: 4,
      count: 28,
    });
    renderEventsPanel(container, app);
  });

  container.querySelector("#btnPreviewDraftEv").addEventListener("click", () => {
    app.sim.executeRule({ id: "__preview__", actions: draft.actions });
    app.showToast("已即時試播目前設定的事件效果！");
  });

  container.querySelector("#btnCancelEditEv")?.addEventListener("click", () => {
    app.eventDraft = getDefaultDraft(app);
    renderEventsPanel(container, app);
  });

  container.querySelector("#btnSaveDraftEv").addEventListener("click", async () => {
    const cleanName = (draft.name || "").trim() || "自訂故事事件";
    const trigCfg = buildCleanTriggerConfig(draft.trigger_type, draft.trigger_config);
    const payload = {
      name: cleanName,
      enabled: true,
      trigger_type: draft.trigger_type,
      trigger_config: trigCfg,
      actions: JSON.parse(JSON.stringify(draft.actions || [])),
      cooldown_sec: draft.cooldown_sec || 3.0,
    };
    if (draft.editingId) {
      const existing = rules.find((r) => r.id === draft.editingId);
      if (existing) {
        Object.assign(existing, payload);
        await app.updateEventRule(existing);
        app.eventDraft = getDefaultDraft(app);
        app.showToast(`已更新規則「${cleanName}」！`);
        renderEventsPanel(container, app);
        return;
      }
    }
    app.eventDraft = getDefaultDraft(app);
    await app.createEventRule(payload);
  });

  container.querySelectorAll("[data-ev-idx]").forEach((card) => {
    const r = rules[Number(card.dataset.evIdx)];
    card.querySelector(".ev-test").addEventListener("click", () => {
      app.sim.executeRule(r);
      app.showToast(`已試播事件：「${r.name}」`);
    });
    card.querySelector(".ev-edit").addEventListener("click", () => {
      const base = getDefaultDraft(app);
      app.eventDraft = {
        ...base,
        editingId: r.id,
        name: r.name,
        trigger_type: r.trigger_type,
        trigger_config: { ...base.trigger_config, ...(r.trigger_config || {}) },
        actions: JSON.parse(JSON.stringify(r.actions || base.actions)),
        cooldown_sec: r.cooldown_sec || 3.0,
      };
      renderEventsPanel(container, app);
    });
    card.querySelector(".ev-en").addEventListener("change", (e) => {
      r.enabled = e.target.checked;
      app.updateEventRule(r);
    });
    card.querySelector(".ev-del").addEventListener("click", () => app.deleteEventRule(r.id));
  });
}

function buildCleanTriggerConfig(type, cfg) {
  if (type === "click") return { entity_id: cfg.entity_id || "" };
  if (type === "encounter") {
    return {
      entity_a: cfg.entity_a || "__player__",
      entity_b: cfg.entity_b || "",
      radius: Number(cfg.radius || 2.0),
    };
  }
  if (type === "clock") return { hour: Number(cfg.hour ?? 20), minute: Number(cfg.minute ?? 0) };
  if (type === "interval") return { seconds: Math.max(3, Number(cfg.seconds ?? 12)) };
  if (type === "proximity") {
    return {
      entity_id: cfg.entity_id || "__player__",
      x: Number(cfg.x ?? 20),
      y: Number(cfg.y ?? 20),
      radius: Number(cfg.radius || 2.6),
    };
  }
  return { mode: cfg.mode || "night" };
}

function actorName(id, app) {
  if (!id) return "中央區域";
  if (id === "__player__") return `玩家(${app.sim.player?.name || "冒險玩家"})`;
  return app.sim.entities.find((e) => e.id === id)?.name || "指定物件";
}

export function describeRulePlain(r, app) {
  const c = r.trigger_config || {};
  let whenText = "";
  switch (r.trigger_type) {
    case "click":
      whenText = `點擊「${actorName(c.entity_id, app)}」`;
      break;
    case "encounter":
      whenText = `「${actorName(c.entity_a, app)}」遇見「${actorName(c.entity_b, app)}」(≤${c.radius || 1.8}格)`;
      break;
    case "clock":
      whenText = `每天 ${String(c.hour ?? 0).padStart(2, "0")}:${String(c.minute ?? 0).padStart(2, "0")}`;
      break;
    case "interval":
      whenText = `每 ${c.seconds || 10} 秒`;
      break;
    case "proximity":
      whenText = `「${actorName(c.entity_id, app)}」走到 (${Number(c.x ?? 0).toFixed(0)}, ${Number(c.y ?? 0).toFixed(0)})`;
      break;
    case "day_night":
      whenText = c.mode === "day" ? "清晨日出 (06:00)" : "黃昏入夜 (18:30)";
      break;
    default:
      whenText = r.trigger_type;
  }

  const acts = (r.actions || []).map((a) => {
    if (a.type === "speech_bubble") return `說話「${(a.text || "").slice(0, 12)}」`;
    if (a.type === "particle_burst") {
      return a.effect === "firework" ? "放煙火" : (a.effect === "sakura" ? "飄櫻花" : "星光特效");
    }
    if (a.type === "scale_tween") return "彈跳";
    if (a.type === "emissive_toggle") return a.on === false ? "關夜燈" : "開夜燈";
    if (a.type === "play_sound") return "播音效";
    if (a.type === "set_weather") return `換天氣(${a.weather})`;
    if (a.type === "set_time") return `調為${a.hour}點`;
    return a.type;
  });

  return `當 ${whenText} → ${acts.join(" ＋ ") || "無動作"}`;
}

//! 工具模式控制器：選取、場景拖曳、生態規則筆刷、地形／室內地貌塑形、智慧鋪路與右欄 XYZ 縮放屬性編輯器。

import { describeRulePlain } from "./events_ui.js";
import { escapeHtml } from "./studio.js";

export class ToolController {
  constructor(app) {
    this.app = app;
    this.activeTool = "select"; // select | pan | place | brush | terrain | road | path | zone | erase
    this.terrainMode = "raise";
    this.roadMode = "asphalt";
    this.brushRadius = 3.5;
    this.placeYaw = 0;
    this.placeScale = 1.0;
    this.scaleLockRatio = true;
    this._lastInspectorEntityId = null;

    // 生態筆刷規則設定
    this.ecoConfig = {
      preset: "forest", // forest | meadow | pasture | town | indoor_decor | custom
      density: 3,
      minSpacing: 1.3,
      randomScale: true,
      randomYaw: true,
      avoidWaterRoad: true,
      customAssetIds: new Set(),
    };

    this._bindToolBar();
  }

  _bindToolBar() {
    document.querySelectorAll("#toolPalette .tool-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        this.setTool(btn.dataset.tool);
      });
    });
    this.renderSubOptions();
  }

  setTool(tool) {
    this.activeTool = tool;
    document.querySelectorAll("#toolPalette .tool-btn").forEach((b) => {
      b.classList.toggle("active", b.dataset.tool === tool);
    });
    if (tool !== "path") {
      this.app.renderer.draftPathWaypoints = [];
    }
    this.app.renderer.brushPreview.active = tool === "brush";
    this.app.renderer.brushPreview.radius = this.brushRadius;
    this.app.glCanvas.style.cursor = tool === "pan" ? "grab" : "default";
    this.renderSubOptions();
  }

  renderSubOptions() {
    const el = document.getElementById("subToolOptions");
    el.innerHTML = "";
    if (this.activeTool === "pan") {
      el.innerHTML = `<div class="hint-text">左鍵拖曳畫布即可平移場景；中鍵或按住 Space + 左鍵也可隨時拖曳。</div>`;
    } else if (this.activeTool === "terrain") {
      const modes = [
        ["raise", "⬆️ 抬高"], ["lower", "⬇️ 降低"], ["water", "🌊 水體"],
        ["grass", "🟩 草地"], ["sand", "🟨 沙地"], ["stone", "⬜ 石地"],
        ["brick", "🟥 紅磚"], ["snow", "❄️ 雪地"],
        ["wood_floor", "🪵 木地板"], ["tile_floor", "◽ 地磚"],
        ["carpet_red", "🟥 紅地毯"], ["carpet_blue", "🟦 藍地毯"],
        ["concrete", "⬛ 水泥地"], ["wall_stone", "🧱 石牆"],
        ["wall_cream", "🏛️ 白牆"], ["wall_glass", "🪟 玻璃牆"],
      ];
      const row = document.createElement("div");
      row.className = "chip-row";
      modes.forEach(([k, label]) => {
        const b = document.createElement("button");
        b.className = `chip-btn ${this.terrainMode === k ? "active" : ""}`;
        b.textContent = label;
        b.addEventListener("click", () => { this.terrainMode = k; this.renderSubOptions(); });
        row.appendChild(b);
      });
      el.appendChild(row);
    } else if (this.activeTool === "road") {
      const modes = [
        ["asphalt", "🛣️ 柏油大道"], ["stone", "🪨 石板道"],
        ["dirt", "🥾 泥土小徑"], ["bridge", "🌉 木橋"], ["none", "✖️ 清除道路"],
      ];
      const row = document.createElement("div");
      row.className = "chip-row";
      modes.forEach(([k, label]) => {
        const b = document.createElement("button");
        b.className = `chip-btn ${this.roadMode === k ? "active" : ""}`;
        b.textContent = label;
        b.addEventListener("click", () => { this.roadMode = k; this.renderSubOptions(); });
        row.appendChild(b);
      });
      el.appendChild(row);
    } else if (this.activeTool === "place") {
      el.innerHTML = `
        <div class="prop-row"><label>投放角度</label>
          <input id="subYawInput" type="range" min="0" max="345" step="15" value="${this.placeYaw}">
          <span>${this.placeYaw}°</span>
        </div>
        <div class="prop-row"><label>投放縮放</label>
          <input id="subScaleInput" type="range" min="0.25" max="4.0" step="0.05" value="${this.placeScale}">
          <span>${this.placeScale.toFixed(2)}x</span>
        </div>`;
      el.querySelector("#subYawInput").addEventListener("input", (e) => {
        this.placeYaw = Number(e.target.value);
        this.renderSubOptions();
      });
      el.querySelector("#subScaleInput").addEventListener("input", (e) => {
        this.placeScale = Number(e.target.value);
        this.renderSubOptions();
      });
    } else if (this.activeTool === "brush") {
      const ecoPresets = [
        ["forest", "🌲 森林混林"],
        ["meadow", "🌻 花田農野"],
        ["pasture", "🐑 牧場動物"],
        ["town", "🏘️ 城鎮聚落"],
        ["indoor_decor", "🪑 室內家具"],
        ["custom", "✨ 自訂多選池"],
      ];
      let html = `<div class="chip-row" id="ecoPresetChips">`;
      ecoPresets.forEach(([k, label]) => {
        html += `<button class="chip-btn ${this.ecoConfig.preset === k ? "active" : ""}" data-eco="${k}">${label}</button>`;
      });
      html += `</div>
        <div class="prop-row"><label>筆刷半徑</label>
          <input id="ecoRad" type="range" min="1.5" max="8.0" step="0.5" value="${this.brushRadius}">
          <span>${this.brushRadius}</span>
        </div>
        <div class="prop-row"><label>單次密度</label>
          <input id="ecoDen" type="range" min="1" max="6" step="1" value="${this.ecoConfig.density}">
          <span>${this.ecoConfig.density} 個</span>
        </div>
        <div class="prop-row"><label>最小間距</label>
          <input id="ecoSpace" type="range" min="0.6" max="2.8" step="0.2" value="${this.ecoConfig.minSpacing}">
          <span>${this.ecoConfig.minSpacing.toFixed(1)}</span>
        </div>
        <div class="row-gap">
          <label class="inline-field"><input id="ecoRandScale" type="checkbox" ${this.ecoConfig.randomScale ? "checked" : ""}> 隨機大小</label>
          <label class="inline-field"><input id="ecoRandYaw" type="checkbox" ${this.ecoConfig.randomYaw ? "checked" : ""}> 隨機朝向</label>
          <label class="inline-field"><input id="ecoAvoid" type="checkbox" ${this.ecoConfig.avoidWaterRoad ? "checked" : ""}> 避開水路牆</label>
        </div>`;

      if (this.ecoConfig.preset === "custom") {
        html += `<div class="hint-text">勾選要納入隨機撒出的資產（可複選）：</div><div class="chip-row" style="max-height:92px;overflow-y:auto;" id="ecoCustomPool">`;
        for (const a of this.app.assetMap.values()) {
          const checked = this.ecoConfig.customAssetIds.has(a.id);
          html += `<button class="chip-btn ${checked ? "active" : ""}" data-cid="${a.id}">${escapeHtml(a.name)}</button>`;
        }
        html += `</div>`;
      }
      el.innerHTML = html;

      el.querySelectorAll("[data-eco]").forEach((b) => {
        b.addEventListener("click", () => {
          this.ecoConfig.preset = b.dataset.eco;
          this.renderSubOptions();
        });
      });
      el.querySelectorAll("[data-cid]").forEach((b) => {
        b.addEventListener("click", () => {
          const id = b.dataset.cid;
          if (this.ecoConfig.customAssetIds.has(id)) this.ecoConfig.customAssetIds.delete(id);
          else this.ecoConfig.customAssetIds.add(id);
          this.renderSubOptions();
        });
      });
      el.querySelector("#ecoRad").addEventListener("input", (e) => {
        this.brushRadius = Number(e.target.value);
        this.app.renderer.brushPreview.radius = this.brushRadius;
        this.renderSubOptions();
      });
      el.querySelector("#ecoDen").addEventListener("input", (e) => {
        this.ecoConfig.density = Number(e.target.value);
        this.renderSubOptions();
      });
      el.querySelector("#ecoSpace").addEventListener("input", (e) => {
        this.ecoConfig.minSpacing = Number(e.target.value);
        this.renderSubOptions();
      });
      el.querySelector("#ecoRandScale").addEventListener("change", (e) => { this.ecoConfig.randomScale = e.target.checked; });
      el.querySelector("#ecoRandYaw").addEventListener("change", (e) => { this.ecoConfig.randomYaw = e.target.checked; });
      el.querySelector("#ecoAvoid").addEventListener("change", (e) => { this.ecoConfig.avoidWaterRoad = e.target.checked; });
    } else if (this.activeTool === "path") {
      el.innerHTML = `
        <div class="hint-text">左鍵在地圖依序點擊路徑點，按下方「完成路徑」即刻綁定給選中物件。</div>
        <div class="row-gap">
          <button id="btnFinishPath" class="btn btn-primary" style="flex:1;">✅ 完成並綁定路徑 (${this.app.renderer.draftPathWaypoints.length}點)</button>
          <button id="btnClearDraftPath" class="btn btn-sub">清除</button>
        </div>`;
      el.querySelector("#btnFinishPath").addEventListener("click", () => this.app.commitDraftPath());
      el.querySelector("#btnClearDraftPath").addEventListener("click", () => {
        this.app.renderer.draftPathWaypoints = [];
        this.renderSubOptions();
      });
    } else if (this.activeTool === "zone") {
      el.innerHTML = `
        <div class="prop-row"><label>漫遊半徑</label>
          <input id="subZoneRad" type="range" min="2" max="14" step="0.5" value="${this.brushRadius}">
          <span>${this.brushRadius} 格</span>
        </div>
        <div class="hint-text">點擊地圖任一位置即可建立自由漫遊圓形區域。</div>`;
      el.querySelector("#subZoneRad").addEventListener("input", (e) => {
        this.brushRadius = Number(e.target.value);
        this.renderSubOptions();
      });
    }
  }

  /// 繪製右欄「物件屬性編輯器」（含長寬高 XYZ 分別縮放、固定比例、附掛解除、直連路徑／漫遊／事件與任務設定）
  renderInspector(container, entity) {
    if (!entity) {
      this._lastInspectorEntityId = null;
      container.innerHTML = `
        <div class="prop-card">
          <strong>未選取任何場景物件</strong>
          <p class="hint-text">在左側選擇「🖱️ 選取」工具並點擊 3D 世界中的任何建築、人物、動物、植物、家具或載具，即可獨立調整長寬高 (X/Y/Z)、旋轉、浮空高度、解除附掛、直接指派路徑與事件或設定任務考驗！</p>
        </div>`;
      return;
    }

    const sx = entity.scale_x > 0.01 ? entity.scale_x : (entity.scale || 1.0);
    const sy = entity.scale_y > 0.01 ? entity.scale_y : (entity.scale || 1.0);
    const sz = entity.scale_z > 0.01 ? entity.scale_z : (entity.scale || 1.0);
    entity.scale_x = sx;
    entity.scale_y = sy;
    entity.scale_z = sz;

    if (this._lastInspectorEntityId !== entity.id) {
      this._lastInspectorEntityId = entity.id;
      this.scaleLockRatio = Math.abs(sx - sy) < 0.02 && Math.abs(sy - sz) < 0.02;
    }

    // 計算子孫節點集合，避免於介面中選中自己或子孫造成循環附掛
    const blockedParents = new Set([entity.id]);
    let grew = true;
    while (grew) {
      grew = false;
      for (const e of this.app.sim.entities) {
        if (e.parent_id && blockedParents.has(e.parent_id) && !blockedParents.has(e.id)) {
          blockedParents.add(e.id);
          grew = true;
        }
      }
    }
    const otherEntities = this.app.sim.entities.filter((e) => !blockedParents.has(e.id));
    const hasParent = Boolean(entity.parent_id && this.app.sim.entities.some((e) => e.id === entity.parent_id));
    const parentOpts = `<option value="">（無附掛・獨立位置）</option>` +
      otherEntities.map((o) =>
        `<option value="${o.id}" ${entity.parent_id === o.id ? "selected" : ""}>${escapeHtml(o.name)}</option>`,
      ).join("");

    // 所屬移動路徑與漫遊區
    const boundPaths = this.app.sim.paths.filter((p) => p.entity_id === entity.id);
    const unboundPaths = this.app.sim.paths.filter((p) => !p.entity_id);
    const boundZones = this.app.sim.wanderZones.filter((z) => (z.entity_ids || []).includes(entity.id));

    // 與此物件相關的全部事件規則
    const relatedEvents = this.app.sim.events.filter((r) => {
      const c = r.trigger_config || {};
      if (c.entity_id === entity.id || c.entity_a === entity.id || c.entity_b === entity.id) return true;
      return (r.actions || []).some((a) => a.entity_id === entity.id);
    });

    container.innerHTML = `
      <div class="prop-card">
        <div class="prop-row">
          <label>物件名稱</label>
          <input id="insName" class="ctrl-input" value="${escapeHtml(entity.name)}" />
        </div>

        <div class="scale-xyz-box">
          <div class="row-gap space-between" style="margin-bottom:4px;">
            <strong style="font-size:12px;">📐 尺寸縮放 (X・Y・Z)</strong>
            <div class="row-gap">
              <label class="inline-field" title="開啟後調整任一軸會維持目前長寬高比例同步縮放">
                <input id="insScaleLock" type="checkbox" ${this.scaleLockRatio ? "checked" : ""} />
                <span>🔗 固定比例</span>
              </label>
              <button id="btnScaleReset" class="btn-mini" title="將長寬高重設為 1:1:1">1:1</button>
            </div>
          </div>

          <div class="scale-axis-row">
            <span class="axis-tag axis-x">長 X</span>
            <input id="insScaleX" type="range" min="0.2" max="5.0" step="0.05" value="${sx.toFixed(2)}" />
            <input id="insScaleXNum" type="number" min="0.15" max="8.0" step="0.05" class="ctrl-input axis-num" value="${sx.toFixed(2)}" />
          </div>
          <div class="scale-axis-row">
            <span class="axis-tag axis-y">寬 Y</span>
            <input id="insScaleY" type="range" min="0.2" max="5.0" step="0.05" value="${sy.toFixed(2)}" />
            <input id="insScaleYNum" type="number" min="0.15" max="8.0" step="0.05" class="ctrl-input axis-num" value="${sy.toFixed(2)}" />
          </div>
          <div class="scale-axis-row">
            <span class="axis-tag axis-z">高 Z</span>
            <input id="insScaleZ" type="range" min="0.2" max="5.0" step="0.05" value="${sz.toFixed(2)}" />
            <input id="insScaleZNum" type="number" min="0.15" max="8.0" step="0.05" class="ctrl-input axis-num" value="${sz.toFixed(2)}" />
          </div>
        </div>

        <div class="prop-row">
          <label>旋轉朝向</label>
          <input id="insYaw" type="range" min="0" max="359" step="1" value="${Math.round(entity.yaw)}" />
          <span id="insYawVal" class="mono-badge">${Math.round(entity.yaw)}°</span>
        </div>
        <div class="prop-row">
          <label>浮空高度</label>
          <input id="insZ" type="range" min="0" max="12" step="0.2" value="${entity.z}" />
          <span id="insZVal" class="mono-badge">${entity.z.toFixed(1)}</span>
        </div>
        <div class="prop-row">
          <label>位置座標</label>
          <div class="row-gap" style="flex:1;">
            <input id="insX" type="number" step="0.5" class="ctrl-input" style="width:72px;" value="${entity.x.toFixed(1)}" />
            <input id="insY" type="number" step="0.5" class="ctrl-input" style="width:72px;" value="${entity.y.toFixed(1)}" />
          </div>
        </div>
        <div class="prop-row">
          <label>色調染色</label>
          <input id="insTint" type="color" value="${entity.tint_hex || "#ffffff"}" />
          <label class="inline-field"><input id="insEmissive" type="checkbox" ${entity.emissive_on ? "checked" : ""} /> 夜間發光</label>
        </div>
        <div class="prop-row">
          <label>動態效果</label>
          <select id="insAnim" class="ctrl-select" style="flex:1;">
            <option value="auto" ${entity.anim_mode === "auto" ? "selected" : ""}>依物件自動預設</option>
            <option value="static" ${entity.anim_mode === "static" ? "selected" : ""}>靜止不動</option>
            <option value="sway" ${entity.anim_mode === "sway" ? "selected" : ""}>隨風搖曳 (植物/旗幟)</option>
            <option value="walk" ${entity.anim_mode === "walk" ? "selected" : ""}>彈性踏步 (人物/動物)</option>
            <option value="float" ${entity.anim_mode === "float" ? "selected" : ""}>優雅漂浮 (熱氣球/水晶)</option>
            <option value="bob" ${entity.anim_mode === "bob" ? "selected" : ""}>水波／引擎微震</option>
            <option value="spin" ${entity.anim_mode === "spin" ? "selected" : ""}>持續旋轉</option>
          </select>
        </div>
        <div class="prop-row">
          <label>附掛搭乘</label>
          <div class="row-gap" style="flex:1;flex-wrap:nowrap;">
            <select id="insParent" class="ctrl-select" style="flex:1;min-width:0;">${parentOpts}</select>
            ${hasParent ? `<button id="btnDetachParent" class="btn-mini btn-warn" title="解除目前附掛搭乘並平穩放回地面">🔓 解除附掛</button>` : ""}
          </div>
        </div>
        <div class="prop-row">
          <label>群組標籤</label>
          <input id="insTag" class="ctrl-input" placeholder="如：night_light" value="${escapeHtml(entity.tag)}" />
        </div>
        <div class="row-gap" style="margin-top:4px;">
          <button id="btnFollowEntity" class="btn btn-citywalk" style="flex:1;">🎥 附身跟拍</button>
          <button id="btnCloneEntity" class="btn btn-sub">📑 複製</button>
          <button id="btnDeleteEntity" class="btn btn-danger">🗑️ 刪除</button>
        </div>
      </div>

      <div class="prop-card">
        <div class="row-gap space-between">
          <strong>〰️ 此物件的路徑與漫遊設定</strong>
        </div>
        ${boundPaths.length === 0 ? `<div class="hint-text">尚未指派巡航移動路徑。</div>` : boundPaths.map((p) => `
          <div class="ev-action-item" data-epath="${p.id}">
            <div class="row-gap space-between">
              <strong style="font-size:12px;color:${p.color};">${escapeHtml(p.name)} (${p.waypoints.length}點)</strong>
              <div class="row-gap">
                <label class="inline-field"><input type="checkbox" class="ep-act" ${p.active ? "checked" : ""} /> 啟用</label>
                <button class="btn-mini ep-del" title="刪除路徑">✕</button>
              </div>
            </div>
            <div class="prop-row">
              <select class="ctrl-select ctrl-compact ep-loop">
                <option value="loop" ${p.loop_mode === "loop" ? "selected" : ""}>循環 Loop</option>
                <option value="pingpong" ${p.loop_mode === "pingpong" ? "selected" : ""}>往返 PingPong</option>
                <option value="once" ${p.loop_mode === "once" ? "selected" : ""}>單程 Once</option>
              </select>
              <label class="inline-field">速 <input type="number" class="ctrl-input ep-spd" step="0.3" min="0.2" max="15" style="width:62px;" value="${p.speed}" /></label>
            </div>
          </div>`).join("")}
        ${unboundPaths.length > 0 ? `
          <div class="row-gap">
            <select id="insBindPathSel" class="ctrl-select" style="flex:1;">
              ${unboundPaths.map((up) => `<option value="${up.id}">${escapeHtml(up.name)} (${up.waypoints.length}點)</option>`).join("")}
            </select>
            <button id="btnInsBindPath" class="btn-mini">🔗 綁定</button>
          </div>` : ""}
        <div class="row-gap">
          <button id="btnInsDrawPath" class="btn btn-sub" style="flex:1;">✏️ 為此物件畫新路徑</button>
          <button id="btnInsAddZone" class="btn btn-sub" style="flex:1;">⭕ 原地建立漫遊圈</button>
        </div>
        ${boundZones.map((z) => `
          <div class="lib-item" data-ezone="${z.id}">
            <span style="color:${z.color};">⭕ ${escapeHtml(z.name)} (半徑 ${z.radius})</span>
            <button class="btn-mini ez-leave" title="將此物件移出漫遊圈">退出漫遊</button>
          </div>`).join("")}
      </div>

      <div class="prop-card">
        <div class="row-gap space-between">
          <strong>🎬 此物件的事件設定 (${relatedEvents.length})</strong>
        </div>
        ${relatedEvents.length === 0 ? `<div class="hint-text">目前尚無綁定事件，點下方按鈕即可一鍵建立！</div>` : relatedEvents.map((r) => `
          <div class="ev-action-item" data-eev="${r.id}">
            <div class="row-gap space-between">
              <strong style="font-size:12px;">${escapeHtml(r.name)}</strong>
              <div class="row-gap">
                <button class="btn-mini eev-test" title="立即試播">▶</button>
                <button class="btn-mini eev-edit" title="進入事件導演編輯">✏️</button>
                <input type="checkbox" class="eev-en" ${r.enabled ? "checked" : ""} />
                <button class="btn-mini eev-del" title="刪除事件">✕</button>
              </div>
            </div>
            <div class="hint-text">${describeRulePlain(r, this.app)}</div>
          </div>`).join("")}
        <div class="row-gap">
          <button class="btn-mini ins-new-ev" data-evkind="click" style="flex:1;">＋ 點擊事件</button>
          <button class="btn-mini ins-new-ev" data-evkind="encounter" style="flex:1;">＋ 玩家相遇</button>
          <button class="btn-mini ins-new-ev" data-evkind="proximity" style="flex:1;">＋ 靠近座標</button>
        </div>
      </div>

      <div id="insQuestWrap"></div>`;

    const patch = (fields) => this.app.updateEntityFields(entity.id, fields);

    const sliderX = container.querySelector("#insScaleX");
    const sliderY = container.querySelector("#insScaleY");
    const sliderZ = container.querySelector("#insScaleZ");
    const numX = container.querySelector("#insScaleXNum");
    const numY = container.querySelector("#insScaleYNum");
    const numZ = container.querySelector("#insScaleZNum");

    const syncScaleInputs = () => {
      sliderX.value = Math.min(5, Math.max(0.2, entity.scale_x)).toFixed(2);
      sliderY.value = Math.min(5, Math.max(0.2, entity.scale_y)).toFixed(2);
      sliderZ.value = Math.min(5, Math.max(0.2, entity.scale_z)).toFixed(2);
      numX.value = entity.scale_x.toFixed(2);
      numY.value = entity.scale_y.toFixed(2);
      numZ.value = entity.scale_z.toFixed(2);
    };

    const applyAxisScale = (axis, rawVal, persist = false) => {
      const clamped = Math.max(0.15, Math.min(8.0, Number(rawVal) || 1.0));
      const key = `scale_${axis}`;
      const oldVal = Math.max(0.05, entity[key] || 1.0);
      if (this.scaleLockRatio) {
        const ratio = clamped / oldVal;
        entity.scale_x = Number(Math.max(0.15, Math.min(8.0, (axis === "x" ? clamped : entity.scale_x * ratio))).toFixed(2));
        entity.scale_y = Number(Math.max(0.15, Math.min(8.0, (axis === "y" ? clamped : entity.scale_y * ratio))).toFixed(2));
        entity.scale_z = Number(Math.max(0.15, Math.min(8.0, (axis === "z" ? clamped : entity.scale_z * ratio))).toFixed(2));
      } else {
        entity[key] = Number(clamped.toFixed(2));
      }
      entity.scale = Number(((entity.scale_x + entity.scale_y + entity.scale_z) / 3.0).toFixed(2));
      syncScaleInputs();
      if (persist) {
        patch({
          scale_x: entity.scale_x,
          scale_y: entity.scale_y,
          scale_z: entity.scale_z,
          scale: entity.scale,
        });
      }
    };

    container.querySelector("#insScaleLock").addEventListener("change", (e) => {
      this.scaleLockRatio = e.target.checked;
    });
    container.querySelector("#btnScaleReset").addEventListener("click", () => {
      entity.scale = 1.0;
      entity.scale_x = 1.0;
      entity.scale_y = 1.0;
      entity.scale_z = 1.0;
      syncScaleInputs();
      patch({ scale: 1.0, scale_x: 1.0, scale_y: 1.0, scale_z: 1.0 });
    });

    [
      ["x", sliderX, numX],
      ["y", sliderY, numY],
      ["z", sliderZ, numZ],
    ].forEach(([axis, rng, num]) => {
      rng.addEventListener("input", (e) => applyAxisScale(axis, e.target.value, false));
      rng.addEventListener("change", (e) => applyAxisScale(axis, e.target.value, true));
      num.addEventListener("change", (e) => applyAxisScale(axis, e.target.value, true));
    });

    container.querySelector("#insName").addEventListener("change", (e) => patch({ name: e.target.value }));
    container.querySelector("#insYaw").addEventListener("input", (e) => {
      const v = Number(e.target.value);
      entity.yaw = v;
      container.querySelector("#insYawVal").textContent = `${Math.round(v)}°`;
    });
    container.querySelector("#insYaw").addEventListener("change", (e) => patch({ yaw: Number(e.target.value) }));

    container.querySelector("#insZ").addEventListener("input", (e) => {
      const v = Number(e.target.value);
      entity.z = v;
      container.querySelector("#insZVal").textContent = v.toFixed(1);
    });
    container.querySelector("#insZ").addEventListener("change", (e) => patch({ z: Number(e.target.value) }));

    container.querySelector("#insX").addEventListener("change", (e) => patch({ x: Number(e.target.value) }));
    container.querySelector("#insY").addEventListener("change", (e) => patch({ y: Number(e.target.value) }));
    container.querySelector("#insTint").addEventListener("change", (e) => patch({ tint_hex: e.target.value }));
    container.querySelector("#insEmissive").addEventListener("change", (e) => patch({ emissive_on: e.target.checked }));
    container.querySelector("#insAnim").addEventListener("change", (e) => patch({ anim_mode: e.target.value }));

    // 修正解除附掛搭乘：清除 parent_id 並將物件平穩放置至附近可行走地面
    const detachParent = async () => {
      const [safeX, safeY] = this.app.sim._findSafeLandPoint(entity.x + 0.9, entity.y, false);
      entity.parent_id = null;
      entity.x = Number(safeX.toFixed(2));
      entity.y = Number(safeY.toFixed(2));
      entity.z = 0;
      const ok = await patch({
        parent_id: null,
        x: entity.x,
        y: entity.y,
        z: 0,
      });
      if (ok) {
        this.renderInspector(container, entity);
        this.app.showToast(`已解除「${entity.name}」的附掛搭乘並放回地面！`);
      }
    };

    container.querySelector("#insParent").addEventListener("change", async (e) => {
      const nextParent = e.target.value || null;
      if (!nextParent) {
        await detachParent();
      } else {
        const ok = await patch({ parent_id: nextParent });
        if (ok) {
          this.renderInspector(container, entity);
          const pName = this.app.sim.entities.find((x) => x.id === nextParent)?.name || "目標物件";
          this.app.showToast(`已將「${entity.name}」附掛搭乘至「${pName}」！`);
        } else {
          this.renderInspector(container, entity);
        }
      }
    });
    container.querySelector("#btnDetachParent")?.addEventListener("click", detachParent);

    container.querySelector("#insTag").addEventListener("change", (e) => patch({ tag: e.target.value }));

    container.querySelector("#btnFollowEntity").addEventListener("click", () => {
      this.app.toggleCityWalk(entity.id);
    });
    container.querySelector("#btnCloneEntity").addEventListener("click", () => {
      this.app.spawnEntityAt(
        entity.asset_id,
        entity.x + 1.5,
        entity.y + 1.5,
        entity.z,
        entity.yaw,
        entity.scale,
        entity.scale_x,
        entity.scale_y,
        entity.scale_z,
      );
    });
    container.querySelector("#btnDeleteEntity").addEventListener("click", () => {
      this.app.deleteEntityById(entity.id);
    });

    // 此物件的路徑／漫遊設定事件綁定
    container.querySelectorAll("[data-epath]").forEach((card) => {
      const p = this.app.sim.paths.find((x) => x.id === card.dataset.epath);
      if (!p) return;
      card.querySelector(".ep-act").addEventListener("change", (e) => { p.active = e.target.checked; this.app.savePath(p); });
      card.querySelector(".ep-loop").addEventListener("change", (e) => { p.loop_mode = e.target.value; this.app.savePath(p); });
      card.querySelector(".ep-spd").addEventListener("change", (e) => { p.speed = Number(e.target.value) || 2.0; this.app.savePath(p); });
      card.querySelector(".ep-del").addEventListener("click", async () => {
        await this.app.deletePath(p.id);
        this.app.renderRightPanel();
      });
    });
    container.querySelector("#btnInsBindPath")?.addEventListener("click", async () => {
      const pid = container.querySelector("#insBindPathSel")?.value;
      const p = this.app.sim.paths.find((x) => x.id === pid);
      if (!p) return;
      p.entity_id = entity.id;
      await this.app.savePath(p);
      this.renderInspector(container, entity);
      this.app.showToast(`已將路徑「${p.name}」綁定至「${entity.name}」！`);
    });
    container.querySelector("#btnInsDrawPath")?.addEventListener("click", () => {
      this.app.renderer.selectedEntityId = entity.id;
      this.setTool("path");
      this.app.showToast(`請在地圖依序點擊路徑節點，完成後即自動綁定「${entity.name}」！`);
    });
    container.querySelector("#btnInsAddZone")?.addEventListener("click", async () => {
      await this.app.createWanderZone(entity.x, entity.y, 4.0);
    });
    container.querySelectorAll("[data-ezone]").forEach((row) => {
      const z = this.app.sim.wanderZones.find((x) => x.id === row.dataset.ezone);
      if (!z) return;
      row.querySelector(".ez-leave").addEventListener("click", async () => {
        z.entity_ids = (z.entity_ids || []).filter((id) => id !== entity.id);
        await this.app.saveZone(z);
        this.renderInspector(container, entity);
      });
    });

    // 此物件的事件設定事件綁定
    container.querySelectorAll("[data-eev]").forEach((card) => {
      const r = this.app.sim.events.find((x) => x.id === card.dataset.eev);
      if (!r) return;
      card.querySelector(".eev-test").addEventListener("click", () => {
        this.app.sim.executeRule(r);
        this.app.showToast(`已試播事件：「${r.name}」`);
      });
      card.querySelector(".eev-edit").addEventListener("click", () => {
        this.app.eventDraft = {
          editingId: r.id,
          name: r.name,
          trigger_type: r.trigger_type,
          trigger_config: JSON.parse(JSON.stringify(r.trigger_config || {})),
          actions: JSON.parse(JSON.stringify(r.actions || [])),
          cooldown_sec: r.cooldown_sec || 3.0,
        };
        this.app.activeRightTab = "events";
        this.app._syncRightTabs();
        this.app.renderRightPanel();
      });
      card.querySelector(".eev-en").addEventListener("change", (e) => {
        r.enabled = e.target.checked;
        this.app.updateEventRule(r);
      });
      card.querySelector(".eev-del").addEventListener("click", async () => {
        await this.app.deleteEventRule(r.id);
        this.renderInspector(container, entity);
      });
    });

    container.querySelectorAll(".ins-new-ev").forEach((btn) => {
      btn.addEventListener("click", () => {
        const kind = btn.dataset.evkind;
        const cx = Number(entity.x.toFixed(1));
        const cy = Number(entity.y.toFixed(1));
        this.app.eventDraft = {
          editingId: null,
          name: kind === "click" ? `點擊「${entity.name}」驚喜` : (kind === "encounter" ? `玩家遇見「${entity.name}」` : `「${entity.name}」抵達目標區`),
          trigger_type: kind,
          trigger_config: {
            entity_id: entity.id,
            entity_a: "__player__",
            entity_b: entity.id,
            radius: 2.2,
            x: cx,
            y: cy,
            hour: 20,
            minute: 0,
            seconds: 12,
            mode: "night",
          },
          actions: [
            { type: "speech_bubble", entity_id: entity.id, text: `你好呀～我是${entity.name}！✨`, duration: 4.0 },
            { type: "play_sound", sound: "encounter" },
          ],
          cooldown_sec: 3.0,
        };
        this.app.activeRightTab = "events";
        this.app._syncRightTabs();
        this.app.renderRightPanel();
        this.app.showToast(`已帶入「${entity.name}」，可調整動作並儲存規則！`);
      });
    });

    // 內嵌「任務角色／互動考驗」編輯器
    this.app.quest?.renderEntityInteractionEditor(container.querySelector("#insQuestWrap"), entity);
  }
}

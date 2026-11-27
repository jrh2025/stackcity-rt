//! 《StackCity RT（像素疊界）》主應用程式協調器。

import { BGM_STYLES, soundEngine } from "./audio.js";
import { renderEventsPanel, renderMotionPanel } from "./events_ui.js";
import { initI18n, onLangChange, tr } from "./i18n.js";
import { QuestManager } from "./quest.js";
import { StackCityRenderer } from "./renderer.js";
import { WorldSimulation } from "./simulation.js";
import { CreationStudio, escapeHtml } from "./studio.js";
import { ToolController } from "./tools.js";

const ECO_RULES = {
  forest: {
    categories: ["plant"],
    keywords: ["樹", "松", "櫻", "橡", "菇", "tree", "pine", "sakura", "oak"],
    fallbackPrompts: ["蒼翠松樹", "大橡樹", "粉紅櫻花樹"],
  },
  meadow: {
    categories: ["plant"],
    keywords: ["花", "葵", "麥", "稻", "菇", "sunflower", "wheat", "mushroom"],
    fallbackPrompts: ["向日葵花圃", "金色稻麥叢", "奇幻發光巨菇"],
  },
  pasture: {
    categories: ["animal"],
    keywords: ["羊", "柴犬", "狗", "馬", "獅", "鴨", "鳥"],
    fallbackPrompts: ["牧場綿羊", "可愛柴犬", "草原駿馬"],
  },
  town: {
    categories: ["building", "prop"],
    keywords: ["屋", "店", "燈", "椅", "木屋", "路燈", "長椅"],
    fallbackPrompts: ["溫馨木屋", "復古街燈", "公園長椅"],
  },
  indoor_decor: {
    categories: ["plant", "prop"],
    keywords: ["盆栽", "椅", "櫃", "桌", "箱", "沙發", "書櫃"],
    fallbackPrompts: ["室內綠意盆栽", "木頭書櫃", "客廳布藝沙發"],
  },
};

class StackCityApp {
  constructor() {
    this.worlds = [];
    this.currentWorldFile = null;
    this.worldState = null;
    this.assetMap = new Map();
    this.activeCategory = "all";
    this.activeRightTab = "inspector";
    this.selectedAssetId = null;
    this.pendingRemoveAssetId = null;
    this.eventDraft = null;
    this._lastPlayerSaveMs = 0;
    this._playerBusy = false;
    this.cameraFollowPlayer = false;
    this.portalReturnStack = [];
    this.sharedAdventure = null;
    this._portalBusy = false;

    this.leftCollapsed = false;
    this.rightCollapsed = false;

    this.keysDown = new Set();
    this.mouseState = {
      downBtn: -1,
      downX: 0,
      downY: 0,
      lastX: 0,
      lastY: 0,
      draggingEntityId: null,
      moved: false,
      isPanning: false,
    };
    this.lastBrushTime = 0;
    this.ecoSeedBusy = false;

    this.glCanvas = document.getElementById("glCanvas");
    this.overlayCanvas = document.getElementById("overlayCanvas");
    this.renderer = new StackCityRenderer(this.glCanvas, this.overlayCanvas);
    this.sim = new WorldSimulation(this.renderer, (kind, val) => this._handleSimUiSync(kind, val));
    this.quest = new QuestManager(this);
    this.sim.onEntityQuestInteract = (ent) => this.quest.openEncounterModal(ent);
    this.sim.onProximityCheck = (px, py, nowMs) => this.quest.checkProximityAmbush(px, py, nowMs);
    this.sim.getQuestBadge = (ent) => this.quest.getEntityBadge(ent);
    this.sim.checkPortalLock = (portal) => this._checkPortalLock(portal);
    this.sim.onPortalTrigger = (ent, portal) => this.handlePortalTransition(ent, portal);
    this.tools = new ToolController(this);
    this.studio = new CreationStudio({
      getWorldFile: () => this.currentWorldFile,
      getWorldAssets: () => Array.from(this.assetMap.values()),
      onAssetReady: (asset, startPlacing) => this._onStudioAssetReady(asset, startPlacing),
      onAssetUpdated: (scope, asset) => this._onStudioAssetUpdated(scope, asset),
      onAssetDeleted: (scope, assetId, removedEntityIds) => this._onStudioAssetDeleted(scope, assetId, removedEntityIds),
      showToast: (m, err) => this.showToast(m, err),
    });

    this._bindGlobalUi();
    this._bindCanvasControls();
    initI18n("langSelect");
    onLangChange(() => {
      if (this.worldState) {
        this._updateWorldBadge(this.sim.entities.length);
        this.renderAssetGrid();
        this.renderRightPanel();
        this.tools.renderSubOptions();
        this.quest.renderQuestHud();
      }
    });
    this._init();
  }

  async _init() {
    // 監聽視窗／面板尺寸變化自動重設畫布大小
    if (window.ResizeObserver) {
      const ro = new ResizeObserver(() => this.renderer.resize());
      ro.observe(document.querySelector(".viewport-wrap"));
    }
    try {
      await this.refreshWorldList();
      if (this.worlds.length > 0) {
        await this.loadWorld(this.worlds[0].file_name);
      }
    } catch (err) {
      console.error("初始化世界失敗：", err);
      this.showToast(`初始化發生錯誤：${err?.message || err}`, true);
    } finally {
      this._startMainLoop();
    }
  }

  showToast(msg, isError = false) {
    const stack = document.getElementById("toastStack");
    const div = document.createElement("div");
    div.className = `toast ${isError ? "err" : ""}`;
    div.textContent = tr(msg);
    stack.appendChild(div);
    setTimeout(() => div.remove(), 3500);
  }

  async refreshWorldList() {
    const res = await fetch("/api/worlds");
    const data = await res.json();
    this.worlds = data.worlds || [];
    const sel = document.getElementById("worldSelect");
    sel.innerHTML = "";
    for (const w of this.worlds) {
      const opt = document.createElement("option");
      opt.value = w.file_name;
      opt.textContent = `${w.name} (${w.width}×${w.height})`;
      sel.appendChild(opt);
    }
    if (this.currentWorldFile) sel.value = this.currentWorldFile;
  }

  async loadWorld(fileName, opts = null) {
    const res = await fetch(`/api/worlds/${encodeURIComponent(fileName)}/state`);
    const data = await res.json();
    if (!res.ok) {
      this.showToast(data.error || "載入世界失敗", true);
      return null;
    }
    const wasWalkMode = Boolean(opts?.preserveWalkMode && this.renderer.camera.walkMode);
    const prevWalkYaw = this.renderer.camera.walkYaw;
    const prevCamYaw = this.renderer.camera.yaw;

    this.currentWorldFile = fileName;
    this.worldState = data;
    document.getElementById("worldSelect").value = fileName;
    this._updateWorldBadge(data.entities.length);

    this.assetMap.clear();
    data.assets.forEach((a) => this.assetMap.set(a.id, a));

    this.renderer.rebuildTerrainMesh(data.meta.width, data.meta.height, data.terrain);
    this.renderer.resetCamera();
    this.cameraFollowPlayer = false;
    this.renderer.selectedEntityId = null;

    await Promise.all(
      data.assets.map((a) => this.renderer.ensureAssetTextures(fileName, a)),
    );

    this.sim.loadState(data, this.assetMap);
    this.quest.loadWorld(data);

    // 跨世界攜帶玩家角色外觀與暱稱
    if (opts?.carryPlayerName) {
      this.sim.player.name = opts.carryPlayerName;
    }
    if (opts?.carryAvatarSha || opts?.carryAvatarName) {
      const matched =
        data.assets.find((a) => opts.carryAvatarSha && a.sha256 === opts.carryAvatarSha) ||
        data.assets.find((a) => opts.carryAvatarName && a.name === opts.carryAvatarName);
      if (matched) {
        this.sim.player.asset_id = matched.id;
        this.sim.player.visible = true;
        this.sim.applyPlayerScale(opts.carryPlayerScale ?? data.meta.player_scale ?? 1.0);
      }
    }

    // 跨世界門落點定位（避免卡在門本體／牆體內）
    if (opts && (typeof opts.spawnX === "number" || opts.targetPortalId)) {
      let prefX = typeof opts.spawnX === "number" ? opts.spawnX : this.sim.player.x;
      let prefY = typeof opts.spawnY === "number" ? opts.spawnY : this.sim.player.y;
      if (opts.targetPortalId && typeof opts.spawnX !== "number") {
        const pairEnt = this.sim.entities.find((e) => e.id === opts.targetPortalId);
        const pPos = pairEnt ? this.sim.getPortalWorldPos(pairEnt) : null;
        if (pPos) {
          prefX = pPos.x;
          prefY = pPos.y;
        }
      }
      const [sx, sy] = this.sim._findSafeLandPoint(prefX, prefY, false);
      this.sim.player.x = sx;
      this.sim.player.y = sy;
      this.sim.player.z = 0;
      if (typeof opts.spawnYaw === "number") {
        this.sim.player.yaw = opts.spawnYaw;
      }
      const gz = this.renderer.getTerrainElevationAt(sx, sy);
      this.renderer.camera.walkPos = [sx, sy, gz];
      this.renderer.camera.target = [sx, sy, gz + 0.8];
      this.cameraFollowPlayer = true;
    }

    if (wasWalkMode) {
      this.renderer.camera.walkMode = true;
      this.renderer.camera.walkYaw = prevWalkYaw;
    } else if (opts?.isPortalJump) {
      this.renderer.camera.yaw = prevCamYaw;
    }

    let portalVictoryPending = false;
    if (opts?.keepSharedAdventure && this.sharedAdventure) {
      const resSync = this.quest.applySharedAdventure(this.sharedAdventure);
      portalVictoryPending = Boolean(resSync?.newlyCompleted);
    }

    this.eventDraft = null;
    this.cancelRemoveWorldAsset();
    if (!this.selectedAssetId || !this.assetMap.has(this.selectedAssetId)) {
      this.selectedAssetId = data.assets[0]?.id || null;
    }

    this._syncBgmFromMeta(data.meta);
    this._syncTimeAndWeatherUi();
    this.renderAssetGrid();
    this.renderRightPanel();
    this.tools.renderSubOptions();
    return { portalVictoryPending };
  }

  _updateBgmStyleInfo(preset) {
    const infoEl = document.getElementById("bgmStyleInfo");
    if (!infoEl) return;
    const st = BGM_STYLES[preset] || BGM_STYLES.kingdom;
    infoEl.innerHTML = preset === "off"
      ? `<strong>${escapeHtml(st.title)}</strong><br><span style="color:#94a3b8;">${escapeHtml(st.instruments)}</span>`
      : `<strong>${escapeHtml(st.title)}</strong><br>節拍：<code>${escapeHtml(st.meter)}</code>｜速度：<code>${st.bpm} BPM</code><br>配器：${escapeHtml(st.instruments)}`;
  }

  _syncBgmFromMeta(meta) {
    const preset = meta.bgm_preset || "kingdom";
    const customUrl = meta.has_custom_bgm
      ? `/api/worlds/${encodeURIComponent(this.currentWorldFile)}/bgm?t=${encodeURIComponent(meta.updated_at || "")}`
      : null;
    soundEngine.configureBgm({
      preset,
      volume: meta.bgm_volume ?? 0.45,
      hasCustom: meta.has_custom_bgm,
      customUrl,
    });
    const presetSel = document.getElementById("bgmPresetSelect");
    const volSlider = document.getElementById("bgmVolumeSlider");
    const volVal = document.getElementById("bgmVolumeVal");
    const statusEl = document.getElementById("customBgmStatus");
    const delBtn = document.getElementById("btnDeleteBgm");
    if (presetSel) presetSel.value = preset;
    if (volSlider) volSlider.value = String(meta.bgm_volume ?? 0.45);
    if (volVal) volVal.textContent = `${Math.round((meta.bgm_volume ?? 0.45) * 100)}%`;
    this._updateBgmStyleInfo(preset);
    if (statusEl) {
      statusEl.textContent = meta.has_custom_bgm
        ? `目前使用自訂音樂：${meta.custom_bgm_name || "自訂音檔"}`
        : "目前使用：內建程序化多軌配樂";
    }
    if (delBtn) delBtn.classList.toggle("hidden", !meta.has_custom_bgm);
  }

  _updatePanelCollapseState() {
    const appEl = document.getElementById("app");
    appEl.classList.toggle("hide-left", this.leftCollapsed);
    appEl.classList.toggle("hide-right", this.rightCollapsed);
    document.getElementById("btnToggleLeft").classList.toggle("active", !this.leftCollapsed);
    document.getElementById("btnToggleRight").classList.toggle("active", !this.rightCollapsed);
    const showFloats = !this.sim.visitorMode;
    document.getElementById("floatExpandLeft").classList.toggle("hidden", !this.leftCollapsed || !showFloats);
    document.getElementById("floatExpandRight").classList.toggle("hidden", !this.rightCollapsed || !showFloats);
    requestAnimationFrame(() => this.renderer.resize());
  }

  _bindGlobalUi() {
    // 任意點擊首次解鎖 Web Audio（含背景音樂與音效）
    const unlockOnce = () => {
      soundEngine.unlockAudio();
      window.removeEventListener("pointerdown", unlockOnce);
      window.removeEventListener("keydown", unlockOnce);
    };
    window.addEventListener("pointerdown", unlockOnce);
    window.addEventListener("keydown", unlockOnce);

    document.getElementById("worldSelect").addEventListener("change", (e) => {
      this.portalReturnStack = [];
      this.sharedAdventure = null;
      this.loadWorld(e.target.value);
    });

    // 面板收合與展開
    document.getElementById("btnToggleLeft").addEventListener("click", () => {
      this.leftCollapsed = !this.leftCollapsed;
      this._updatePanelCollapseState();
    });
    document.getElementById("btnCollapseLeftInner").addEventListener("click", () => {
      this.leftCollapsed = true;
      this._updatePanelCollapseState();
    });
    document.getElementById("floatExpandLeft").addEventListener("click", () => {
      this.leftCollapsed = false;
      this._updatePanelCollapseState();
    });
    document.getElementById("btnToggleRight").addEventListener("click", () => {
      this.rightCollapsed = !this.rightCollapsed;
      this._updatePanelCollapseState();
    });
    document.getElementById("floatExpandRight").addEventListener("click", () => {
      this.rightCollapsed = false;
      this._updatePanelCollapseState();
    });

    // 全螢幕切換
    const fsBtn = document.getElementById("btnFullscreen");
    fsBtn.addEventListener("click", async () => {
      try {
        if (!document.fullscreenElement) {
          await document.documentElement.requestFullscreen();
        } else {
          await document.exitFullscreen();
        }
      } catch (err) {
        this.showToast(`無法切換全螢幕：${err.message}`, true);
      }
    });
    document.addEventListener("fullscreenchange", () => {
      const isFs = Boolean(document.fullscreenElement);
      fsBtn.textContent = isFs ? "🗗" : "⛶";
      fsBtn.title = isFs ? "離開全螢幕" : "進入全螢幕";
      setTimeout(() => this.renderer.resize(), 80);
    });

    // 編輯模式 / 好友參觀模式切換
    document.getElementById("btnModeToggle").addEventListener("click", (e) => {
      this.sim.visitorMode = !this.sim.visitorMode;
      document.getElementById("app").classList.toggle("visitor-mode", this.sim.visitorMode);
      e.target.textContent = this.sim.visitorMode ? "👁️ 訪客參觀中" : "✏️ 編輯模式";
      if (this.sim.visitorMode) this.tools.setTool("select");
      this._updatePanelCollapseState();
    });

    // 時鐘與天氣控制
    document.getElementById("btnPlayPause").addEventListener("click", (e) => {
      this.sim.paused = !this.sim.paused;
      e.target.textContent = this.sim.paused ? "⏸️" : "▶️";
    });
    document.getElementById("timeSpeedSelect").addEventListener("change", (e) => {
      this.sim.timeSpeed = Number(e.target.value);
      this._patchWorldMeta({ time_speed: this.sim.timeSpeed });
    });
    const timeSlider = document.getElementById("timeSlider");
    timeSlider.addEventListener("input", (e) => {
      this.sim.timeOfDay = Number(e.target.value);
      this._updateTimeBadge(this.sim.timeOfDay);
    });
    timeSlider.addEventListener("change", (e) => {
      this._patchWorldMeta({ time_of_day: Number(e.target.value) });
    });

    document.querySelectorAll("#weatherGroup .seg-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        this.sim.weather = btn.dataset.weather;
        this._syncTimeAndWeatherUi();
        this._patchWorldMeta({ weather: this.sim.weather });
      });
    });

    // 相機與漫步模式
    document.getElementById("btnCityWalk").addEventListener("click", () => this.toggleCityWalk(null));
    document.getElementById("btnExitWalk").addEventListener("click", () => this.toggleCityWalk(null));
    document.getElementById("btnMountSwitchMode")?.addEventListener("click", () => this.sim.toggleMountControlMode());
    document.getElementById("btnMountDismount")?.addEventListener("click", () => this.sim.dismountPlayer(false));
    document.getElementById("btnResetCam").addEventListener("click", () => {
      this.cameraFollowPlayer = false;
      this.renderer.resetCamera();
    });
    document.getElementById("btnRotateLeft").addEventListener("click", () => {
      this.renderer.camera.yaw -= Math.PI * 0.25;
    });
    document.getElementById("btnRotateRight").addEventListener("click", () => {
      this.renderer.camera.yaw += Math.PI * 0.25;
    });

    // 靜音開關
    document.getElementById("btnMute").addEventListener("click", (e) => {
      const m = soundEngine.toggleMute();
      e.target.textContent = m ? "🔇" : "🔊";
    });

    // 背景音樂設定視窗
    document.getElementById("btnBgm").addEventListener("click", () => {
      soundEngine.unlockAudio();
      this._updateBgmStyleInfo(document.getElementById("bgmPresetSelect").value);
      document.getElementById("bgmModal").classList.remove("hidden");
    });
    document.getElementById("bgmPresetSelect").addEventListener("change", async (e) => {
      const preset = e.target.value;
      soundEngine.unlockAudio();
      this._updateBgmStyleInfo(preset);
      soundEngine.configureBgm({ preset });
      await this._patchWorldMeta({ bgm_preset: preset });
      this.showToast(preset === "off" ? "已關閉背景音樂" : `已切換配樂：${BGM_STYLES[preset]?.title || preset}`);
    });
    document.getElementById("btnPreviewBgm").addEventListener("click", () => {
      const preset = document.getElementById("bgmPresetSelect").value;
      soundEngine.unlockAudio();
      if (soundEngine.muted) {
        soundEngine.toggleMute();
        document.getElementById("btnMute").textContent = "🔊";
      }
      soundEngine.configureBgm({ preset, hasCustom: false });
      this.showToast(`正在試聽：${BGM_STYLES[preset]?.title || preset}`);
    });

    // 玩家角色（Avatar）選擇與設定視窗
    document.getElementById("btnPlayerAvatar").addEventListener("click", () => this.openPlayerModal());
    document.getElementById("btnWalkChangeAvatar").addEventListener("click", () => this.openPlayerModal());
    document.getElementById("plNameInput").addEventListener("change", async (e) => {
      const nm = e.target.value.trim() || "冒險玩家";
      e.target.value = nm;
      this.sim.player.name = nm;
      if (this.worldState?.meta) this.worldState.meta.player_name = nm;
      await this._patchWorldMeta({ player_name: nm });
      if (this.activeRightTab === "events") this.renderRightPanel();
      this.showToast(`玩家暱稱已更新為「${nm}」`);
    });
    const plScaleSlider = document.getElementById("plScaleSlider");
    plScaleSlider.addEventListener("input", (e) => {
      const v = Number(e.target.value);
      document.getElementById("plScaleVal").textContent = `${v.toFixed(2)}x`;
      this.sim.applyPlayerScale(v);
    });
    plScaleSlider.addEventListener("change", async (e) => {
      const v = Number(e.target.value);
      if (this.worldState?.meta) this.worldState.meta.player_scale = v;
      await this._patchWorldMeta({ player_scale: v });
    });
    document.getElementById("btnPlayerToCenter").addEventListener("click", async () => {
      const tx = Math.max(1, Math.min(this.renderer.worldWidth - 1, this.renderer.camera.target[0]));
      const ty = Math.max(1, Math.min(this.renderer.worldHeight - 1, this.renderer.camera.target[1]));
      const gz = this.renderer.getTerrainElevationAt(tx, ty);
      this.sim.player.x = tx;
      this.sim.player.y = ty;
      this.renderer.camera.walkPos = [tx, ty, gz];
      this.renderer.spawnParticles("sparkle", tx, ty, gz + 1.5, 22);
      soundEngine.play("chime");
      await this._patchWorldMeta({ player_x: tx, player_y: ty });
      this.showToast("已將玩家角色傳送到目前畫面中央！");
    });
    document.getElementById("btnFocusPlayer").addEventListener("click", () => {
      const p = this.sim.player;
      const gz = this.renderer.getTerrainElevationAt(p.x, p.y);
      this.renderer.camera.target = [p.x, p.y, gz + 0.8];
      document.getElementById("playerModal").classList.add("hidden");
    });
    document.getElementById("btnPlayAsPlayer").addEventListener("click", () => {
      document.getElementById("playerModal").classList.add("hidden");
      if (!this.renderer.camera.walkMode) {
        this.toggleCityWalk(null);
      } else {
        this.renderer.camera.followEntityId = null;
      }
      this.showToast(`正操控「${this.sim.player.name}」探索世界！按 WASD 移動、F 互動`);
    });
    document.querySelectorAll("#playerQuickPresets .pl-quick").forEach((btn) => {
      btn.addEventListener("click", () => {
        this.applyQuickPlayerPreset(btn.dataset.key, btn.dataset.label);
      });
    });
    const bgmVolSlider = document.getElementById("bgmVolumeSlider");
    bgmVolSlider.addEventListener("input", (e) => {
      const v = Number(e.target.value);
      soundEngine.setBgmVolume(v);
      document.getElementById("bgmVolumeVal").textContent = `${Math.round(v * 100)}%`;
    });
    bgmVolSlider.addEventListener("change", (e) => {
      this._patchWorldMeta({ bgm_volume: Number(e.target.value) });
    });
    document.getElementById("btnUploadBgm").addEventListener("click", async () => {
      const f = document.getElementById("bgmFileInput").files?.[0];
      if (!f || !this.currentWorldFile) {
        return this.showToast("請先選擇音樂檔案（.mp3 / .wav / .ogg / .m4a）！", true);
      }
      const fd = new FormData();
      fd.append("file", f);
      const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/bgm`, {
        method: "POST",
        body: fd,
      });
      const data = await res.json();
      if (!res.ok) return this.showToast(data.error || "上傳音樂失敗", true);
      this.worldState.meta = data.meta;
      this._syncBgmFromMeta(data.meta);
      this.showToast(`已載入自訂背景音樂「${data.meta.custom_bgm_name}」並寫入世界檔！`);
    });
    document.getElementById("btnDeleteBgm").addEventListener("click", async () => {
      if (!this.currentWorldFile) return;
      const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/bgm`, {
        method: "DELETE",
      });
      const data = await res.json();
      if (res.ok) {
        this.worldState.meta = data.meta;
        this._syncBgmFromMeta(data.meta);
        this.showToast("已移除自訂音樂，恢復內建背景音樂。");
      }
    });

    // 擴展場景地圖 Modal
    document.getElementById("btnMapManage").addEventListener("click", () => {
      if (this.worldState?.meta) {
        const m = this.worldState.meta;
        document.getElementById("mapCurrentSize").textContent =
          `目前地圖尺寸：${m.width} × ${m.height} 格（上限 96 × 96 格）`;
        const isIndoor = (m.theme || "").endsWith("_interior");
        document.getElementById("resizeGroundSelect").value = isIndoor ? "wood_floor" : "grass";
      }
      document.getElementById("mapManageModal").classList.remove("hidden");
    });
    document.getElementById("btnQuickExpandAll").addEventListener("click", () => {
      document.getElementById("addNorth").value = "4";
      document.getElementById("addSouth").value = "4";
      document.getElementById("addWest").value = "4";
      document.getElementById("addEast").value = "4";
    });
    document.getElementById("btnApplyResize").addEventListener("click", async () => {
      if (!this.currentWorldFile) return;
      const payload = {
        add_north: Number(document.getElementById("addNorth").value) || 0,
        add_south: Number(document.getElementById("addSouth").value) || 0,
        add_west: Number(document.getElementById("addWest").value) || 0,
        add_east: Number(document.getElementById("addEast").value) || 0,
        fill_ground: document.getElementById("resizeGroundSelect").value,
        fill_elevation: Number(document.getElementById("resizeElevInput").value) || 1,
      };
      const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/resize`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      const data = await res.json();
      if (!res.ok) return this.showToast(data.error || "調整地圖尺寸失敗", true);
      document.getElementById("mapManageModal").classList.add("hidden");
      await this.refreshWorldList();
      await this.loadWorld(this.currentWorldFile);
      this.showToast(`場景已擴展為 ${data.meta.width} × ${data.meta.height} 格！`);
    });

    // 造物工坊按鈕
    document.getElementById("btnOpenStudio").addEventListener("click", () => {
      const selAsset = this.selectedAssetId ? this.assetMap.get(this.selectedAssetId) : null;
      this.studio.open(selAsset);
    });

    // 資產分類切換
    document.querySelectorAll("#assetCategoryTabs .cat-tab").forEach((tab) => {
      tab.addEventListener("click", () => {
        document.querySelectorAll("#assetCategoryTabs .cat-tab").forEach((b) => b.classList.remove("active"));
        tab.classList.add("active");
        this.activeCategory = tab.dataset.cat;
        this.renderAssetGrid();
      });
    });

    // 右欄分頁切換
    document.querySelectorAll(".right-tab").forEach((tab) => {
      tab.addEventListener("click", () => {
        document.querySelectorAll(".right-tab").forEach((b) => b.classList.remove("active"));
        tab.classList.add("active");
        this.activeRightTab = tab.dataset.rtab;
        this.renderRightPanel();
      });
    });

    // Modal 關閉通用鈕
    document.querySelectorAll("[data-close-modal]").forEach((btn) => {
      btn.addEventListener("click", () => {
        document.getElementById(btn.dataset.closeModal).classList.add("hidden");
      });
    });

    // 重新命名目前世界
    document.getElementById("btnRenameWorld")?.addEventListener("click", () => this.openRenameWorldModal());
    document.getElementById("btnConfirmRenameWorld")?.addEventListener("click", () => this.confirmRenameWorld());
    document.getElementById("rwNameInput")?.addEventListener("keydown", (e) => {
      if (e.key === "Enter") this.confirmRenameWorld();
    });

    // 本世界資產庫快速移除確認條
    document.getElementById("btnCancelRemoveWorldAsset")?.addEventListener("click", () => this.cancelRemoveWorldAsset());
    document.getElementById("btnConfirmRemoveWorldAsset")?.addEventListener("click", () => this.confirmRemoveWorldAsset());

    // 新建主題世界（支援 5 世界聯動企劃 + 12 種戶外與建築內部場景）
    let chosenTheme = "kingdom";
    document.getElementById("btnNewWorld").addEventListener("click", () => {
      document.getElementById("newWorldModal").classList.remove("hidden");
    });
    document.querySelectorAll("#newWorldModal .theme-card").forEach((card) => {
      card.addEventListener("click", () => {
        document.querySelectorAll("#newWorldModal .theme-card").forEach((c) => c.classList.remove("active"));
        card.classList.add("active");
        chosenTheme = card.dataset.theme;
        const nameInp = document.getElementById("nwNameInput");
        if (nameInp) {
          const cur = nameInp.value.trim();
          if (chosenTheme === "office_worker_day" && (!cur || cur === "我的夢幻新世界")) {
            nameInp.value = "社畜的一天";
          } else if (chosenTheme !== "office_worker_day" && cur === "社畜的一天") {
            nameInp.value = "我的夢幻新世界";
          }
        }
      });
    });
    document.getElementById("btnConfirmNewWorld").addEventListener("click", async () => {
      const name = document.getElementById("nwNameInput").value.trim() || "我的像素世界";
      const author = document.getElementById("nwAuthorInput").value.trim() || "像素建築師";
      const res = await fetch("/api/worlds", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name, theme: chosenTheme, author }),
      });
      const data = await res.json();
      if (!res.ok) return this.showToast(data.error || "建立失敗", true);
      document.getElementById("newWorldModal").classList.add("hidden");
      if (data.is_campaign) {
        this.portalReturnStack = [];
        this.sharedAdventure = null;
      }
      await this.refreshWorldList();
      await this.loadWorld(data.world.file_name);
      this.showToast(
        data.is_campaign
          ? `🎉 已一次建置《${name}》${data.count || 5} 座聯動世界（住家・公車都市・辦公室・大賣場・餐廳）！`
          : `世界「${data.world.name}」已建置完成！`,
      );
    });

    // 好友分享（單一世界 .stackworld.db ＋ 多世界 .stackpack.zip ＋ 獨立遊戲 .html ＋ 匯入）
    document.getElementById("btnShareWorld").addEventListener("click", async () => {
      const prevLink = document.getElementById("btnPreviewPlayHtml");
      if (prevLink && this.currentWorldFile) {
        prevLink.href = `/api/worlds/${encodeURIComponent(this.currentWorldFile)}/export-html?preview=1`;
      }
      const infoEl = document.getElementById("shareLinkedInfo");
      if (infoEl && this.currentWorldFile) {
        infoEl.textContent = "🔍 正在檢查門連結的關聯世界...";
        try {
          const r = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/linked-worlds`);
          const d = await r.json();
          if (r.ok && Array.isArray(d.worlds)) {
            const names = d.worlds.map((w) => w.name).join("、");
            infoEl.textContent = d.count > 1
              ? `🔗 偵測到「門」聯動專案（共 ${d.count} 座世界：${names}），匯出 .html 與 .stackpack.zip 將自動全部打包！`
              : `🌍 目前為單一獨立世界（未連結其他門世界），可直接匯出分享。`;
          }
        } catch {
          infoEl.textContent = "🌍 準備好匯出或匯入世界專案！";
        }
      }
      document.getElementById("shareModal").classList.remove("hidden");
    });
    document.getElementById("btnExportWorldDb").addEventListener("click", () => {
      if (!this.currentWorldFile) return;
      window.location.href = `/api/worlds/${encodeURIComponent(this.currentWorldFile)}/export`;
      this.showToast("已開始打包下載單一世界檔（.stackworld.db）！");
    });
    document.getElementById("btnExportWorldPack")?.addEventListener("click", () => {
      if (!this.currentWorldFile) return;
      window.location.href = `/api/worlds/${encodeURIComponent(this.currentWorldFile)}/export-pack`;
      this.showToast("正在打包所有透過「門」相連的多世界編輯包（.stackpack.zip）！");
    });
    document.getElementById("btnExportPlayHtml")?.addEventListener("click", () => {
      if (!this.currentWorldFile) return;
      window.location.href = `/api/worlds/${encodeURIComponent(this.currentWorldFile)}/export-html`;
      this.showToast("正在產生單純遊戲網頁檔（.html），好友開啟即可直接跨世界遊玩！");
    });
    document.getElementById("btnImportWorldDb").addEventListener("click", async () => {
      const f = document.getElementById("importWorldFile").files?.[0];
      if (!f) return this.showToast("請選擇好友分享的 .stackworld.db 或 .stackpack.zip 檔案", true);
      const fd = new FormData();
      fd.append("file", f);
      const res = await fetch("/api/worlds/import", { method: "POST", body: fd });
      const data = await res.json();
      if (!res.ok) return this.showToast(data.error || "匯入失敗", true);
      document.getElementById("shareModal").classList.add("hidden");
      this.portalReturnStack = [];
      this.sharedAdventure = null;
      await this.refreshWorldList();
      await this.loadWorld(data.world.file_name);
      this.showToast(
        data.is_pack
          ? `已成功匯入多世界編輯包（共 ${data.count} 座聯動世界）並開啟「${data.world.name}」！`
          : `已成功匯入好友的世界「${data.world.name}」！`,
      );
    });

    // 快照與重置／刪除世界管理
    document.getElementById("btnResetProgress")?.addEventListener("click", () => this.openResetModal());
    document.getElementById("btnDoResetProgress")?.addEventListener("click", () => this.resetGameProgress("progress_only"));
    document.getElementById("btnDoResetInitial")?.addEventListener("click", () => this.resetGameProgress("world_initial"));
    document.getElementById("btnAskDeleteWorld")?.addEventListener("click", () => {
      document.getElementById("deleteWorldConfirmBox")?.classList.remove("hidden");
    });
    document.getElementById("btnCancelDeleteWorld")?.addEventListener("click", () => {
      document.getElementById("deleteWorldConfirmBox")?.classList.add("hidden");
    });
    document.getElementById("btnConfirmDeleteWorld")?.addEventListener("click", () => this.deleteCurrentWorld());

    document.getElementById("btnSnapshot").addEventListener("click", () => {
      this._renderSnapshotList();
      document.getElementById("snapshotModal").classList.remove("hidden");
    });
    document.getElementById("btnCreateSnapshot").addEventListener("click", async () => {
      if (!this.currentWorldFile) return;
      const label = document.getElementById("snapLabelInput").value.trim() || "快速快照";
      const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/snapshots`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ label }),
      });
      const data = await res.json();
      if (res.ok) {
        this.worldState.snapshots.unshift(data.snapshot);
        document.getElementById("snapLabelInput").value = "";
        this._renderSnapshotList();
        this.showToast("世界快照已存檔！");
      }
    });
  }

  _bindCanvasControls() {
    const cv = this.glCanvas;
    cv.addEventListener("contextmenu", (e) => e.preventDefault());

    cv.addEventListener("mousedown", (e) => {
      this.mouseState.downBtn = e.button;
      this.mouseState.downX = e.clientX;
      this.mouseState.downY = e.clientY;
      this.mouseState.lastX = e.clientX;
      this.mouseState.lastY = e.clientY;
      this.mouseState.moved = false;

      const cam = this.renderer.camera;
      const isPanAction = e.button === 1
        || (e.button === 0 && !cam.walkMode && (this.tools.activeTool === "pan" || this.keysDown.has(" ")));
      this.mouseState.isPanning = isPanAction;
      if (isPanAction) {
        cv.style.cursor = "grabbing";
        return;
      }

      if (e.button === 0 && !cam.walkMode) {
        const hit = this.renderer.pickScene(e.clientX, e.clientY, this.sim.entities, this.assetMap);
        if (this.sim.visitorMode) {
          if (hit.entity) this.sim.interactWithEntity(hit.entity.id, true);
          return;
        }
        if (this.tools.activeTool === "select") {
          if (hit.entity) {
            this.renderer.selectedEntityId = hit.entity.id;
            this.mouseState.draggingEntityId = hit.entity.id;
            this.sim.interactWithEntity(hit.entity.id, false);
            this.renderRightPanel();
          } else {
            this.renderer.selectedEntityId = null;
            this.renderRightPanel();
          }
        } else {
          this._applyToolClick(hit);
        }
      }
      // walkMode 下不在 mousedown 急著觸發點擊，留待 mouseup 區分是「拖曳轉向」還是「點擊物件互動」
    });

    window.addEventListener("mousemove", (e) => {
      const rect = cv.getBoundingClientRect();
      const inside = e.clientX >= rect.left && e.clientX <= rect.right && e.clientY >= rect.top && e.clientY <= rect.bottom;
      const cam = this.renderer.camera;

      if (inside) {
        const hit = this.renderer.pickScene(e.clientX, e.clientY, this.sim.entities, this.assetMap);
        this.renderer.hoverTile = hit.tile;
        this.renderer.hoverWorldPos = hit.worldPos;
        this.renderer.hoverEntity = hit.entity;
        if (!this.mouseState.isPanning) {
          if ((cam.walkMode || this.sim.visitorMode) && hit.entity) {
            cv.style.cursor = "pointer";
          } else if (this.tools.activeTool === "pan") {
            cv.style.cursor = "grab";
          } else {
            cv.style.cursor = "default";
          }
        }
      }

      if (this.mouseState.downBtn === -1) return;
      const dx = e.clientX - this.mouseState.lastX;
      const dy = e.clientY - this.mouseState.lastY;
      if (Math.hypot(e.clientX - this.mouseState.downX, e.clientY - this.mouseState.downY) > 4) {
        this.mouseState.moved = true;
      }
      this.mouseState.lastX = e.clientX;
      this.mouseState.lastY = e.clientY;

      if (this.mouseState.isPanning) {
        this.cameraFollowPlayer = false;
        const panSpd = cam.dist * 0.0022;
        const fx = -Math.sin(cam.yaw);
        const fy = -Math.cos(cam.yaw);
        const rx = -Math.cos(cam.yaw);
        const ry = Math.sin(cam.yaw);
        // 滑鼠往右拖曳時畫面跟著往右移動（目標往螢幕左側方向移動）
        cam.target[0] -= (dx * rx - dy * fx) * panSpd;
        cam.target[1] -= (dx * ry - dy * fy) * panSpd;
        return;
      }

      if (this.mouseState.downBtn === 2 || (this.mouseState.downBtn === 0 && cam.walkMode)) {
        if (cam.walkMode) {
          cam.walkYaw += dx * 0.006;
        } else {
          cam.yaw += dx * 0.007;
          cam.pitch = Math.max(0.30, Math.min(1.38, cam.pitch + dy * 0.006));
        }
      } else if (this.mouseState.downBtn === 0 && !this.sim.visitorMode) {
        if (this.tools.activeTool === "select" && this.mouseState.draggingEntityId) {
          const hit = this.renderer.pickScene(e.clientX, e.clientY, [], this.assetMap);
          if (hit.worldPos) {
            const ent = this.sim.entities.find((x) => x.id === this.mouseState.draggingEntityId);
            if (ent) {
              ent.x = Number(hit.worldPos[0].toFixed(2));
              ent.y = Number(hit.worldPos[1].toFixed(2));
            }
          }
        } else if (["terrain", "road", "brush"].includes(this.tools.activeTool)) {
          const hit = this.renderer.pickScene(e.clientX, e.clientY, [], this.assetMap);
          if (hit.worldPos) this._applyToolDrag(hit);
        }
      }
    });

    window.addEventListener("mouseup", (e) => {
      const cam = this.renderer.camera;
      if (this.mouseState.downBtn === 0 && cam.walkMode && !this.mouseState.moved) {
        // 街道漫步點擊物件進行互動！
        const hit = this.renderer.pickScene(e.clientX, e.clientY, this.sim.entities, this.assetMap);
        if (hit.entity) {
          this.sim.interactWithEntity(hit.entity.id, true);
        }
      }

      if (this.mouseState.draggingEntityId && this.mouseState.moved) {
        const ent = this.sim.entities.find((x) => x.id === this.mouseState.draggingEntityId);
        if (ent) {
          this.updateEntityFields(ent.id, { x: ent.x, y: ent.y });
          this.renderRightPanel();
        }
      }
      this.mouseState.downBtn = -1;
      this.mouseState.draggingEntityId = null;
      this.mouseState.isPanning = false;
      cv.style.cursor = this.tools.activeTool === "pan" ? "grab" : "default";
    });

    cv.addEventListener("dblclick", (e) => {
      const cam = this.renderer.camera;
      if (cam.walkMode) return;
      const hit = this.renderer.pickScene(e.clientX, e.clientY, this.sim.entities, this.assetMap);
      if (hit.entity) {
        this.sim.interactWithEntity(hit.entity.id, true);
      }
    });

    cv.addEventListener("wheel", (e) => {
      e.preventDefault();
      const maxD = this.renderer.getMaxZoomDist();
      this.renderer.camera.dist = Math.max(4.0, Math.min(maxD, this.renderer.camera.dist * (1 + e.deltaY * 0.001)));
    }, { passive: false });

    window.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        ["encounterModal", "victoryModal", "resetModal"].forEach((id) => {
          document.getElementById(id)?.classList.add("hidden");
        });
        this.quest.activeEncounterEntityId = null;
        return;
      }
      if (document.querySelector(".modal-backdrop:not(.hidden)")) {
        this.keysDown.clear();
        return;
      }
      if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName)) return;
      const lower = e.key.toLowerCase();
      this.keysDown.add(lower);
      if (lower === "r") {
        this.cameraFollowPlayer = false;
        this.renderer.resetCamera();
      }
      if (lower === "f" && this.sim.player.visible && !this.renderer.camera.followEntityId) {
        e.preventDefault();
        this.sim.interactNearestToPlayer(2.8);
      }
      if (lower === "g" && this.sim.player.visible && !this.renderer.camera.followEntityId) {
        e.preventDefault();
        const res = this.sim.toggleMountNearest();
        if (res.ok && res.vehicle) this.cameraFollowPlayer = true;
      }
      if (lower === "tab" && this.sim.player.mountedEntityId) {
        e.preventDefault();
        this.sim.toggleMountControlMode();
      }
      if (e.key === " " && this.sim.player.visible && !this.mouseState.isPanning) {
        e.preventDefault();
        const isAirDrive =
          this.sim.player.mountedEntityId &&
          this.sim.player.mountControlMode === "drive" &&
          this.sim.player.mountConfig?.mobility === "air";
        if (!isAirDrive) {
          this.sim.jumpPlayer();
        }
      }
      if (e.key === "Delete" && this.renderer.selectedEntityId && !this.sim.visitorMode) {
        this.deleteEntityById(this.renderer.selectedEntityId);
      }
    });
    window.addEventListener("keyup", (e) => {
      this.keysDown.delete(e.key.toLowerCase());
    });
  }

  _applyToolClick(hit) {
    if (!hit.tile || !hit.worldPos) return;
    const t = this.tools.activeTool;
    if (t === "place") {
      if (!this.selectedAssetId) return this.showToast("請先在左側資產庫選擇一個物件！", true);
      const asset = this.assetMap.get(this.selectedAssetId);
      // 透過地形網格查表取得真實地形與道路屬性，避免陸地角色或建築家具誤投進深水
      const w = this.worldState?.meta?.width || this.renderer.worldWidth || 40;
      const gridTile = this.renderer.terrainGrid[hit.tile.y * w + hit.tile.x];
      const isWaterNoBridge = Boolean(
        gridTile && gridTile.ground_type === "water" && gridTile.road_type !== "bridge"
      );
      if (isWaterNoBridge) {
        const mob = this.sim.getActorMobility(asset, asset?.name, 0);
        if (mob === "land") {
          return this.showToast("陸地角色、動物、建築與家具不可直接放進深水，請放於地面或木橋上！", true);
        }
      }
      const wx = Math.floor(hit.worldPos[0]) + 0.5;
      const wy = Math.floor(hit.worldPos[1]) + 0.5;
      this.spawnEntityAt(
        this.selectedAssetId,
        wx,
        wy,
        0,
        this.tools.placeYaw,
        (asset?.default_scale || 1.0) * this.tools.placeScale,
      );
    } else if (t === "brush") {
      this._applyEcoBrush(hit.worldPos);
    } else if (t === "terrain" || t === "road") {
      this._paintTerrainTile(hit.tile.x, hit.tile.y);
    } else if (t === "path") {
      this.renderer.draftPathWaypoints.push({
        x: Number(hit.worldPos[0].toFixed(2)),
        y: Number(hit.worldPos[1].toFixed(2)),
        z: 0,
        wait_sec: 0.5,
      });
      this.tools.renderSubOptions();
    } else if (t === "zone") {
      this.createWanderZone(hit.worldPos[0], hit.worldPos[1], this.tools.brushRadius);
    } else if (t === "erase") {
      if (hit.entity) {
        this.deleteEntityById(hit.entity.id);
      } else {
        this._clearRoadTile(hit.tile.x, hit.tile.y);
      }
    }
  }

  _applyToolDrag(hit) {
    const t = this.tools.activeTool;
    if (t === "terrain" || t === "road") {
      if (hit.tile) this._paintTerrainTile(hit.tile.x, hit.tile.y);
    } else if (t === "brush" && hit.worldPos) {
      const now = performance.now();
      if (now - this.lastBrushTime < 220) return;
      this.lastBrushTime = now;
      this._applyEcoBrush(hit.worldPos);
    }
  }

  /// 生態規則隨機筆刷：依生態群組挑出資產池、檢查水體/道路/最小間距並於圓形區域內隨機撒出
  async _applyEcoBrush(worldPos) {
    if (this.ecoSeedBusy || !this.currentWorldFile || !this.worldState) return;
    const cfg = this.tools.ecoConfig;
    let pool = [];

    if (cfg.preset === "custom") {
      pool = Array.from(cfg.customAssetIds)
        .map((id) => this.assetMap.get(id))
        .filter(Boolean);
      if (pool.length === 0 && this.selectedAssetId) {
        const cur = this.assetMap.get(this.selectedAssetId);
        if (cur) pool.push(cur);
      }
    } else {
      const rule = ECO_RULES[cfg.preset] || ECO_RULES.forest;
      const allAssets = Array.from(this.assetMap.values());
      pool = allAssets.filter(
        (a) =>
          rule.categories.includes(a.category) &&
          rule.keywords.some((kw) => a.name.toLowerCase().includes(kw)),
      );
      if (pool.length === 0) {
        pool = allAssets.filter((a) => rule.categories.includes(a.category));
      }
      // 若世界內剛好缺少此生態群組的代表物件，自動透過純 Rust 引擎補生成該生態基礎資產！
      if (pool.length === 0) {
        this.ecoSeedBusy = true;
        try {
          this.showToast("正在為此生態規則準備精靈素材...");
          for (const prompt of rule.fallbackPrompts) {
            const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/assets/generate`, {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ prompt, slices: 38, seed: Math.floor(Math.random() * 999) }),
            });
            const data = await res.json();
            if (res.ok && data.asset) {
              this.assetMap.set(data.asset.id, data.asset);
              await this.renderer.ensureAssetTextures(this.currentWorldFile, data.asset);
              pool.push(data.asset);
            }
          }
          this.renderAssetGrid();
        } finally {
          this.ecoSeedBusy = false;
        }
      }
    }

    if (pool.length === 0) {
      return this.showToast("生態池目前沒有可用資產，請先勾選自訂池或至工坊生成！", true);
    }

    const rad = this.tools.brushRadius;
    const minSpacing = cfg.minSpacing;
    const maxW = this.worldState.meta.width;
    const maxH = this.worldState.meta.height;
    const batch = [];
    const attempts = cfg.density * 10;

    for (let i = 0; i < attempts && batch.length < cfg.density; i++) {
      const ang = Math.random() * Math.PI * 2;
      const r = Math.sqrt(Math.random()) * rad;
      const px = Number((worldPos[0] + Math.cos(ang) * r).toFixed(2));
      const py = Number((worldPos[1] + Math.sin(ang) * r).toFixed(2));
      if (px < 0.6 || py < 0.6 || px > maxW - 0.6 || py > maxH - 0.6) continue;

      const gx = Math.floor(px);
      const gy = Math.floor(py);
      const tile = this.renderer.terrainGrid[gy * maxW + gx];
      if (!tile) continue;
      if (cfg.avoidWaterRoad) {
        if (
          tile.ground_type === "water" ||
          (tile.road_type && tile.road_type !== "none") ||
          tile.ground_type.startsWith("wall_")
        ) {
          continue;
        }
      }

      const tooCloseExisting = this.sim.entities.some(
        (e) => Math.hypot(e.x - px, e.y - py) < minSpacing,
      );
      const tooCloseBatch = batch.some(
        (b) => Math.hypot(b.x - px, b.y - py) < minSpacing,
      );
      const insideCollider = this.sim.entities.some((e) => {
        const col = this.sim.getEntityCollider(e);
        return col && this.sim._isCircleHittingObb(px, py, 0.45, col);
      });
      if (tooCloseExisting || tooCloseBatch || insideCollider) continue;

      const chosen = pool[Math.floor(Math.random() * pool.length)];
      const yaw = cfg.randomYaw ? Math.floor(Math.random() * 360) : 0;
      const scaleMul = cfg.randomScale ? 0.78 + Math.random() * 0.56 : 1.0;
      batch.push({
        asset_id: chosen.id,
        x: px,
        y: py,
        z: 0,
        yaw,
        scale: Number(((chosen.default_scale || 1.0) * scaleMul).toFixed(2)),
      });
    }

    if (batch.length === 0) return;

    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/entities`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ entities: batch }),
    });
    const data = await res.json();
    if (!res.ok) return this.showToast(data.error || "生態筆刷撒出失敗", true);
    for (const e of data.entities || []) {
      e._phase = Math.random() * Math.PI * 2;
      e._reveal = 0.02;
      e._tweenMul = 1.0;
      this.sim.entities.push(e);
    }
    soundEngine.play("place");
    this._updateWorldBadge(this.sim.entities.length);
  }

  async _paintTerrainTile(gx, gy) {
    if (!this.worldState || gx < 0 || gy < 0 || gx >= this.worldState.meta.width || gy >= this.worldState.meta.height) return;
    const tile = this.renderer.terrainGrid[gy * this.worldState.meta.width + gx];
    if (!tile) return;

    const updated = { ...tile };
    if (this.tools.activeTool === "terrain") {
      const m = this.tools.terrainMode;
      if (m === "raise") updated.elevation = Math.min(5, tile.elevation + 1);
      else if (m === "lower") updated.elevation = Math.max(0, tile.elevation - 1);
      else if (m === "water") { updated.ground_type = "water"; updated.elevation = 0; }
      else if (m.startsWith("wall_")) {
        updated.ground_type = m;
        updated.elevation = 4;
      } else {
        updated.ground_type = m;
        if (updated.elevation === 0) updated.elevation = 1;
      }
    } else if (this.tools.activeTool === "road") {
      const rm = this.tools.roadMode;
      if (tile.ground_type === "water" && rm !== "none") updated.road_type = "bridge";
      else updated.road_type = rm;
    }

    this.renderer.terrainGrid[gy * this.worldState.meta.width + gx] = updated;
    this.renderer.rebuildTerrainMesh(this.worldState.meta.width, this.worldState.meta.height, this.renderer.terrainGrid);
    soundEngine.play("place");
    await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/terrain/batch`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ tiles: [updated] }),
    });
  }

  async _clearRoadTile(gx, gy) {
    const tile = this.renderer.terrainGrid[gy * this.worldState.meta.width + gx];
    if (!tile || tile.road_type === "none") return;
    const updated = { ...tile, road_type: "none" };
    this.renderer.terrainGrid[gy * this.worldState.meta.width + gx] = updated;
    this.renderer.rebuildTerrainMesh(this.worldState.meta.width, this.worldState.meta.height, this.renderer.terrainGrid);
    await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/terrain/batch`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ tiles: [updated] }),
    });
  }

  async spawnEntityAt(assetId, x, y, z, yaw, scale, scaleX = null, scaleY = null, scaleZ = null) {
    const payload = { asset_id: assetId, x, y, z, yaw, scale };
    if (scaleX !== null) payload.scale_x = scaleX;
    if (scaleY !== null) payload.scale_y = scaleY;
    if (scaleZ !== null) payload.scale_z = scaleZ;
    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/entities`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    const data = await res.json();
    if (!res.ok) return this.showToast(data.error || "投放失敗", true);
    for (const e of data.entities || []) {
      e._phase = Math.random() * Math.PI * 2;
      e._reveal = 0.02;
      e._tweenMul = 1.0;
      this.sim.entities.push(e);
      this.renderer.selectedEntityId = e.id;
    }
    soundEngine.play("place");
    this._updateWorldBadge(this.sim.entities.length);
    this.renderRightPanel();
  }

  async updateEntityFields(id, fields) {
    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/entities/${encodeURIComponent(id)}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(fields),
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) {
      this.showToast(data.error || "更新物件屬性失敗", true);
      return false;
    }
    const idx = this.sim.entities.findIndex((e) => e.id === id);
    if (idx !== -1) Object.assign(this.sim.entities[idx], data.entity);
    return true;
  }

  async deleteEntityById(id) {
    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/entities/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
    if (res.ok) {
      for (const e of this.sim.entities) {
        if (e.parent_id === id) e.parent_id = null;
      }
      for (const p of this.sim.paths) {
        if (p.entity_id === id) p.entity_id = null;
      }
      this.sim.entities = this.sim.entities.filter((e) => e.id !== id);
      if (this.renderer.selectedEntityId === id) this.renderer.selectedEntityId = null;
      soundEngine.play("delete");
      this._updateWorldBadge(this.sim.entities.length);
      this.renderRightPanel();
    }
  }

  openRenameWorldModal() {
    if (!this.currentWorldFile || !this.worldState?.meta) return;
    const m = this.worldState.meta;
    const nameEl = document.getElementById("rwNameInput");
    const authEl = document.getElementById("rwAuthorInput");
    const descEl = document.getElementById("rwDescInput");
    if (nameEl) nameEl.value = m.name || "";
    if (authEl) authEl.value = m.author || "";
    if (descEl) descEl.value = m.description || "";
    document.getElementById("renameWorldModal")?.classList.remove("hidden");
    setTimeout(() => {
      nameEl?.focus();
      nameEl?.select();
    }, 30);
  }

  async confirmRenameWorld() {
    if (!this.currentWorldFile) return;
    const nextName = document.getElementById("rwNameInput")?.value.trim() || "";
    const nextAuthor = document.getElementById("rwAuthorInput")?.value.trim();
    const nextDesc = document.getElementById("rwDescInput")?.value.trim();
    if (!nextName) {
      this.showToast("世界名稱不可空白！", true);
      return;
    }
    const payload = { name: nextName };
    if (nextAuthor) payload.author = nextAuthor;
    if (nextDesc !== undefined) payload.description = nextDesc;

    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/meta`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) {
      return this.showToast(data.error || "重新命名世界失敗", true);
    }
    if (data.meta && this.worldState) {
      this.worldState.meta = data.meta;
    } else if (this.worldState?.meta) {
      this.worldState.meta.name = nextName;
    }
    document.getElementById("renameWorldModal")?.classList.add("hidden");
    await this.refreshWorldList();
    this._updateWorldBadge(this.sim.entities.length);
    this.showToast(`世界已重新命名為「${nextName}」！`);
  }

  openResetModal() {
    if (!this.currentWorldFile) return;
    const nameEl = document.getElementById("deleteWorldTargetName");
    if (nameEl) {
      nameEl.textContent = this.worldState?.meta?.name || this.currentWorldFile;
    }
    document.getElementById("deleteWorldConfirmBox")?.classList.add("hidden");
    document.getElementById("resetModal")?.classList.remove("hidden");
  }

  async deleteCurrentWorld() {
    if (!this.currentWorldFile) return;
    const deletedName = this.worldState?.meta?.name || this.currentWorldFile;
    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}`, {
      method: "DELETE",
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) {
      return this.showToast(data.error || "刪除世界失敗", true);
    }
    document.getElementById("resetModal")?.classList.add("hidden");
    document.getElementById("deleteWorldConfirmBox")?.classList.add("hidden");
    this.currentWorldFile = null;
    await this.refreshWorldList();
    if (this.worlds.length > 0) {
      await this.loadWorld(this.worlds[0].file_name);
    }
    this.showToast(
      data.recreated_default
        ? `已刪除「${deletedName}」，並自動為您建立新的預設世界！`
        : `已永久刪除世界「${deletedName}」！`,
    );
  }

  async resetGameProgress(mode = "progress_only") {
    if (!this.currentWorldFile) return;
    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/reset`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ mode }),
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) {
      return this.showToast(data.error || "重置世界失敗", true);
    }
    this.sharedAdventure = null;
    this.portalReturnStack = [];
    document.getElementById("resetModal")?.classList.add("hidden");
    document.getElementById("victoryModal")?.classList.add("hidden");
    document.getElementById("encounterModal")?.classList.add("hidden");
    await this.loadWorld(this.currentWorldFile);
    this.showToast(
      mode === "world_initial"
        ? "已徹底回復此世界的初始出廠狀態！"
        : "已清除遊玩進度、積分與筆記，祝您挑戰順利！",
    );
  }

  /// 從 QuestManager 即時同步跨世界冒險背包與積分快照
  syncSharedAdventureFromQuest() {
    if (this.quest) {
      this.sharedAdventure = this.quest.exportSharedAdventure();
    }
  }

  /// 檢查門 (`portal`) 是否有道具或旗標上鎖條件
  _checkPortalLock(portal) {
    if (!portal) return { locked: false };
    const reqItem = (portal.required_item || "").trim();
    const reqFlag = (portal.required_flag || "").trim();
    if (reqItem && !this.quest.hasItem(reqItem)) {
      return {
        locked: true,
        reason: portal.locked_text || `門上鎖了！需要「${reqItem}」才能進入。`,
      };
    }
    if (reqFlag && !this.quest.hasFlag(reqFlag)) {
      return {
        locked: true,
        reason: portal.locked_text || "尚未達成開門條件，目前無法進入！",
      };
    }
    return { locked: false };
  }

  /// 一鍵為指定建築物／門建立全新室內世界並雙向連結
  async quickLinkInteriorWorld(entity, theme, name) {
    if (!this.currentWorldFile || !entity) return;
    const res = await fetch(
      `/api/worlds/${encodeURIComponent(this.currentWorldFile)}/entities/${encodeURIComponent(entity.id)}/link-interior`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ theme, name }),
      },
    );
    const data = await res.json().catch(() => ({}));
    if (!res.ok) {
      return this.showToast(data.error || "建立室內世界連結失敗", true);
    }
    if (data.source_entity) {
      const idx = this.sim.entities.findIndex((e) => e.id === entity.id);
      if (idx !== -1) Object.assign(this.sim.entities[idx], data.source_entity);
      Object.assign(entity, data.source_entity);
    }
    await this.refreshWorldList();
    this.renderRightPanel();
    const intName = data.interior_world?.name || name;
    this.showToast(`已建立室內世界「${intName}」並完成雙向門連結！按 F 或點擊「立即進門前往」即可進入！`);
  }

  /// 執行跨世界進門／出門轉場，保留玩家造型並同步共享冒險進度
  async handlePortalTransition(ent, portal) {
    if (this._portalBusy || !portal) return;
    this._portalBusy = true;
    const overlay = document.getElementById("portalTransition");
    const overlayText = document.getElementById("portalTransitionText");

    try {
      this.syncSharedAdventureFromQuest();
      await this.quest.persistProgress(true);
      this.keysDown.clear();

      const curAsset = this.sim.player.asset_id ? this.assetMap.get(this.sim.player.asset_id) : null;
      const carryAvatarSha = curAsset?.sha256 || null;
      const carryAvatarName = curAsset?.name || null;
      const carryPlayerName = this.sim.player.name;
      const carryPlayerScale = this.worldState?.meta?.player_scale ?? 1.0;

      let targetFile = null;
      let spawnX = typeof portal.target_x === "number" ? portal.target_x : null;
      let spawnY = typeof portal.target_y === "number" ? portal.target_y : null;
      let spawnYaw = typeof portal.target_yaw === "number" ? portal.target_yaw : undefined;
      let targetPortalId = portal.target_portal_id || null;

      const curWorldId = this.worldState?.meta?.world_id;

      if (portal.kind === "exit" && this.portalReturnStack.length > 0) {
        const ret = this.portalReturnStack.pop();
        const matchById = ret.worldId && this.worlds.find((w) => w.world_id === ret.worldId);
        const matchByFile = ret.fileName && this.worlds.find((w) => w.file_name === ret.fileName);
        targetFile = (matchById || matchByFile)?.file_name || null;
        if (typeof ret.x === "number") spawnX = ret.x;
        if (typeof ret.y === "number") spawnY = ret.y;
        if (typeof ret.yaw === "number") spawnYaw = ret.yaw;
      } else {
        if (!this.worlds.length) await this.refreshWorldList();
        const findTarget = () =>
          (portal.target_world_id && this.worlds.find((w) => w.world_id === portal.target_world_id)) ||
          (portal.target_world_file && this.worlds.find((w) => w.file_name === portal.target_world_file));
        let found = findTarget();
        if (!found) {
          await this.refreshWorldList();
          found = findTarget();
        }
        targetFile = found?.file_name || null;

        if (targetFile && portal.kind !== "exit") {
          const doorPos = this.sim.getPortalWorldPos(ent, portal) || { x: this.sim.player.x, y: this.sim.player.y };
          const yawRad = ((ent.yaw || 0) * Math.PI) / 180;
          const pushX = doorPos.x + (-Math.sin(yawRad)) * 0.45;
          const pushY = doorPos.y + Math.cos(yawRad) * 0.45;
          const [safeRetX, safeRetY] = this.sim._findSafeLandPoint(pushX, pushY, false);
          this.portalReturnStack.push({
            worldId: curWorldId,
            fileName: this.currentWorldFile,
            x: safeRetX,
            y: safeRetY,
            yaw: ((ent.yaw || 0) + 180) % 360,
          });
        }
      }

      if (!targetFile) {
        return this.showToast("找不到門連結的目標世界（可能已被刪除或尚未設定目標世界）。", true);
      }

      const targetMeta = this.worlds.find((w) => w.file_name === targetFile);
      if (overlayText) {
        overlayText.textContent = portal.kind === "exit"
          ? `🚪 正在返回「${targetMeta?.name || "原世界"}」...`
          : `🚪 正在進入「${targetMeta?.name || "目標世界"}」...`;
      }
      overlay?.classList.add("active");
      await new Promise((r) => setTimeout(r, 210));

      const loadRes = await this.loadWorld(targetFile, {
        spawnX,
        spawnY,
        spawnYaw,
        targetPortalId,
        preserveWalkMode: true,
        isPortalJump: true,
        keepSharedAdventure: true,
        carryAvatarSha,
        carryAvatarName,
        carryPlayerName,
        carryPlayerScale,
      });

      await new Promise((r) => setTimeout(r, 140));
      overlay?.classList.remove("active");

      if (loadRes?.portalVictoryPending) {
        setTimeout(() => this.quest.triggerVictory(), 280);
      }
    } finally {
      overlay?.classList.remove("active");
      this._portalBusy = false;
    }
  }

  async commitDraftPath() {
    const wps = this.renderer.draftPathWaypoints;
    if (wps.length < 2) return this.showToast("路徑至少需要 2 個節點！", true);
    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/paths`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: `巡航路線 ${this.sim.paths.length + 1}`,
        entity_id: this.renderer.selectedEntityId,
        loop_mode: "loop",
        speed: 2.2,
        smooth: true,
        active: true,
        color: "#38bdf8",
        waypoints: wps,
      }),
    });
    const data = await res.json();
    if (res.ok) {
      this.sim.paths.push(data.path);
      this.renderer.draftPathWaypoints = [];
      this.tools.setTool("select");
      this.activeRightTab = "motion";
      this._syncRightTabs();
      this.renderRightPanel();
      this.showToast("移動路徑建立完成！");
    }
  }

  async savePath(p) {
    await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/paths/${encodeURIComponent(p.id)}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(p),
    });
  }

  async deletePath(id) {
    await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/paths/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
    this.sim.paths = this.sim.paths.filter((p) => p.id !== id);
    this.renderRightPanel();
  }

  async createWanderZone(cx, cy, radius) {
    const entityIds = this.renderer.selectedEntityId ? [this.renderer.selectedEntityId] : [];
    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/wander-zones`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: `漫遊圍欄 ${this.sim.wanderZones.length + 1}`,
        cx, cy, radius, speed: 1.2, entity_ids: entityIds, color: "#4ade80",
      }),
    });
    const data = await res.json();
    if (res.ok) {
      this.sim.wanderZones.push(data.wander_zone);
      this.activeRightTab = "motion";
      this._syncRightTabs();
      this.renderRightPanel();
      this.showToast("已建立自由漫遊區！");
    }
  }

  async saveZone(z) {
    await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/wander-zones/${encodeURIComponent(z.id)}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(z),
    });
    this.renderRightPanel();
  }

  async deleteZone(id) {
    await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/wander-zones/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
    this.sim.wanderZones = this.sim.wanderZones.filter((z) => z.id !== id);
    this.renderRightPanel();
  }

  async createEventRule(rule) {
    const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/events`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(rule),
    });
    const data = await res.json();
    if (res.ok) {
      this.sim.events.push(data.event);
      this.renderRightPanel();
      this.showToast(`已新增事件「${data.event.name}」！`);
    }
  }

  async updateEventRule(r) {
    await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/events/${encodeURIComponent(r.id)}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(r),
    });
  }

  async deleteEventRule(id) {
    await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/events/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
    this.sim.events = this.sim.events.filter((e) => e.id !== id);
    this.renderRightPanel();
  }

  async _onStudioAssetReady(asset, startPlacing) {
    this.assetMap.set(asset.id, asset);
    await this.renderer.ensureAssetTextures(this.currentWorldFile, asset);
    this.selectedAssetId = asset.id;
    this.renderAssetGrid();
    this.tools.renderSubOptions();
    if (startPlacing) {
      this.tools.setTool("place");
      this.showToast(`已切換投放工具，點擊地圖即可放進「${asset.name}」！`);
    }
  }

  async _onStudioAssetUpdated(_scope, asset) {
    if (!asset) return;
    this.assetMap.set(asset.id, asset);
    if (this.worldState?.assets) {
      const idx = this.worldState.assets.findIndex((a) => a.id === asset.id);
      if (idx >= 0) this.worldState.assets[idx] = asset;
    }
    // 清除貼圖快取以確保重烘後立即重新上傳 WebGL2 紋理
    this.renderer.assetTextures.delete(asset.id);
    await this.renderer.ensureAssetTextures(this.currentWorldFile, asset);
    this.renderAssetGrid();
    this.renderRightPanel();
    this.tools.renderSubOptions();
  }

  async _onStudioAssetDeleted(_scope, assetId, removedEntityIds = []) {
    this.assetMap.delete(assetId);
    this.renderer.assetTextures.delete(assetId);
    if (this.worldState?.assets) {
      this.worldState.assets = this.worldState.assets.filter((a) => a.id !== assetId);
    }
    if (removedEntityIds.length > 0) {
      const rmSet = new Set(removedEntityIds);
      this.sim.entities = this.sim.entities.filter((e) => !rmSet.has(e.id));
      for (const e of this.sim.entities) {
        if (e.parent_id && rmSet.has(e.parent_id)) e.parent_id = null;
      }
      if (this.renderer.selectedEntityId && rmSet.has(this.renderer.selectedEntityId)) {
        this.renderer.selectedEntityId = null;
      }
    }
    if (this.selectedAssetId === assetId) {
      this.selectedAssetId = this.worldState?.assets?.[0]?.id || null;
    }
    // 若玩家當前化身剛好使用被移除的資產，安全切換為本世界其餘可用人物／動物資產
    if (this.sim.player.asset_id === assetId) {
      const candidates = Array.from(this.assetMap.values());
      const nextAvatar =
        candidates.find((a) => a.category === "character" || a.category === "animal") ||
        candidates[0] ||
        null;
      this.sim.player.asset_id = nextAvatar ? nextAvatar.id : null;
      if (this.worldState?.meta) {
        this.worldState.meta.player_asset_id = this.sim.player.asset_id;
      }
      if (!nextAvatar) this.sim.player.visible = false;
    }
    if (this.pendingRemoveAssetId === assetId) {
      this.cancelRemoveWorldAsset();
    }
    this._updateWorldBadge(this.sim.entities.length);
    this.renderAssetGrid();
    this.renderRightPanel();
    this.tools.renderSubOptions();
  }

  promptRemoveWorldAsset(asset) {
    if (!asset || !this.currentWorldFile) return;
    this.pendingRemoveAssetId = asset.id;
    const usedCount = this.sim.entities.filter((e) => e.asset_id === asset.id).length;
    const msgEl = document.getElementById("assetRemoveMsg");
    if (msgEl) {
      msgEl.textContent = usedCount > 0
        ? `確定從本世界資產庫移除「${asset.name}」？（地圖上 ${usedCount} 個此物件也會同步移除）`
        : `確定從本世界資產庫移除「${asset.name}」？`;
    }
    document.getElementById("assetRemoveBar")?.classList.remove("hidden");
  }

  cancelRemoveWorldAsset() {
    this.pendingRemoveAssetId = null;
    document.getElementById("assetRemoveBar")?.classList.add("hidden");
  }

  async confirmRemoveWorldAsset() {
    if (!this.currentWorldFile || !this.pendingRemoveAssetId) return;
    const id = this.pendingRemoveAssetId;
    const assetName = this.assetMap.get(id)?.name || "物件";
    const res = await fetch(
      `/api/worlds/${encodeURIComponent(this.currentWorldFile)}/assets/${encodeURIComponent(id)}`,
      { method: "DELETE" },
    );
    const data = await res.json().catch(() => ({}));
    if (!res.ok) {
      return this.showToast(data.error || "移除本世界資產失敗", true);
    }
    const removedCount = (data.removed_entity_ids || []).length;
    await this._onStudioAssetDeleted("world", id, data.removed_entity_ids || []);
    this.cancelRemoveWorldAsset();
    this.showToast(
      removedCount > 0
        ? `已從本世界移除「${assetName}」（並清理 ${removedCount} 個場上物件）！`
        : `已從本世界資產庫移除「${assetName}」！`,
    );
  }

  renderAssetGrid() {
    const grid = document.getElementById("assetGrid");
    grid.innerHTML = "";
    const items = Array.from(this.assetMap.values()).filter(
      (a) => this.activeCategory === "all" || a.category === this.activeCategory,
    );
    for (const a of items) {
      const card = document.createElement("div");
      card.className = `asset-card ${a.id === this.selectedAssetId ? "selected" : ""}`;
      const delBtn = document.createElement("button");
      delBtn.type = "button";
      delBtn.className = "asset-card-del";
      delBtn.title = `從本世界資產庫移除「${a.name}」`;
      delBtn.textContent = "✕";
      delBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        this.promptRemoveWorldAsset(a);
      });
      card.appendChild(delBtn);

      const texEntry = this.renderer.assetTextures.get(a.id);
      if (texEntry?.thumbCanvas) {
        const clone = document.createElement("canvas");
        clone.width = texEntry.thumbCanvas.width;
        clone.height = texEntry.thumbCanvas.height;
        clone.getContext("2d").drawImage(texEntry.thumbCanvas, 0, 0);
        card.appendChild(clone);
      }
      const lbl = document.createElement("span");
      lbl.textContent = a.name;
      card.appendChild(lbl);
      card.addEventListener("click", () => {
        this.selectedAssetId = a.id;
        this.tools.setTool("place");
        this.renderAssetGrid();
      });
      grid.appendChild(card);
    }
  }

  renderRightPanel() {
    const body = document.getElementById("rightPanelBody");
    if (!body) return;
    try {
      if (this.activeRightTab === "inspector") {
        const sel = this.sim.entities.find((e) => e.id === this.renderer.selectedEntityId) || null;
        this.tools.renderInspector(body, sel);
      } else if (this.activeRightTab === "motion") {
        renderMotionPanel(body, this);
      } else if (this.activeRightTab === "events") {
        renderEventsPanel(body, this);
      } else {
        this.quest.renderQuestPlannerPanel(body);
      }
    } catch (err) {
      console.error("繪製右側面板發生錯誤：", err);
      body.innerHTML = `<div class="prop-card"><strong>面板載入錯誤</strong><p class="hint-text">${escapeHtml(err?.message || String(err))}</p></div>`;
    }
  }

  _syncRightTabs() {
    document.querySelectorAll(".right-tab").forEach((b) => {
      b.classList.toggle("active", b.dataset.rtab === this.activeRightTab);
    });
  }

  openPlayerModal() {
    const p = this.sim.player;
    const scale = this.worldState?.meta?.player_scale ?? 1.0;
    document.getElementById("plNameInput").value = p.name || "冒險玩家";
    document.getElementById("plScaleSlider").value = String(scale);
    document.getElementById("plScaleVal").textContent = `${Number(scale).toFixed(2)}x`;
    this.renderPlayerAssetGrid();
    document.getElementById("playerModal").classList.remove("hidden");
  }

  renderPlayerAssetGrid() {
    const grid = document.getElementById("playerAssetGrid");
    if (!grid) return;
    grid.innerHTML = "";
    const curId = this.sim.player.asset_id;
    // 優先將人物與動物排在前面方便挑選
    const order = { character: 0, animal: 1, vehicle: 2, prop: 3, plant: 4, building: 5 };
    const assets = Array.from(this.assetMap.values()).sort(
      (a, b) => (order[a.category] ?? 9) - (order[b.category] ?? 9),
    );
    for (const a of assets) {
      const card = document.createElement("div");
      card.className = `player-asset-card ${a.id === curId ? "selected" : ""}`;
      const texEntry = this.renderer.assetTextures.get(a.id);
      if (texEntry?.thumbCanvas) {
        const clone = document.createElement("canvas");
        clone.width = texEntry.thumbCanvas.width;
        clone.height = texEntry.thumbCanvas.height;
        clone.getContext("2d").drawImage(texEntry.thumbCanvas, 0, 0);
        card.appendChild(clone);
      }
      const lbl = document.createElement("span");
      lbl.textContent = a.name;
      card.appendChild(lbl);
      card.addEventListener("click", () => this.selectPlayerAsset(a.id));
      grid.appendChild(card);
    }
  }

  async selectPlayerAsset(assetId) {
    if (!this.assetMap.has(assetId)) return;
    const pScale = this.worldState?.meta?.player_scale ?? 1.0;
    this.sim.player.asset_id = assetId;
    this.sim.player.visible = true;
    this.sim.applyPlayerScale(pScale);
    if (this.worldState?.meta) {
      this.worldState.meta.player_asset_id = assetId;
    }
    const assetName = this.assetMap.get(assetId)?.name || "自訂角色";
    const gz = this.renderer.getTerrainElevationAt(this.sim.player.x, this.sim.player.y);
    this.renderer.spawnParticles("sparkle", this.sim.player.x, this.sim.player.y, gz + 1.4, 24);
    soundEngine.play("chime");
    this.renderPlayerAssetGrid();
    await this._patchWorldMeta({ player_asset_id: assetId });
    this.showToast(`玩家化身已更換為「${assetName}」！`);
  }

  async applyQuickPlayerPreset(presetKey, label) {
    if (this._playerBusy || !this.currentWorldFile) return;
    // 先於既有世界資產中尋找相符名稱的角色
    const existing = Array.from(this.assetMap.values()).find((a) => a.name.includes(label));
    if (existing) {
      await this.selectPlayerAsset(existing.id);
      return;
    }
    this._playerBusy = true;
    try {
      this.showToast(`正在即時烘焙「${label}」精靈造型...`);
      const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/assets/generate`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt: presetKey, slices: 36 }),
      });
      const data = await res.json();
      if (!res.ok) return this.showToast(data.error || "生成角色造型失敗", true);
      this.assetMap.set(data.asset.id, data.asset);
      await this.renderer.ensureAssetTextures(this.currentWorldFile, data.asset);
      this.renderAssetGrid();
      await this.selectPlayerAsset(data.asset.id);
    } finally {
      this._playerBusy = false;
    }
  }

  toggleCityWalk(entityId = null) {
    const cam = this.renderer.camera;
    if (cam.walkMode && !entityId) {
      cam.walkMode = false;
      cam.followEntityId = null;
      this.sim.player.anim_mode = "static";
      this.sim.player.z = 0;
      this._patchWorldMeta({ player_x: this.sim.player.x, player_y: this.sim.player.y });
    } else {
      cam.walkMode = true;
      cam.followEntityId = entityId;
      if (!entityId) {
        const px = Math.max(0.6, Math.min(this.renderer.worldWidth - 0.6, this.sim.player.x));
        const py = Math.max(0.6, Math.min(this.renderer.worldHeight - 0.6, this.sim.player.y));
        const gz = this.renderer.getTerrainElevationAt(px, py);
        cam.walkPos = [px, py, gz];
        cam.walkVelZ = 0;
        cam.walkYaw = cam.yaw;
      }
    }
    document.getElementById("btnCityWalk").classList.toggle("active", cam.walkMode);
    document.getElementById("cityWalkBanner").classList.toggle("hidden", !cam.walkMode);
  }

  _renderSnapshotList() {
    const box = document.getElementById("snapshotList");
    box.innerHTML = "";
    const snaps = this.worldState?.snapshots || [];
    if (snaps.length === 0) {
      box.innerHTML = "<div class='hint-text'>尚無快照紀錄。</div>";
      return;
    }
    for (const s of snaps) {
      const row = document.createElement("div");
      row.className = "snap-item";
      row.innerHTML = `<div><strong>${escapeHtml(s.label)}</strong><br><small style="color:#94a3b8;">${s.created_at.slice(0, 19).replace("T", " ")}</small></div>`;
      const btn = document.createElement("button");
      btn.className = "btn btn-sub";
      btn.textContent = "⏪ 還原";
      btn.addEventListener("click", async () => {
        const res = await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/snapshots/${encodeURIComponent(s.id)}/restore`, {
          method: "POST",
        });
        if (res.ok) {
          document.getElementById("snapshotModal").classList.add("hidden");
          await this.loadWorld(this.currentWorldFile);
          this.showToast(`已還原至快照「${s.label}」！`);
        }
      });
      row.appendChild(btn);
      box.appendChild(row);
    }
  }

  _handleSimUiSync(kind, val) {
    if (kind === "weather" || kind === "time") this._syncTimeAndWeatherUi();
    else if (kind === "mount") this._syncMountBanner(val);
  }

  _syncMountBanner(state) {
    const banner = document.getElementById("mountBanner");
    const txt = document.getElementById("mountBannerText");
    const swBtn = document.getElementById("btnMountSwitchMode");
    if (!banner || !txt) return;
    if (!state || !state.entity) {
      banner.classList.add("hidden");
      return;
    }
    const icon = this.sim._getMountIcon(state.entity, state.config);
    const modeLbl = state.controlMode === "passenger" ? "自動巡航乘客" : "親自駕駛";
    let hint = state.controlMode === "passenger"
      ? "沿既定路線巡航中・按 G 隨時下車"
      : "WASD 控制移動・按 G 下車";
    if (state.controlMode === "drive" && state.config?.mobility === "air") {
      hint = "WASD 飛行・空白鍵上升・Shift/C 下降・按 G 降落";
    } else if (state.controlMode === "drive" && state.config?.mobility === "water") {
      hint = "WASD 航行・空白鍵鳴笛・靠近岸邊按 G 下船";
    }
    txt.textContent = `${icon} ${state.entity.name}（${modeLbl}）｜${hint}`;
    if (swBtn) swBtn.classList.toggle("hidden", !state.canSwitchMode);
    banner.classList.remove("hidden");
  }

  _syncTimeAndWeatherUi() {
    this._updateTimeBadge(this.sim.timeOfDay);
    document.getElementById("timeSlider").value = this.sim.timeOfDay;
    document.getElementById("timeSpeedSelect").value = String(this.sim.timeSpeed);
    document.querySelectorAll("#weatherGroup .seg-btn").forEach((b) => {
      b.classList.toggle("active", b.dataset.weather === this.sim.weather);
    });
  }

  _updateWorldBadge(count) {
    if (!this.worldState?.meta) return;
    const m = this.worldState.meta;
    document.getElementById("worldBadge").textContent =
      `🏰 ${m.name} (${m.width}×${m.height})・${count} 物件`;
  }

  _updateTimeBadge(t) {
    const h = Math.floor(t) % 24;
    const m = Math.floor((t - Math.floor(t)) * 60);
    document.getElementById("timeLabel").textContent =
      `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
  }

  async _patchWorldMeta(fields) {
    if (!this.currentWorldFile) return;
    await fetch(`/api/worlds/${encodeURIComponent(this.currentWorldFile)}/meta`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(fields),
    });
  }

  _stepCameraKeyboard(dt) {
    const cam = this.renderer.camera;
    const pl = this.sim.player;
    if (document.querySelector(".modal-backdrop:not(.hidden)")) {
      pl.moving = false;
      return;
    }
    const k = this.keysDown;

    let fwd = 0;
    let strafe = 0;
    if (k.has("w") || k.has("arrowup")) fwd += 1;
    if (k.has("s") || k.has("arrowdown")) fwd -= 1;
    // A 往畫面左方 (-right)、D 往畫面右方 (+right)
    if (k.has("a") || k.has("arrowleft")) strafe -= 1;
    if (k.has("d") || k.has("arrowright")) strafe += 1;

    // 駕駛飛行載具時：空白鍵上升、Shift/C 鍵下降
    if (pl.mountedEntityId && pl.mountControlMode === "drive" && pl.mountConfig?.mobility === "air") {
      if (k.has(" ")) this.sim.adjustMountedAltitude(1, dt);
      if (k.has("shift") || k.has("c")) this.sim.adjustMountedAltitude(-1, dt);
    }

    if (cam.walkMode && !cam.followEntityId) {
      if (k.has("q")) cam.walkYaw -= 1.6 * dt;
      if (k.has("e")) cam.walkYaw += 1.6 * dt;

      if (fwd !== 0 || strafe !== 0) {
        const len = Math.hypot(fwd, strafe) || 1;
        const nFwd = fwd / len;
        const nStrafe = strafe / len;
        const spd = 6.2 * dt;
        const dirX = -Math.sin(cam.walkYaw);
        const dirY = -Math.cos(cam.walkYaw);
        const rightX = -Math.cos(cam.walkYaw);
        const rightY = Math.sin(cam.walkYaw);
        const mvX = (dirX * nFwd + rightX * nStrafe) * spd;
        const mvY = (dirY * nFwd + rightY * nStrafe) * spd;

        const moved = this.sim.movePlayerBy(mvX, mvY, dt);
        if (moved) {
          const nowMs = performance.now();
          if (nowMs - this._lastPlayerSaveMs > 2500) {
            this._lastPlayerSaveMs = nowMs;
            this._patchWorldMeta({ player_x: pl.x, player_y: pl.y });
          }
        }
      } else {
        pl.moving = false;
      }
      return;
    }

    // 上帝視角：Q/E 旋轉視角；WASD / 方向鍵直接操控玩家角色或所駕駛的載具並柔和跟隨鏡頭！
    if (k.has("q")) cam.yaw -= 1.6 * dt;
    if (k.has("e")) cam.yaw += 1.6 * dt;

    if (!cam.walkMode && pl.visible && (fwd !== 0 || strafe !== 0)) {
      const len = Math.hypot(fwd, strafe) || 1;
      const nFwd = fwd / len;
      const nStrafe = strafe / len;
      const spd = 6.2 * dt;
      const fx = -Math.sin(cam.yaw);
      const fy = -Math.cos(cam.yaw);
      const rx = -Math.cos(cam.yaw);
      const ry = Math.sin(cam.yaw);
      const mvX = (fx * nFwd + rx * nStrafe) * spd;
      const mvY = (fy * nFwd + ry * nStrafe) * spd;

      this.cameraFollowPlayer = true;
      const moved = this.sim.movePlayerBy(mvX, mvY, dt);
      if (moved) {
        const nowMs = performance.now();
        if (nowMs - this._lastPlayerSaveMs > 2500) {
          this._lastPlayerSaveMs = nowMs;
          this._patchWorldMeta({ player_x: pl.x, player_y: pl.y });
        }
      }
    } else if (!cam.walkMode) {
      pl.moving = false;
    }

    // 柔和將上帝視角鏡頭目標跟向移動中或搭乘中的玩家角色
    if (!cam.walkMode && (this.cameraFollowPlayer || pl.mountedEntityId) && pl.visible) {
      const gz = this.renderer.getTerrainElevationAt(pl.x, pl.y);
      const followRate = Math.min(1, dt * 6.5);
      cam.target[0] += (pl.x - cam.target[0]) * followRate;
      cam.target[1] += (pl.y - cam.target[1]) * followRate;
      cam.target[2] += (gz + (pl.z || 0) + 0.8 - cam.target[2]) * followRate;
    }
  }

  _startMainLoop() {
    let lastTs = performance.now();
    let uiTimer = 0;
    const frame = (ts) => {
      const dt = Math.min(0.05, (ts - lastTs) * 0.001);
      lastTs = ts;
      this._stepCameraKeyboard(dt);
      this.sim.step(dt);
      this.renderer.updateAndRender(dt, this.sim);

      uiTimer += dt;
      if (uiTimer > 0.25) {
        uiTimer = 0;
        this._updateTimeBadge(this.sim.timeOfDay);
        document.getElementById("timeSlider").value = this.sim.timeOfDay;
      }
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  }
}

window.addEventListener("DOMContentLoaded", () => {
  window.__stackCityApp = new StackCityApp();
});

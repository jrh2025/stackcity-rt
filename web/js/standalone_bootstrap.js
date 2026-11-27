//! 《StackCity RT（像素疊界）》單純遊戲分享獨立網頁啟動器（Pure Player Runtime）。
//!
//! 此檔案會與核心渲染／模擬／音效／任務引擎一併被封裝進單一 `.html` 檔，
//! 讓好友無需啟動伺服器（即使透過 `file://` 直接雙擊開啟）也能以玩家身分完整體驗
//! 已規劃好的 3D 微縮世界與過關任務，且完全不提供任何編輯與改圖功能。

function cloneJsonData(val) {
  return JSON.parse(JSON.stringify(val));
}

class StandalonePlayerApp {
  constructor() {
    const bundle = window.__STACKCITY_STANDALONE__;
    if (!bundle || !bundle.state) {
      throw new Error("找不到內嵌的世界遊戲資料。");
    }
    this.bundle = bundle;
    this.entryWorldId = bundle.entryWorldId || bundle.state?.meta?.world_id || "default";
    this.worldsById = new Map();
    this.worldsByFile = new Map();

    if (bundle.worlds && typeof bundle.worlds === "object") {
      for (const [wid, entry] of Object.entries(bundle.worlds)) {
        if (!entry?.state) continue;
        this.worldsById.set(wid, entry);
        if (entry.state.file_name) {
          this.worldsByFile.set(entry.state.file_name, wid);
        }
      }
    }
    if (!this.worldsById.has(this.entryWorldId)) {
      this.worldsById.set(this.entryWorldId, {
        state: bundle.state,
        bgmDataUrl: bundle.bgmDataUrl || null,
      });
      if (bundle.state.file_name) {
        this.worldsByFile.set(bundle.state.file_name, this.entryWorldId);
      }
    }

    this.currentWorldId = this.entryWorldId;
    this.worldState = cloneJsonData(bundle.state);
    this.currentWorldFile = this.worldState.meta?.file_name || "standalone.stackworld.db";
    this.assetMap = new Map();
    this.keysDown = new Set();
    this.cameraFollowPlayer = false;
    this._lastPlayerSaveMs = 0;
    this.portalReturnStack = [];
    this.sharedAdventure = null;
    this._portalBusy = false;

    this.mouseState = {
      downBtn: -1,
      downX: 0,
      downY: 0,
      lastX: 0,
      lastY: 0,
      moved: false,
      isPanning: false,
    };

    this.glCanvas = document.getElementById("glCanvas");
    this.overlayCanvas = document.getElementById("overlayCanvas");
    this.renderer = new StackCityRenderer(this.glCanvas, this.overlayCanvas);
    this.sim = new WorldSimulation(this.renderer, (kind, val) => {
      if (kind === "time") this._updateTimeBadge(this.sim.timeOfDay);
      else if (kind === "mount") this._syncMountBanner(val);
    });
    this.sim.visitorMode = true; // 鎖定為純玩家參觀／闖關模式
    this.quest = new QuestManager(this);
    this.sim.onEntityQuestInteract = (ent) => this.quest.openEncounterModal(ent);
    this.sim.onProximityCheck = (px, py, nowMs) => this.quest.checkProximityAmbush(px, py, nowMs);
    this.sim.getQuestBadge = (ent) => this.quest.getEntityBadge(ent);
    this.sim.checkPortalLock = (portal) => this._checkPortalLock(portal);
    this.sim.onPortalTrigger = (ent, portal) => this.handlePortalTransition(ent, portal);

    this._bindUi();
    this._bindCanvasControls();
    if (typeof initI18n === "function") {
      initI18n("langSelect");
      if (typeof onLangChange === "function") {
        onLangChange(() => {
          if (this.worldState?.meta) {
            const m = this.worldState.meta;
            document.getElementById("worldBadge").textContent =
              `🎮 ${m.name} (${m.width}×${m.height})`;
            this.quest.renderQuestHud();
          }
        });
      }
    }
    this._init();
  }

  _storageKey() {
    return `stackcity_standalone_proj_${this.entryWorldId}`;
  }

  _readSavedProject() {
    try {
      const raw = localStorage.getItem(this._storageKey());
      return raw ? JSON.parse(raw) : {};
    } catch {
      return {};
    }
  }

  _writeSavedProject(data) {
    try {
      localStorage.setItem(this._storageKey(), JSON.stringify(data));
    } catch { /* ignore */ }
  }

  async _init() {
    if (window.ResizeObserver) {
      const ro = new ResizeObserver(() => this.renderer.resize());
      ro.observe(document.querySelector(".viewport-wrap"));
    }
    try {
      const saved = this._readSavedProject();
      const startWorldId = saved.currentWorldId && this.worldsById.has(saved.currentWorldId)
        ? saved.currentWorldId
        : this.entryWorldId;
      if (Array.isArray(saved.returnStack)) this.portalReturnStack = saved.returnStack;
      if (saved.sharedAdventure) this.sharedAdventure = saved.sharedAdventure;
      await this._loadWorldFromBundle(startWorldId, false, {
        keepSharedAdventure: Boolean(saved.sharedAdventure),
      });
    } catch (err) {
      console.error("獨立版初始化世界失敗：", err);
      this.showToast(`載入世界發生錯誤：${err?.message || err}`, true);
    } finally {
      this._startMainLoop();
    }
  }

  async _loadWorldFromBundle(worldId, ignoreSavedProgress = false, opts = null) {
    const entry = this.worldsById.get(worldId) || this.worldsById.get(this.entryWorldId);
    if (!entry) return { portalVictoryPending: false };

    const wasWalkMode = Boolean(opts?.preserveWalkMode && this.renderer.camera.walkMode);
    const prevWalkYaw = this.renderer.camera.walkYaw;
    const prevCamYaw = this.renderer.camera.yaw;

    this.currentWorldId = worldId;
    this.worldState = cloneJsonData(entry.state);
    this.currentWorldFile = this.worldState.file_name || this.worldState.meta?.file_name || "standalone.stackworld.db";

    if (!ignoreSavedProgress) {
      const savedProj = this._readSavedProject();
      const saved = savedProj.perWorld?.[worldId];
      if (saved && this.worldState.meta) {
        if (saved.quest_progress) this.worldState.meta.quest_progress = saved.quest_progress;
        if (typeof saved.player_x === "number" && !opts?.isPortalJump) {
          this.worldState.meta.player_x = saved.player_x;
        }
        if (typeof saved.player_y === "number" && !opts?.isPortalJump) {
          this.worldState.meta.player_y = saved.player_y;
        }
        if (typeof saved.player_asset_id === "string") {
          this.worldState.meta.player_asset_id = saved.player_asset_id;
        }
      }
    }

    const data = this.worldState;
    this.assetMap.clear();
    (data.assets || []).forEach((a) => this.assetMap.set(a.id, a));

    this.renderer.rebuildTerrainMesh(data.meta.width, data.meta.height, data.terrain);
    this.renderer.resetCamera();
    this.cameraFollowPlayer = false;
    this.renderer.selectedEntityId = null;

    await Promise.all(
      (data.assets || []).map((a) => this.renderer.ensureAssetTextures(this.currentWorldFile, a)),
    );

    this.sim.loadState(data, this.assetMap);
    this.sim.visitorMode = true;
    this.quest.loadWorld(data);

    if (opts?.carryPlayerName) {
      this.sim.player.name = opts.carryPlayerName;
    }
    if (opts?.carryAvatarSha || opts?.carryAvatarName) {
      const matched =
        (data.assets || []).find((a) => opts.carryAvatarSha && a.sha256 === opts.carryAvatarSha) ||
        (data.assets || []).find((a) => opts.carryAvatarName && a.name === opts.carryAvatarName);
      if (matched) {
        this.sim.player.asset_id = matched.id;
        this.sim.player.visible = true;
        this.sim.applyPlayerScale(opts.carryPlayerScale ?? data.meta.player_scale ?? 1.0);
      }
    }

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

    // 同步標頭資訊、BGM 與時間
    const titleEl = document.getElementById("standaloneWorldTitle");
    if (titleEl) {
      titleEl.textContent = `${data.meta.name}（作者：${data.meta.author || "創作者"}）`;
    }
    document.getElementById("worldBadge").textContent =
      `🎮 ${data.meta.name} (${data.meta.width}×${data.meta.height})`;
    this._updateTimeBadge(this.sim.timeOfDay);

    const preset = data.meta.bgm_preset || "kingdom";
    const bgmDataUrl = entry.bgmDataUrl || null;
    soundEngine.configureBgm({
      preset,
      volume: data.meta.bgm_volume ?? 0.45,
      hasCustom: Boolean(bgmDataUrl),
      customUrl: bgmDataUrl,
    });

    this._renderAvatarPicker();
    return { portalVictoryPending };
  }

  showToast(msg, isError = false) {
    const stack = document.getElementById("toastStack");
    if (!stack) return;
    const div = document.createElement("div");
    div.className = `toast ${isError ? "err" : ""}`;
    div.textContent = typeof tr === "function" ? tr(msg) : msg;
    stack.appendChild(div);
    setTimeout(() => div.remove(), 3500);
  }

  syncSharedAdventureFromQuest() {
    if (this.quest) {
      this.sharedAdventure = this.quest.exportSharedAdventure();
    }
  }

  /// 純遊戲版本地進度儲存（寫入瀏覽器 localStorage，支援多世界分開紀錄與共享背包）
  async _patchWorldMeta(fields) {
    if (!this.worldState?.meta) return;
    Object.assign(this.worldState.meta, fields);
    const savedProj = this._readSavedProject();
    savedProj.currentWorldId = this.currentWorldId;
    savedProj.returnStack = this.portalReturnStack;
    if (this.sharedAdventure) savedProj.sharedAdventure = this.sharedAdventure;
    savedProj.perWorld = savedProj.perWorld || {};
    const cur = savedProj.perWorld[this.currentWorldId] || {};
    if ("quest_progress" in fields) cur.quest_progress = fields.quest_progress;
    if ("player_x" in fields) cur.player_x = fields.player_x;
    if ("player_y" in fields) cur.player_y = fields.player_y;
    if ("player_asset_id" in fields) cur.player_asset_id = fields.player_asset_id;
    savedProj.perWorld[this.currentWorldId] = cur;
    this._writeSavedProject(savedProj);
  }

  async resetGameProgress(_mode = "progress_only") {
    try {
      localStorage.removeItem(this._storageKey());
    } catch { /* ignore */ }
    this.portalReturnStack = [];
    this.sharedAdventure = null;
    await this._loadWorldFromBundle(this.entryWorldId, true, null);
    this.showToast("✨ 已重置遊玩進度並回到關卡起點，祝你挑戰成功！");
  }

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

      let targetWorldId = null;
      let spawnX = typeof portal.target_x === "number" ? portal.target_x : null;
      let spawnY = typeof portal.target_y === "number" ? portal.target_y : null;
      let spawnYaw = typeof portal.target_yaw === "number" ? portal.target_yaw : undefined;
      let targetPortalId = portal.target_portal_id || null;

      if (portal.kind === "exit" && this.portalReturnStack.length > 0) {
        const ret = this.portalReturnStack.pop();
        if (ret.worldId && this.worldsById.has(ret.worldId)) {
          targetWorldId = ret.worldId;
        } else if (ret.fileName && this.worldsByFile.has(ret.fileName)) {
          targetWorldId = this.worldsByFile.get(ret.fileName);
        }
        if (typeof ret.x === "number") spawnX = ret.x;
        if (typeof ret.y === "number") spawnY = ret.y;
        if (typeof ret.yaw === "number") spawnYaw = ret.yaw;
      } else {
        if (portal.target_world_id && this.worldsById.has(portal.target_world_id)) {
          targetWorldId = portal.target_world_id;
        } else if (portal.target_world_file && this.worldsByFile.has(portal.target_world_file)) {
          targetWorldId = this.worldsByFile.get(portal.target_world_file);
        }
        if (targetWorldId && portal.kind !== "exit") {
          const doorPos = this.sim.getPortalWorldPos(ent, portal) || { x: this.sim.player.x, y: this.sim.player.y };
          const yawRad = ((ent.yaw || 0) * Math.PI) / 180;
          const pushX = doorPos.x + (-Math.sin(yawRad)) * 0.45;
          const pushY = doorPos.y + Math.cos(yawRad) * 0.45;
          const [safeRetX, safeRetY] = this.sim._findSafeLandPoint(pushX, pushY, false);
          this.portalReturnStack.push({
            worldId: this.currentWorldId,
            fileName: this.currentWorldFile,
            x: safeRetX,
            y: safeRetY,
            yaw: ((ent.yaw || 0) + 180) % 360,
          });
        }
      }

      if (!targetWorldId || !this.worldsById.has(targetWorldId)) {
        return this.showToast("此門目前尚未連結到可進入的世界！", true);
      }

      const targetEntry = this.worldsById.get(targetWorldId);
      if (overlayText) {
        overlayText.textContent = portal.kind === "exit"
          ? `🚪 正在返回「${targetEntry?.state?.meta?.name || "原世界"}」...`
          : `🚪 正在進入「${targetEntry?.state?.meta?.name || "目標世界"}」...`;
      }
      overlay?.classList.add("active");
      await new Promise((r) => setTimeout(r, 210));

      const loadRes = await this._loadWorldFromBundle(targetWorldId, false, {
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

      const savedProj = this._readSavedProject();
      savedProj.currentWorldId = this.currentWorldId;
      savedProj.returnStack = this.portalReturnStack;
      if (this.sharedAdventure) savedProj.sharedAdventure = this.sharedAdventure;
      this._writeSavedProject(savedProj);

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

  // 以下提供空實作／本地化對應方法以完整相容 QuestManager 呼叫
  async updateEntityFields(entityId, patch) {
    const ent = this.sim.entities.find((e) => e.id === entityId);
    if (ent) Object.assign(ent, patch);
  }
  renderRightPanel() { /* 純玩家模式無右側編輯面板 */ }

  _updateTimeBadge(t) {
    const el = document.getElementById("timeLabel");
    if (!el) return;
    const h = Math.floor(t) % 24;
    const m = Math.floor((t - Math.floor(t)) * 60);
    el.textContent = `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
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

  _bindUi() {
    // 初次互動即解鎖音效引擎與 BGM
    const unlockAudio = () => {
      soundEngine.unlockAudio?.();
      window.removeEventListener("pointerdown", unlockAudio);
      window.removeEventListener("keydown", unlockAudio);
    };
    window.addEventListener("pointerdown", unlockAudio);
    window.addEventListener("keydown", unlockAudio);

    document.querySelectorAll("[data-close-modal]").forEach((btn) => {
      btn.addEventListener("click", () => {
        document.getElementById(btn.dataset.closeModal)?.classList.add("hidden");
      });
    });

    const btnPlayPause = document.getElementById("btnPlayPause");
    if (btnPlayPause) {
      btnPlayPause.textContent = "⏸️";
      btnPlayPause.addEventListener("click", () => {
        this.sim.paused = !this.sim.paused;
        btnPlayPause.textContent = this.sim.paused ? "▶️" : "⏸️";
      });
    }

    document.getElementById("btnMute")?.addEventListener("click", (e) => {
      const muted = soundEngine.toggleMute();
      e.currentTarget.textContent = muted ? "🔇" : "🔊";
      this.showToast(muted ? "已靜音遊戲聲音" : "已開啟遊戲聲音");
    });

    document.getElementById("btnCityWalk")?.addEventListener("click", () => this._toggleWalkMode());
    document.getElementById("btnExitWalk")?.addEventListener("click", () => this._toggleWalkMode(false));
    document.getElementById("btnMountSwitchMode")?.addEventListener("click", () => this.sim.toggleMountControlMode());
    document.getElementById("btnMountDismount")?.addEventListener("click", () => this.sim.dismountPlayer(false));

    document.getElementById("btnPlayerAvatar")?.addEventListener("click", () => {
      this._renderAvatarPicker();
      document.getElementById("standaloneAvatarModal")?.classList.remove("hidden");
    });
    document.getElementById("btnWalkChangeAvatar")?.addEventListener("click", () => {
      this._renderAvatarPicker();
      document.getElementById("standaloneAvatarModal")?.classList.remove("hidden");
    });

    document.getElementById("btnResetProgress")?.addEventListener("click", () => {
      this.resetGameProgress("progress_only");
    });

    document.getElementById("btnFullscreen")?.addEventListener("click", () => {
      if (!document.fullscreenElement) {
        document.documentElement.requestFullscreen?.().catch(() => {});
      } else {
        document.exitFullscreen?.().catch(() => {});
      }
    });

    document.getElementById("btnResetCam")?.addEventListener("click", () => {
      this.cameraFollowPlayer = false;
      this.renderer.resetCamera();
    });
    document.getElementById("btnRotateLeft")?.addEventListener("click", () => {
      const cam = this.renderer.camera;
      if (cam.walkMode) cam.walkYaw -= Math.PI / 4;
      else cam.yaw -= Math.PI / 4;
    });
    document.getElementById("btnRotateRight")?.addEventListener("click", () => {
      const cam = this.renderer.camera;
      if (cam.walkMode) cam.walkYaw += Math.PI / 4;
      else cam.yaw += Math.PI / 4;
    });
  }

  _toggleWalkMode(force) {
    const cam = this.renderer.camera;
    const target = typeof force === "boolean" ? force : !cam.walkMode;
    cam.walkMode = target;
    cam.followEntityId = null;
    if (target) {
      this.sim.player.visible = true;
      cam.walkYaw = this.sim.player.yaw || 0;
      this.showToast("已進入街道漫步視角！WASD 移動，F 鍵或點擊物件互動");
    } else {
      this.cameraFollowPlayer = true;
      this.showToast("已切換回上帝微縮視角（仍可用 WASD 操控角色）");
    }
    document.getElementById("btnCityWalk")?.classList.toggle("active", cam.walkMode);
    document.getElementById("cityWalkBanner")?.classList.toggle("hidden", !cam.walkMode);
  }

  _renderAvatarPicker() {
    const box = document.getElementById("standaloneAvatarGrid");
    if (!box) return;
    box.innerHTML = "";
    const curId = this.sim.player.asset_id;
    // 比照主程式排序，優先呈現人物／動物／載具方便玩家挑選
    const order = { character: 0, animal: 1, vehicle: 2, prop: 3, plant: 4, building: 5 };
    const list = [...(this.worldState?.assets || [])].sort(
      (a, b) => (order[a.category] ?? 9) - (order[b.category] ?? 9),
    );
    for (const a of list) {
      const card = document.createElement("button");
      card.type = "button";
      card.className = `player-asset-card ${a.id === curId ? "selected" : ""}`;
      const entry = this.renderer.assetTextures.get(a.id);
      if (entry?.thumbCanvas) {
        const thumb = document.createElement("canvas");
        thumb.width = entry.thumbCanvas.width;
        thumb.height = entry.thumbCanvas.height;
        thumb.getContext("2d").drawImage(entry.thumbCanvas, 0, 0);
        card.appendChild(thumb);
      }
      const lbl = document.createElement("span");
      lbl.textContent = a.name;
      card.appendChild(lbl);
      card.addEventListener("click", () => this.selectPlayerAsset(a.id));
      box.appendChild(card);
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
    this._renderAvatarPicker();
    await this._patchWorldMeta({ player_asset_id: assetId });
    document.getElementById("standaloneAvatarModal")?.classList.add("hidden");
    this.showToast(`玩家造型已更換為「${assetName}」！`);
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
      const isPanAction = e.button === 1 || (e.button === 0 && !cam.walkMode && this.keysDown.has(" "));
      this.mouseState.isPanning = isPanAction;
      if (isPanAction) {
        cv.style.cursor = "grabbing";
        return;
      }

      if (e.button === 0 && !cam.walkMode) {
        const hit = this.renderer.pickScene(e.clientX, e.clientY, this.sim.entities, this.assetMap);
        if (hit.entity) this.sim.interactWithEntity(hit.entity.id, true);
      }
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
          cv.style.cursor = hit.entity ? "pointer" : "default";
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
      }
    });

    window.addEventListener("mouseup", (e) => {
      const cam = this.renderer.camera;
      if (this.mouseState.downBtn === 0 && cam.walkMode && !this.mouseState.moved) {
        const hit = this.renderer.pickScene(e.clientX, e.clientY, this.sim.entities, this.assetMap);
        if (hit.entity) this.sim.interactWithEntity(hit.entity.id, true);
      }
      this.mouseState.downBtn = -1;
      this.mouseState.isPanning = false;
      cv.style.cursor = "default";
    });

    cv.addEventListener("wheel", (e) => {
      e.preventDefault();
      const maxD = this.renderer.getMaxZoomDist();
      this.renderer.camera.dist = Math.max(4.0, Math.min(maxD, this.renderer.camera.dist * (1 + e.deltaY * 0.001)));
    }, { passive: false });

    window.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        ["encounterModal", "victoryModal", "standaloneAvatarModal"].forEach((id) => {
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
    });
    window.addEventListener("keyup", (e) => {
      this.keysDown.delete(e.key.toLowerCase());
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
        const moved = this.sim.movePlayerBy((dirX * nFwd + rightX * nStrafe) * spd, (dirY * nFwd + rightY * nStrafe) * spd, dt);
        if (moved && performance.now() - this._lastPlayerSaveMs > 2500) {
          this._lastPlayerSaveMs = performance.now();
          this._patchWorldMeta({ player_x: pl.x, player_y: pl.y });
        }
      } else {
        pl.moving = false;
      }
      return;
    }

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
      this.cameraFollowPlayer = true;
      const moved = this.sim.movePlayerBy((fx * nFwd + rx * nStrafe) * spd, (fy * nFwd + ry * nStrafe) * spd, dt);
      if (moved && performance.now() - this._lastPlayerSaveMs > 2500) {
        this._lastPlayerSaveMs = performance.now();
        this._patchWorldMeta({ player_x: pl.x, player_y: pl.y });
      }
    } else if (!cam.walkMode) {
      pl.moving = false;
    }

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
      }
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  }
}

function bootStandalonePlayer() {
  window.addEventListener("DOMContentLoaded", () => {
    window.__stackCityStandaloneApp = new StandalonePlayerApp();
  });
}

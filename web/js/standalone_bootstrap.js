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
    this.initialState = cloneJsonData(bundle.state);
    this.worldState = cloneJsonData(bundle.state);
    this.currentWorldFile = this.worldState.meta?.file_name || "standalone.stackworld.db";
    this.assetMap = new Map();
    this.keysDown = new Set();
    this.cameraFollowPlayer = false;
    this._lastPlayerSaveMs = 0;

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
    this.sim = new WorldSimulation(this.renderer, (kind) => {
      if (kind === "time") this._updateTimeBadge(this.sim.timeOfDay);
    });
    this.sim.visitorMode = true; // 鎖定為純玩家參觀／闖關模式
    this.quest = new QuestManager(this);
    this.sim.onEntityQuestInteract = (ent) => this.quest.openEncounterModal(ent);
    this.sim.onProximityCheck = (px, py, nowMs) => this.quest.checkProximityAmbush(px, py, nowMs);
    this.sim.getQuestBadge = (ent) => this.quest.getEntityBadge(ent);

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
    const wid = this.worldState?.meta?.world_id || "default";
    return `stackcity_standalone_prog_${wid}`;
  }

  async _init() {
    if (window.ResizeObserver) {
      const ro = new ResizeObserver(() => this.renderer.resize());
      ro.observe(document.querySelector(".viewport-wrap"));
    }
    await this._loadFromBundle(false);
    this._startMainLoop();
  }

  async _loadFromBundle(ignoreSavedProgress = false) {
    this.worldState = cloneJsonData(this.initialState);
    if (!ignoreSavedProgress) {
      try {
        const raw = localStorage.getItem(this._storageKey());
        if (raw) {
          const saved = JSON.parse(raw);
          if (saved.quest_progress && this.worldState.meta) {
            this.worldState.meta.quest_progress = saved.quest_progress;
          }
          if (typeof saved.player_x === "number" && this.worldState.meta) {
            this.worldState.meta.player_x = saved.player_x;
          }
          if (typeof saved.player_y === "number" && this.worldState.meta) {
            this.worldState.meta.player_y = saved.player_y;
          }
          if (typeof saved.player_asset_id === "string" && this.worldState.meta) {
            this.worldState.meta.player_asset_id = saved.player_asset_id;
          }
        }
      } catch { /* 忽略 localStorage 讀取例外（例如無痕模式） */ }
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

    // 同步標頭資訊、BGM 與時間
    const titleEl = document.getElementById("standaloneWorldTitle");
    if (titleEl) {
      titleEl.textContent = `${data.meta.name}（作者：${data.meta.author || "創作者"}）`;
    }
    document.getElementById("worldBadge").textContent =
      `🎮 ${data.meta.name} (${data.meta.width}×${data.meta.height})`;
    this._updateTimeBadge(this.sim.timeOfDay);

    const preset = data.meta.bgm_preset || "kingdom";
    const bgmDataUrl = this.bundle.bgmDataUrl || null;
    soundEngine.configureBgm({
      preset,
      volume: data.meta.bgm_volume ?? 0.45,
      hasCustom: Boolean(bgmDataUrl),
      customUrl: bgmDataUrl,
    });

    // 若使用者未曾設過自訂造型視窗，預先建好造型清單
    this._renderAvatarPicker();
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

  /// 純遊戲版本地進度儲存（寫入瀏覽器 localStorage，不存取任何編輯伺服器 API）
  async _patchWorldMeta(fields) {
    if (!this.worldState?.meta) return;
    Object.assign(this.worldState.meta, fields);
    try {
      const prevRaw = localStorage.getItem(this._storageKey());
      const saved = prevRaw ? JSON.parse(prevRaw) : {};
      if ("quest_progress" in fields) saved.quest_progress = fields.quest_progress;
      if ("player_x" in fields) saved.player_x = fields.player_x;
      if ("player_y" in fields) saved.player_y = fields.player_y;
      if ("player_asset_id" in fields) saved.player_asset_id = fields.player_asset_id;
      localStorage.setItem(this._storageKey(), JSON.stringify(saved));
    } catch { /* ignore */ }
  }

  async resetGameProgress(_mode = "progress_only") {
    try {
      localStorage.removeItem(this._storageKey());
    } catch { /* ignore */ }
    await this._loadFromBundle(true);
    this.showToast("✨ 已重置遊玩進度並回到關卡起點，祝你挑戰成功！");
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
      if (e.key === " " && this.sim.player.visible && !this.mouseState.isPanning) {
        e.preventDefault();
        this.sim.jumpPlayer();
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

    if (!cam.walkMode && this.cameraFollowPlayer && pl.visible) {
      const gz = this.renderer.getTerrainElevationAt(pl.x, pl.y);
      const followRate = Math.min(1, dt * 6.5);
      cam.target[0] += (pl.x - cam.target[0]) * followRate;
      cam.target[1] += (pl.y - cam.target[1]) * followRate;
      cam.target[2] += (gz + 0.8 - cam.target[2]) * followRate;
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

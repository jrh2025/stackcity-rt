//! 終極任務關卡過關引擎、NPC／物件多元互動考驗（線索／選擇題／搶劫／支援／寶藏／嫌疑犯指認）與規劃器 UI。

import { soundEngine } from "./audio.js";
import { escapeHtml } from "./studio.js";

export const INTERACTION_META = {
  none: { icon: "", label: "無任務互動", shortBadge: "", actionLabel: "互動", badgeBg: "#475569" },
  clue: { icon: "💬", label: "線索／情報 NPC", shortBadge: "線索", actionLabel: "打聽線索", badgeBg: "rgba(14, 165, 233, 0.9)" },
  quiz: { icon: "❓", label: "選擇題考官", shortBadge: "考驗", actionLabel: "接受考驗", badgeBg: "rgba(168, 85, 247, 0.92)" },
  robbery: { icon: "⚠️", label: "搶劫／攔路反派", shortBadge: "劫匪", actionLabel: "應對攔截", badgeBg: "rgba(220, 38, 38, 0.92)" },
  support: { icon: "🛡️", label: "夥伴支援補給", shortBadge: "支援", actionLabel: "領取支援", badgeBg: "rgba(16, 185, 129, 0.92)" },
  treasure: { icon: "💎", label: "寶藏／關鍵物件", shortBadge: "寶藏", actionLabel: "開啟寶藏", badgeBg: "rgba(217, 119, 6, 0.92)" },
  suspect: { icon: "🕵️", label: "案件嫌疑犯", shortBadge: "嫌疑人", actionLabel: "盤查指認", badgeBg: "rgba(234, 88, 12, 0.92)" },
};

export const GOAL_TYPE_LABELS = {
  score: "🏆 積分達標制（執行任務／考驗累積積分）",
  catch_culprit: "🚨 抓出真兇制（蒐集線索後從嫌疑犯中逮捕歹徒）",
  find_treasure: "💎 尋寶探索制（尋找指定數量的寶藏）",
  solve_puzzles: "❓ 知識解謎制（通過指定數量的難題考驗）",
  custom_checklist: "📋 綜合清單制（完成規畫者自訂的多項條件）",
};

/// 安全解析實體 meta_json 中的任務互動設定
export function parseEntityInteraction(entity) {
  if (!entity || !entity.meta_json) return { kind: "none" };
  try {
    const root = typeof entity.meta_json === "string" ? JSON.parse(entity.meta_json) : entity.meta_json;
    const inter = root && typeof root === "object" ? root.interaction : null;
    if (!inter || !inter.kind || inter.kind === "none") return { kind: "none" };
    return inter;
  } catch {
    return { kind: "none" };
  }
}

/// 將更新後的 interaction 寫回實體的 meta_json（保留其他未知中繼欄位）
export function buildEntityMetaWithInteraction(entity, interaction) {
  let root = {};
  try {
    const parsed = typeof entity?.meta_json === "string" ? JSON.parse(entity.meta_json || "{}") : {};
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) root = parsed;
  } catch {
    root = {};
  }
  if (!interaction || interaction.kind === "none") {
    delete root.interaction;
  } else {
    root.interaction = interaction;
  }
  return JSON.stringify(root);
}

function defaultQuestConfig() {
  return {
    enabled: false,
    title: "冒險任務關卡",
    goal_type: "score",
    target_count: 100,
    culprit_entity_id: "",
    checklist: [],
    description: "與場景中的 NPC 和物件互動、解決難題並取得高分即可過關！",
    victory_title: "🎉 恭喜完成關卡目標！",
    victory_message: "太出色了！你已順利完成這座世界的所有挑戰！",
  };
}

function defaultQuestProgress() {
  return {
    score: 0,
    hp: 5,
    max_hp: 5,
    shield: 0,
    clues: [], // { sourceId, sourceName, text }
    items: [],
    flags: {},
    treasures_found: 0,
    puzzles_solved: 0,
    resolved_entities: [],
    culprit_caught: false,
    wrong_accusations: 0,
    completed: false,
  };
}

export class QuestManager {
  constructor(app) {
    this.app = app;
    this.config = defaultQuestConfig();
    this.progress = defaultQuestProgress();
    this.activeEncounterEntityId = null;
    this.ambushCooldowns = new Map();
    this.hudCollapsed = false;
    this.notesOpen = false;
    this._saveTimer = null;

    this._bindModalEvents();
  }

  _bindModalEvents() {
    document.getElementById("btnVicContinue")?.addEventListener("click", () => {
      document.getElementById("victoryModal")?.classList.add("hidden");
    });
    document.getElementById("btnVicReplay")?.addEventListener("click", async () => {
      document.getElementById("victoryModal")?.classList.add("hidden");
      await this.app.resetGameProgress("progress_only");
    });
    document.querySelector('[data-close-modal="encounterModal"]')?.addEventListener("click", () => {
      if (this._victoryTimer) {
        clearTimeout(this._victoryTimer);
        this._victoryTimer = null;
      }
      this.activeEncounterEntityId = null;
      if (this.progress.completed) {
        this.triggerVictory();
      }
    });
  }

  /// 載入世界時同步讀取 quest_config 與 quest_progress
  loadWorld(worldData) {
    const rawCfg = worldData?.meta?.quest_config;
    const rawProg = worldData?.meta?.quest_progress;
    this.config = rawCfg && typeof rawCfg === "object"
      ? { ...defaultQuestConfig(), ...rawCfg }
      : defaultQuestConfig();
    this.progress = rawProg && typeof rawProg === "object"
      ? {
          ...defaultQuestProgress(),
          ...rawProg,
          clues: Array.isArray(rawProg.clues) ? rawProg.clues : [],
          items: Array.isArray(rawProg.items) ? rawProg.items : [],
          flags: rawProg.flags && typeof rawProg.flags === "object" ? rawProg.flags : {},
          resolved_entities: Array.isArray(rawProg.resolved_entities) ? rawProg.resolved_entities : [],
        }
      : defaultQuestProgress();

    this.activeEncounterEntityId = null;
    this.ambushCooldowns.clear();
    this.notesOpen = false;
    document.getElementById("encounterModal")?.classList.add("hidden");
    document.getElementById("victoryModal")?.classList.add("hidden");
    this.renderQuestHud();
  }

  /// 排程寫回任務進度至 SQLite world_meta（或獨立純遊戲版之 localStorage）
  async persistProgress(immediate = false) {
    if (!this.app.currentWorldFile && !window.__STACKCITY_STANDALONE__) return;
    if (this._saveTimer) {
      clearTimeout(this._saveTimer);
      this._saveTimer = null;
    }
    const doSave = async () => {
      if (this.app.worldState?.meta) {
        this.app.worldState.meta.quest_progress = JSON.parse(JSON.stringify(this.progress));
      }
      this.app.syncSharedAdventureFromQuest?.();
      await this.app._patchWorldMeta({ quest_progress: this.progress });
    };
    if (immediate) {
      await doSave();
    } else {
      this._saveTimer = setTimeout(doSave, 180);
    }
  }

  /// 儲存規劃者修改之過關目標設定至 SQLite
  async saveConfig(nextCfg = this.config) {
    this.config = { ...defaultQuestConfig(), ...nextCfg };
    if (this.app.worldState?.meta) {
      this.app.worldState.meta.quest_config = JSON.parse(JSON.stringify(this.config));
    }
    await this.app._patchWorldMeta({ quest_config: this.config });
    this.renderQuestHud();
  }

  /// 供 2D Overlay 繪製物件頭頂任務徽章與 [F] 提示字樣
  getEntityBadge(entity) {
    const inter = parseEntityInteraction(entity);
    if (inter.kind === "none") return null;
    const meta = INTERACTION_META[inter.kind] || INTERACTION_META.clue;
    const isResolved = this.progress.resolved_entities.includes(entity.id);

    if (inter.kind === "suspect") {
      const isCulprit = Boolean(inter.is_culprit) || (this.config.culprit_entity_id && this.config.culprit_entity_id === entity.id);
      if (this.progress.culprit_caught && isCulprit) {
        return { text: "🚨 已落網", bg: "rgba(34, 197, 94, 0.9)", color: "#fff", actionLabel: meta.actionLabel };
      }
      const questioned = this.progress.clues.some((c) => c.sourceId === entity.id);
      return {
        text: questioned ? "🕵️ 已盤問" : `${meta.icon} ${meta.shortBadge}`,
        bg: questioned ? "rgba(100, 116, 139, 0.9)" : meta.badgeBg,
        color: "#fff",
        actionLabel: meta.actionLabel,
      };
    }

    if (isResolved && inter.once !== false) {
      return {
        text: `✅ 已完成`,
        bg: "rgba(22, 163, 74, 0.85)",
        color: "#ecfdf5",
        actionLabel: meta.actionLabel,
      };
    }

    return {
      text: `${meta.icon} ${meta.shortBadge}`,
      bg: meta.badgeBg,
      color: "#ffffff",
      actionLabel: `${meta.icon} ${meta.actionLabel}`,
    };
  }

  /// 偵測玩家靠近「搶劫／埋伏」類型角色時自動觸發遭遇事件
  checkProximityAmbush(px, py, nowMs) {
    if (!this.config.enabled || this.progress.completed || !this.app.sim.player.visible) return;
    // 若畫面上已有任何彈窗開啟則不重複觸發
    if (document.querySelector(".modal-backdrop:not(.hidden)")) return;

    for (const ent of this.app.sim.entities) {
      if (!ent.visible) continue;
      if (this.progress.resolved_entities.includes(ent.id)) continue;
      const inter = parseEntityInteraction(ent);
      if (inter.kind !== "robbery") continue;
      const rad = Number(inter.auto_trigger_radius || 0);
      if (rad <= 0) continue;

      const dist = Math.hypot(ent.x - px, ent.y - py);
      if (dist > rad) continue;

      const lastMs = this.ambushCooldowns.get(ent.id) || 0;
      if (nowMs - lastMs < 12000) continue;
      this.ambushCooldowns.set(ent.id, nowMs);

      soundEngine.play("alarm");
      this.openEncounterModal(ent);
      break;
    }
  }

  /// 計算目前目標進度字串與百分比
  getProgressSummary() {
    const cfg = this.config;
    const p = this.progress;
    const g = cfg.goal_type || "score";

    if (g === "score") {
      const target = Math.max(1, Number(cfg.target_count || 100));
      const pct = Math.min(100, Math.round((p.score / target) * 100));
      return { current: p.score, target, unit: "分", pct, text: `${p.score} / ${target} 分` };
    }
    if (g === "find_treasure") {
      const target = Math.max(1, Number(cfg.target_count || 1));
      const pct = Math.min(100, Math.round((p.treasures_found / target) * 100));
      return { current: p.treasures_found, target, unit: "個寶藏", pct, text: `寶藏 ${p.treasures_found} / ${target}` };
    }
    if (g === "solve_puzzles") {
      const target = Math.max(1, Number(cfg.target_count || 1));
      const pct = Math.min(100, Math.round((p.puzzles_solved / target) * 100));
      return { current: p.puzzles_solved, target, unit: "題解謎", pct, text: `解謎 ${p.puzzles_solved} / ${target}` };
    }
    if (g === "catch_culprit") {
      const cur = p.culprit_caught ? 1 : 0;
      return { current: cur, target: 1, unit: "位真兇", pct: cur ? 100 : 0, text: p.culprit_caught ? "真兇已歸案 (1/1)" : "鎖定並指認真兇 (0/1)" };
    }
    // custom_checklist
    const list = Array.isArray(cfg.checklist) ? cfg.checklist : [];
    const target = Math.max(1, list.length);
    const done = list.filter((it) => Boolean(p.flags?.[it.key])).length;
    const pct = Math.min(100, Math.round((done / target) * 100));
    return { current: done, target, unit: "項清單", pct, text: `清單 ${done} / ${target}` };
  }

  /// 檢查是否剛達成過關條件並啟動慶賀演出
  checkVictory(triggerEffect = true) {
    if (!this.config.enabled) return false;
    if (this.progress.completed) return false;

    const sum = this.getProgressSummary();
    let achieved = false;
    if (this.config.goal_type === "catch_culprit") {
      achieved = Boolean(this.progress.culprit_caught);
    } else if (this.config.goal_type === "custom_checklist") {
      achieved = sum.target > 0 && sum.current >= sum.target;
    } else {
      achieved = sum.current >= sum.target;
    }

    if (achieved) {
      this.progress.completed = true;
      this.persistProgress(true);
      this.renderQuestHud();
      if (triggerEffect) {
        this.triggerVictory();
      }
      return true;
    }
    return false;
  }

  /// 觸發全螢幕煙火、櫻花雨、凱旋號角與過關慶賀彈窗
  triggerVictory() {
    document.getElementById("encounterModal")?.classList.add("hidden");
    this.activeEncounterEntityId = null;

    const pl = this.app.sim.player;
    const gx = pl?.visible ? pl.x : (this.app.renderer.worldWidth || 40) * 0.5;
    const gy = pl?.visible ? pl.y : (this.app.renderer.worldHeight || 40) * 0.5;
    const gz = this.app.renderer.getTerrainElevationAt(gx, gy);

    soundEngine.play("victory");
    this.app.renderer.spawnParticles("firework", gx, gy, gz + 3.2, 56);
    this.app.renderer.spawnParticles("sakura", gx, gy, gz + 4.0, 42);
    setTimeout(() => {
      this.app.renderer.spawnParticles("firework", gx + 2.5, gy - 2.0, gz + 4.2, 48);
    }, 350);

    const titleEl = document.getElementById("victoryTitle");
    const msgEl = document.getElementById("victoryMessage");
    const statsEl = document.getElementById("victoryStats");
    if (titleEl) titleEl.textContent = this.config.victory_title || "🎉 恭喜完成關卡目標！";
    if (msgEl) msgEl.textContent = this.config.victory_message || "你已圓滿達成所有挑戰！";
    if (statsEl) {
      const p = this.progress;
      statsEl.innerHTML = `
        <div class="vic-stat"><span>最終積分</span><strong>${p.score} 分</strong></div>
        <div class="vic-stat"><span>蒐集線索</span><strong>${p.clues.length} 條</strong></div>
        <div class="vic-stat"><span>尋獲寶藏</span><strong>${p.treasures_found} 個</strong></div>
        <div class="vic-stat"><span>通過考驗</span><strong>${p.puzzles_solved} 題</strong></div>`;
    }
    document.getElementById("victoryModal")?.classList.remove("hidden");
  }

  _addClue(sourceId, sourceName, text) {
    if (!text) return;
    if (!this.progress.clues.some((c) => c.sourceId === sourceId && c.text === text)) {
      this.progress.clues.push({ sourceId, sourceName, text });
    }
  }

  _addItem(itemName) {
    const clean = (itemName || "").trim();
    if (!clean) return false;
    if (!this.progress.items.includes(clean)) {
      this.progress.items.push(clean);
      return true;
    }
    return false;
  }

  hasItem(name) {
    const clean = (name || "").trim();
    if (!clean) return true;
    return (this.progress.items || []).includes(clean);
  }

  hasFlag(key) {
    const clean = (key || "").trim();
    if (!clean) return true;
    return Boolean(this.progress.flags?.[clean]);
  }

  /// 匯出可供跨世界旅行攜帶的冒險進度（積分／血量護盾／背包道具／線索／旗標／累計探索數）
  exportSharedAdventure() {
    const p = this.progress;
    return {
      score: Number(p.score || 0),
      hp: Number(p.hp ?? 5),
      max_hp: Number(p.max_hp ?? 5),
      shield: Number(p.shield || 0),
      items: [...(p.items || [])],
      clues: (p.clues || []).map((c) => ({ ...c })),
      flags: { ...(p.flags || {}) },
      treasures_found: Number(p.treasures_found || 0),
      puzzles_solved: Number(p.puzzles_solved || 0),
    };
  }

  /// 進入新世界或回到原世界後，將跨世界共享進度合併套用並檢查是否達成本世界過關條件
  applySharedAdventure(shared) {
    if (!shared || typeof shared !== "object") return { newlyCompleted: false };
    const p = this.progress;
    p.score = Math.max(0, Number(shared.score ?? p.score ?? 0));
    p.max_hp = Math.max(1, Number(shared.max_hp ?? p.max_hp ?? 5));
    p.hp = Math.max(0, Math.min(p.max_hp, Number(shared.hp ?? p.hp ?? p.max_hp)));
    p.shield = Math.max(0, Number(shared.shield ?? p.shield ?? 0));
    p.treasures_found = Math.max(0, Number(shared.treasures_found ?? p.treasures_found ?? 0));
    p.puzzles_solved = Math.max(0, Number(shared.puzzles_solved ?? p.puzzles_solved ?? 0));

    for (const it of shared.items || []) {
      this._addItem(it);
    }
    for (const cl of shared.clues || []) {
      this._addClue(cl.sourceId, cl.sourceName, cl.text);
    }
    p.flags = { ...(p.flags || {}), ...(shared.flags || {}) };

    this.renderQuestHud();
    if (this.app.worldState?.meta) {
      this.app.worldState.meta.quest_progress = JSON.parse(JSON.stringify(this.progress));
    }
    this.app._patchWorldMeta?.({ quest_progress: this.progress });

    const wasCompleted = Boolean(p.completed);
    const newlyCompleted = !wasCompleted && this.checkVictory(false);
    return { newlyCompleted };
  }

  _grantCommonRewards(entity, inter) {
    const rewards = [];
    const pts = Number(inter.reward_points || 0);
    if (pts !== 0) {
      this.progress.score = Math.max(0, this.progress.score + pts);
      rewards.push(`${pts > 0 ? "+" : ""}${pts} 積分`);
    }
    if (inter.reward_item) {
      const added = this._addItem(inter.reward_item);
      if (added) rewards.push(`🎒 獲得「${inter.reward_item}」`);
    }
    if (inter.reward_flag) {
      this.progress.flags[inter.reward_flag] = true;
      rewards.push(`📌 達成指標`);
    }
    if (entity && !this.progress.resolved_entities.includes(entity.id)) {
      this.progress.resolved_entities.push(entity.id);
    }
    return rewards;
  }

  /// 開啟 NPC／物件任務互動考驗彈窗；若物件無任務設定則回傳 false
  openEncounterModal(entity) {
    const inter = parseEntityInteraction(entity);
    if (inter.kind === "none") return false;

    this.app.keysDown.clear();
    this.activeEncounterEntityId = entity.id;

    const modal = document.getElementById("encounterModal");
    const headerIcon = document.getElementById("encIcon");
    const headerTitle = document.getElementById("encTitle");
    const headerSub = document.getElementById("encSubtitle");
    const body = document.getElementById("encBody");
    const metaInfo = INTERACTION_META[inter.kind] || INTERACTION_META.clue;
    const baseZ = this.app.renderer.getTerrainElevationAt(entity.x, entity.y) + (entity.z || 0);

    headerIcon.textContent = metaInfo.icon || "💬";
    headerTitle.textContent = inter.title || entity.name;
    headerSub.textContent = `來源物件：${entity.name}（${metaInfo.label}）`;

    const alreadyDone = this.progress.resolved_entities.includes(entity.id) && inter.once !== false;
    let feedbackHtml = "";
    let controlsHtml = "";
    let autoWon = false;

    // 共用頭像＆目前玩家狀態條
    const statusBar = `
      <div class="enc-status-strip">
        <span>🏆 積分：<strong>${this.progress.score}</strong></span>
        <span>❤️ 體力：<strong>${this.progress.hp}/${this.progress.max_hp}</strong></span>
        <span>🛡️ 護盾：<strong>${this.progress.shield}</strong></span>
      </div>`;

    if (inter.kind === "clue") {
      if (!alreadyDone) {
        const clueStr = inter.clue_text || inter.dialogue || "";
        this._addClue(entity.id, entity.name, clueStr);
        const rw = this._grantCommonRewards(entity, inter);
        soundEngine.play("chime");
        this.app.renderer.spawnParticles("sparkle", entity.x, entity.y, baseZ + 1.4, 20);
        this.persistProgress();
        this.renderQuestHud();
        feedbackHtml = `
          <div class="enc-reward-box">
            <strong>✅ 已記錄線索至冒險筆記！</strong>
            ${rw.length ? `<div class="enc-tags">${rw.map((x) => `<span class="enc-tag">${escapeHtml(x)}</span>`).join("")}</div>` : ""}
          </div>`;
        autoWon = this.checkVictory(false);
      } else {
        feedbackHtml = `<div class="enc-note-box">📝 此線索已抄錄至您的筆記中，可隨時查看右上方「📋 筆記」。</div>`;
      }
      controlsHtml = `<button id="btnEncCloseOk" class="btn btn-primary" style="width:100%;">${autoWon ? "🎉 達成過關目標！前往慶賀！" : "👍 知道了，繼續冒險"}</button>`;
    } else if (inter.kind === "support") {
      if (!alreadyDone) {
        const sh = Math.max(0, Number(inter.grant_shield || 0));
        const heal = Math.max(0, Number(inter.heal_hp || 0));
        this.progress.shield += sh;
        this.progress.hp = Math.min(this.progress.max_hp, this.progress.hp + heal);
        if (inter.clue_text) this._addClue(entity.id, entity.name, inter.clue_text);
        const rw = this._grantCommonRewards(entity, inter);
        if (sh > 0) rw.unshift(`🛡️ 護盾 +${sh}`);
        if (heal > 0) rw.unshift(`❤️ 體力 +${heal}`);
        soundEngine.play("support");
        this.app.renderer.spawnParticles("sparkle", entity.x, entity.y, baseZ + 1.4, 24);
        this.persistProgress();
        this.renderQuestHud();
        feedbackHtml = `
          <div class="enc-reward-box">
            <strong>🛡️ 夥伴全力支援已送達！</strong>
            <div class="enc-tags">${rw.map((x) => `<span class="enc-tag">${escapeHtml(x)}</span>`).join("")}</div>
          </div>`;
        autoWon = this.checkVictory(false);
      } else {
        feedbackHtml = `<div class="enc-note-box">✅ 您已經領取過「${escapeHtml(entity.name)}」的支援補給！</div>`;
      }
      controlsHtml = `<button id="btnEncCloseOk" class="btn btn-primary" style="width:100%;">${autoWon ? "🎉 達成過關目標！前往慶賀！" : "💪 感謝支援！"}</button>`;
    } else if (inter.kind === "treasure") {
      const reqItem = (inter.required_item || "").trim();
      const hasReq = !reqItem || this.progress.items.includes(reqItem);
      if (alreadyDone) {
        feedbackHtml = `<div class="enc-note-box">💎 此寶藏已經開啟收藏囉！</div>`;
        controlsHtml = `<button id="btnEncCloseOk" class="btn btn-primary" style="width:100%;">關閉</button>`;
      } else if (!hasReq) {
        soundEngine.play("quiz_fail");
        feedbackHtml = `
          <div class="enc-warn-box">
            <strong>🔒 寶藏目前上鎖中（需要「${escapeHtml(reqItem)}」）</strong>
            <p>${escapeHtml(inter.locked_text || `請先取得「${reqItem}」再回來開啟！`)}</p>
          </div>`;
        controlsHtml = `<button id="btnEncCloseOk" class="btn btn-sub" style="width:100%;">先去尋找「${escapeHtml(reqItem)}」</button>`;
      } else {
        this.progress.treasures_found += 1;
        const rw = this._grantCommonRewards(entity, inter);
        soundEngine.play("firework");
        this.app.renderer.spawnParticles("firework", entity.x, entity.y, baseZ + 2.0, 32);
        this.persistProgress();
        this.renderQuestHud();
        feedbackHtml = `
          <div class="enc-reward-box">
            <strong>💎 ${escapeHtml(inter.success_text || "成功尋獲珍貴寶藏！")}</strong>
            <div class="enc-tags">${rw.map((x) => `<span class="enc-tag">${escapeHtml(x)}</span>`).join("")}</div>
          </div>`;
        autoWon = this.checkVictory(false);
        controlsHtml = `<button id="btnEncCloseOk" class="btn btn-gold" style="width:100%;">${autoWon ? "🎉 目標達成！查看過關慶賀！" : "✨ 收下寶藏"}</button>`;
      }
    } else if (inter.kind === "quiz") {
      if (alreadyDone) {
        feedbackHtml = `<div class="enc-reward-box">✅ 您已通過此項考驗！（${escapeHtml(inter.success_text || "")}）</div>`;
        controlsHtml = `<button id="btnEncCloseOk" class="btn btn-primary" style="width:100%;">關閉</button>`;
      } else {
        const opts = Array.isArray(inter.options) ? inter.options : ["A. 是", "B. 否"];
        controlsHtml = `
          <div class="enc-question-box">${escapeHtml(inter.question || "請選擇正確答案：")}</div>
          <div class="enc-options-list">
            ${opts.map((opt, idx) => `<button class="enc-opt-btn quiz-opt" data-opt="${idx}">${escapeHtml(opt)}</button>`).join("")}
          </div>
          <div id="encQuizFeedback"></div>`;
      }
    } else if (inter.kind === "robbery") {
      if (alreadyDone) {
        feedbackHtml = `<div class="enc-note-box">✅ 該名攔路歹徒已經被你擊退囉！</div>`;
        controlsHtml = `<button id="btnEncCloseOk" class="btn btn-primary" style="width:100%;">關閉</button>`;
      } else {
        const hasShield = this.progress.shield > 0 && inter.allow_shield_counter !== false;
        const opts = Array.isArray(inter.options) && inter.options.length > 0 ? inter.options : null;
        controlsHtml = `
          ${hasShield ? `<button id="btnRobShield" class="btn btn-citywalk" style="width:100%;padding:9px 12px;font-size:13px;">🛡️ 使用支援護盾直接擊退！（目前持有 ${this.progress.shield} 枚護盾・保證獲勝）</button>` : `<div class="enc-warn-box">目前無護盾，可直接應戰機智對決或向夥伴尋求護盾支援！</div>`}
          ${opts ? `
            <div class="enc-question-box" style="margin-top:6px;">🧠 機智反擊對決：${escapeHtml(inter.question || "快速選擇反擊策略！")}</div>
            <div class="enc-options-list">
              ${opts.map((opt, idx) => `<button class="enc-opt-btn rob-opt" data-opt="${idx}">${escapeHtml(opt)}</button>`).join("")}
            </div>` : ""}
          <button id="btnRobYield" class="btn btn-danger" style="width:100%;margin-top:4px;">💰 暫時妥協脱身（失去 ${Number(inter.demand_points || 15)} 積分、體力 -${Number(inter.hp_damage || 1)}）</button>
          <div id="encQuizFeedback"></div>`;
      }
    } else if (inter.kind === "suspect") {
      const alibi = inter.alibi || "我什麼都沒做，請大人明察！";
      this._addClue(entity.id, entity.name, `【${entity.name}口供】${alibi}`);
      this.persistProgress();
      this.renderQuestHud();
      feedbackHtml = `
        <div class="enc-note-box">
          <strong>💬 嫌疑人證詞／不在場證明（已抄錄至筆記）：</strong>
          <p style="margin:4px 0 0;">「${escapeHtml(alibi)}」</p>
        </div>
        <div id="encQuizFeedback"></div>`;
      controlsHtml = `
        <div class="row-gap" style="margin-top:6px;">
          <button id="btnSusAccuse" class="btn btn-danger" style="flex:1.2;">🚨 指認「${escapeHtml(entity.name)}」是真兇！</button>
          <button id="btnEncCloseOk" class="btn btn-sub" style="flex:1;">📝 先繼續調查線索</button>
        </div>`;
    }

    body.innerHTML = `
      ${statusBar}
      ${inter.dialogue ? `<p class="enc-dialogue">${escapeHtml(inter.dialogue)}</p>` : ""}
      ${feedbackHtml}
      ${controlsHtml}`;

    modal.classList.remove("hidden");

    // 若因自動發放（clue / support / treasure）剛好達成目標，短暫呈現獎勵後也可按按鈕或自動於關閉時彈出慶賀視窗
    if (autoWon) {
      this._victoryTimer && clearTimeout(this._victoryTimer);
      this._victoryTimer = setTimeout(() => {
        this._victoryTimer = null;
        this.triggerVictory();
      }, 550);
    }

    body.querySelector("#btnEncCloseOk")?.addEventListener("click", () => {
      if (this._victoryTimer) {
        clearTimeout(this._victoryTimer);
        this._victoryTimer = null;
      }
      modal.classList.add("hidden");
      this.activeEncounterEntityId = null;
      if (this.progress.completed) {
        this.triggerVictory();
      }
    });

    // 選擇題回答處理
    body.querySelectorAll(".quiz-opt").forEach((btn) => {
      btn.addEventListener("click", () => {
        const picked = Number(btn.dataset.opt);
        const correct = Number(inter.correct_index || 0);
        const fb = body.querySelector("#encQuizFeedback");
        if (picked === correct) {
          this.progress.puzzles_solved += 1;
          const rw = this._grantCommonRewards(entity, inter);
          soundEngine.play("quiz_ok");
          this.app.renderer.spawnParticles("sparkle", entity.x, entity.y, baseZ + 1.6, 28);
          this.persistProgress(true);
          this.renderQuestHud();

          if (this.checkVictory(false)) {
            this.app.showToast(`答對考驗！${rw.join("・")}`);
            this.triggerVictory();
            return;
          }
          body.querySelectorAll(".quiz-opt").forEach((b) => { b.disabled = true; });
          fb.innerHTML = `
            <div class="enc-reward-box" style="margin-top:8px;">
              <strong>✅ ${escapeHtml(inter.success_text || "回答完全正確！")}</strong>
              <div class="enc-tags">${rw.map((x) => `<span class="enc-tag">${escapeHtml(x)}</span>`).join("")}</div>
              <button id="btnQuizDoneClose" class="btn btn-primary" style="margin-top:8px;width:100%;">確定並繼續</button>
            </div>`;
          fb.querySelector("#btnQuizDoneClose").addEventListener("click", () => {
            modal.classList.add("hidden");
            this.activeEncounterEntityId = null;
          });
        } else {
          soundEngine.play("quiz_fail");
          const pen = Math.max(0, Number(inter.penalty_points || 0));
          if (pen > 0) this.progress.score = Math.max(0, this.progress.score - pen);
          this.persistProgress();
          this.renderQuestHud();
          fb.innerHTML = `<div class="enc-warn-box" style="margin-top:8px;">❌ ${escapeHtml(inter.fail_text || "答錯了，再仔細想想喔！")}${pen > 0 ? `（扣 ${pen} 分）` : ""}</div>`;
          if (inter.allow_retry === false) {
            body.querySelectorAll(".quiz-opt").forEach((b) => { b.disabled = true; });
          }
        }
      });
    });

    // 搶劫：護盾擊退／策略反擊／妥協
    const resolveRobberyWin = async (usedShield) => {
      if (usedShield && this.progress.shield > 0) {
        this.progress.shield -= 1;
      }
      const rw = this._grantCommonRewards(entity, inter);
      if (usedShield) rw.unshift("🛡️ 護盾精準反制！");
      if (inter.hide_on_defeat) {
        entity.visible = false;
        await this.app.updateEntityFields(entity.id, { visible: false });
      }
      soundEngine.play("quiz_ok");
      this.app.renderer.spawnParticles("firework", entity.x, entity.y, baseZ + 1.8, 30);
      this.persistProgress(true);
      this.renderQuestHud();
      if (this.checkVictory(false)) {
        this.app.showToast(`${inter.success_text || "成功擊退歹徒！"} (${rw.join("・")})`);
        this.triggerVictory();
        return;
      }
      this.app.showToast(`${inter.success_text || "成功擊退歹徒！"} ${rw.join(" ")}`);
      modal.classList.add("hidden");
      this.activeEncounterEntityId = null;
    };

    const resolveRobberyLoss = () => {
      const lost = Math.max(0, Number(inter.demand_points || 15));
      const dmg = Math.max(0, Number(inter.hp_damage || 1));
      this.progress.score = Math.max(0, this.progress.score - lost);
      this.progress.hp = Math.max(1, this.progress.hp - dmg);
      this.ambushCooldowns.set(entity.id, performance.now());
      soundEngine.play("quiz_fail");
      this.persistProgress(true);
      this.renderQuestHud();
      this.app.showToast(inter.fail_text || `失去 ${lost} 分，建議先找支援護盾再挑戰！`, true);
      modal.classList.add("hidden");
      this.activeEncounterEntityId = null;
    };

    body.querySelector("#btnRobShield")?.addEventListener("click", () => resolveRobberyWin(true));
    body.querySelector("#btnRobYield")?.addEventListener("click", () => resolveRobberyLoss());
    body.querySelectorAll(".rob-opt").forEach((btn) => {
      btn.addEventListener("click", () => {
        const picked = Number(btn.dataset.opt);
        if (picked === Number(inter.correct_index || 0)) {
          resolveRobberyWin(false);
        } else {
          resolveRobberyLoss();
        }
      });
    });

    // 嫌疑犯指認
    body.querySelector("#btnSusAccuse")?.addEventListener("click", () => {
      const fb = body.querySelector("#encQuizFeedback");
      const isCulprit = Boolean(inter.is_culprit) || (this.config.culprit_entity_id && this.config.culprit_entity_id === entity.id);
      if (isCulprit) {
        this.progress.culprit_caught = true;
        this.progress.score += 60;
        if (!this.progress.resolved_entities.includes(entity.id)) {
          this.progress.resolved_entities.push(entity.id);
        }
        this.persistProgress(true);
        this.renderQuestHud();
        this.app.showToast(`🚨 真兇「${entity.name}」當場逮捕歸案！+60 分`);
        this.checkVictory(true);
        if (!this.config.enabled) {
          this.triggerVictory();
        }
      } else {
        soundEngine.play("quiz_fail");
        const pen = Math.max(0, Number(inter.wrong_penalty || 20));
        this.progress.score = Math.max(0, this.progress.score - pen);
        this.progress.hp = Math.max(1, this.progress.hp - 1);
        this.progress.wrong_accusations += 1;
        this.persistProgress();
        this.renderQuestHud();
        fb.innerHTML = `<div class="enc-warn-box" style="margin-top:8px;">❌ <strong>指認錯誤！</strong>「${escapeHtml(entity.name)}」是無辜的，扣除 ${pen} 分與 1 點體力，請查看「📋 線索筆記」再確認！</div>`;
      }
    });

    return true;
  }

  /// 渲染畫布右上角「任務進度與線索筆記浮動 HUD」
  renderQuestHud() {
    const hud = document.getElementById("questHud");
    if (!hud) return;

    if (!this.config.enabled) {
      hud.classList.add("hidden");
      return;
    }
    hud.classList.remove("hidden");

    const sum = this.getProgressSummary();
    const p = this.progress;
    const isChecklist = this.config.goal_type === "custom_checklist";
    const listHtml = isChecklist && Array.isArray(this.config.checklist)
      ? `<div class="qh-checklist">${this.config.checklist.map((it) => {
          const ok = Boolean(p.flags?.[it.key]);
          return `<div class="qh-chk ${ok ? "done" : ""}">${ok ? "✅" : "⬜"} ${escapeHtml(it.label)}</div>`;
        }).join("")}</div>`
      : "";

    const notesHtml = this.notesOpen
      ? `<div class="qh-notes">
          <div class="qh-sub-title">📝 線索筆記 (${p.clues.length})</div>
          ${p.clues.length === 0 ? `<div class="hint-text">尚未蒐集到線索，走近物件按 F 或雙擊互動！</div>` : p.clues.map((c) => `<div class="qh-clue-item"><strong>${escapeHtml(c.sourceName)}：</strong>${escapeHtml(c.text)}</div>`).join("")}
          ${p.items.length > 0 ? `<div class="qh-sub-title" style="margin-top:6px;">🎒 持有道具 (${p.items.length})</div><div class="qh-items">${p.items.map((it) => `<span class="enc-tag">${escapeHtml(it)}</span>`).join("")}</div>` : ""}
        </div>`
      : "";

    hud.innerHTML = `
      <div class="qh-header">
        <span class="qh-title" title="${escapeHtml(this.config.description || "")}">${p.completed ? "🏆 " : "🎯 "}${escapeHtml(this.config.title || "冒險任務")}</span>
        <button id="btnQhCollapse" class="btn-mini" title="${this.hudCollapsed ? "展開任務面板" : "收合任務面板"}">${this.hudCollapsed ? "展開" : "收合"}</button>
      </div>
      ${this.hudCollapsed ? `<div class="qh-mini-bar">${escapeHtml(sum.text)}｜❤️ ${p.hp}｜🛡️ ${p.shield}</div>` : `
        <div class="qh-desc">${escapeHtml(this.config.description || "")}</div>
        <div class="qh-prog-row">
          <div class="qh-prog-bar"><div class="qh-prog-fill" style="width:${sum.pct}%;"></div></div>
          <span class="mono-badge">${escapeHtml(sum.text)}</span>
        </div>
        ${listHtml}
        <div class="qh-stats">
          <span>❤️ ${p.hp}/${p.max_hp}</span>
          <span>🛡️ 護盾 ${p.shield}</span>
          <span>⭐ ${p.score} 分</span>
          ${p.items.length > 0 ? `<span>🎒 ${p.items.length}</span>` : ""}
        </div>
        <div class="row-gap" style="margin-top:4px;">
          <button id="btnQhNotes" class="btn-mini" style="flex:1;">📋 筆記 (${p.clues.length})</button>
          <button id="btnQhReset" class="btn-mini" title="清除進度重玩">🔄 重置</button>
          ${this.app.sim.visitorMode ? "" : `<button id="btnQhEdit" class="btn-mini" title="開啟右欄目標設定">⚙️ 設定</button>`}
        </div>
        ${notesHtml}
      `}`;

    hud.querySelector("#btnQhCollapse")?.addEventListener("click", () => {
      this.hudCollapsed = !this.hudCollapsed;
      this.renderQuestHud();
    });
    hud.querySelector("#btnQhNotes")?.addEventListener("click", () => {
      this.notesOpen = !this.notesOpen;
      this.renderQuestHud();
    });
    hud.querySelector("#btnQhReset")?.addEventListener("click", () => {
      this.app.openResetModal();
    });
    hud.querySelector("#btnQhEdit")?.addEventListener("click", () => {
      if (this.app.rightCollapsed) {
        this.app.rightCollapsed = false;
        this.app._updatePanelCollapseState();
      }
      this.app.activeRightTab = "quest";
      this.app._syncRightTabs();
      this.app.renderRightPanel();
    });
  }

  /// 渲染右欄「🏆 任務目標規畫者面板」
  renderQuestPlannerPanel(container) {
    const cfg = this.config;
    const ents = this.app.sim.entities;
    const sum = this.getProgressSummary();

    const interactiveEnts = ents.filter((e) => parseEntityInteraction(e).kind !== "none");
    const suspectOpts = `<option value="">（依互動設定中的「真兇」旗標判定）</option>` +
      ents.map((e) => `<option value="${e.id}" ${cfg.culprit_entity_id === e.id ? "selected" : ""}>${escapeHtml(e.name)}</option>`).join("");

    let goalDetailHtml = "";
    if (["score", "find_treasure", "solve_puzzles"].includes(cfg.goal_type)) {
      const lbl = cfg.goal_type === "score" ? "過關目標積分" : (cfg.goal_type === "find_treasure" ? "需尋獲寶藏數量" : "需通過考驗題數");
      goalDetailHtml = `
        <div class="prop-row">
          <label>${lbl}</label>
          <input id="qpTargetCount" type="number" min="1" max="9999" class="ctrl-input" style="width:92px;" value="${Number(cfg.target_count || 100)}" />
        </div>`;
    } else if (cfg.goal_type === "catch_culprit") {
      goalDetailHtml = `
        <div class="prop-row">
          <label>真正歹徒</label>
          <select id="qpCulpritSelect" class="ctrl-select" style="flex:1;">${suspectOpts}</select>
        </div>
        <div class="hint-text">請將數個 NPC 設為「🕵️ 案件嫌疑犯」，並將真兇標記為歹徒！</div>`;
    } else if (cfg.goal_type === "custom_checklist") {
      const list = Array.isArray(cfg.checklist) ? cfg.checklist : [];
      goalDetailHtml = `
        <div class="sec-title">待完成檢查清單 (${list.length})</div>
        <div id="qpChecklistBox" style="display:flex;flex-direction:column;gap:4px;">
          ${list.map((it, i) => `
            <div class="row-gap" data-chk-idx="${i}">
              <input class="ctrl-input qp-chk-key" style="width:76px;" placeholder="key" value="${escapeHtml(it.key)}" />
              <input class="ctrl-input qp-chk-lbl" style="flex:1;" placeholder="顯示項目名稱" value="${escapeHtml(it.label)}" />
              <button class="btn-mini qp-chk-del">✕</button>
            </div>`).join("")}
        </div>
        <button id="btnQpAddChk" class="btn btn-sub" style="width:100%;margin-top:4px;">＋ 新增清單條件</button>`;
    }

    const html = `
      <div class="prop-card">
        <div class="row-gap space-between">
          <strong>🏆 終極過關目標設定</strong>
          <label class="inline-field">
            <input id="qpEnabled" type="checkbox" ${cfg.enabled ? "checked" : ""} />
            <span>啟用任務關卡</span>
          </label>
        </div>
        <div class="hint-text">目前進度：${escapeHtml(sum.text)}（${this.progress.completed ? "已通關 🎉" : "進行中"}）</div>
        <div class="prop-row">
          <label>關卡標題</label>
          <input id="qpTitle" class="ctrl-input" style="flex:1;" value="${escapeHtml(cfg.title)}" />
        </div>
        <div class="prop-row">
          <label>目標種類</label>
          <select id="qpGoalType" class="ctrl-select" style="flex:1;">
            ${Object.entries(GOAL_TYPE_LABELS).map(([k, lbl]) => `<option value="${k}" ${cfg.goal_type === k ? "selected" : ""}>${lbl}</option>`).join("")}
          </select>
        </div>
        ${goalDetailHtml}
        <label class="field-label">過關指引說明
          <textarea id="qpDesc" class="ctrl-input" rows="2" style="resize:vertical;">${escapeHtml(cfg.description || "")}</textarea>
        </label>
        <div class="prop-row">
          <label>慶賀標題</label>
          <input id="qpVicTitle" class="ctrl-input" style="flex:1;" value="${escapeHtml(cfg.victory_title || "")}" />
        </div>
        <label class="field-label">過關慶賀訊息
          <textarea id="qpVicMsg" class="ctrl-input" rows="2" style="resize:vertical;">${escapeHtml(cfg.victory_message || "")}</textarea>
        </label>
        <div class="row-gap">
          <button id="btnQpSave" class="btn btn-primary" style="flex:1.3;">💾 儲存目標設定</button>
          <button id="btnQpTestVictory" class="btn btn-gold" style="flex:1;">🎆 預覽過關視窗</button>
        </div>
      </div>

      <div class="prop-card">
        <div class="row-gap space-between">
          <strong>🎭 場內任務 NPC／物件 (${interactiveEnts.length})</strong>
          <button id="btnQpResetProg" class="btn-mini">🔄 清除進度</button>
        </div>
        <div class="hint-text">點擊下方項目可立即定位並編輯其線索、選擇題、搶劫或寶藏設定：</div>
        <div style="display:flex;flex-direction:column;gap:5px;max-height:260px;overflow-y:auto;">
          ${interactiveEnts.length === 0 ? `<div class="hint-text">尚未設定任何互動物件。點選畫布上任意物件即可在「🎛️ 屬性」內指派任務角色！</div>` : interactiveEnts.map((e) => {
            const inter = parseEntityInteraction(e);
            const meta = INTERACTION_META[inter.kind] || INTERACTION_META.clue;
            return `
              <div class="lib-item" data-qent="${e.id}" style="cursor:pointer;">
                <span>${meta.icon} <strong>${escapeHtml(e.name)}</strong> <small style="color:#94a3b8;">(${meta.shortBadge})</small></span>
                <div class="row-gap">
                  <button class="btn-mini qent-play" title="直接體驗互動">▶</button>
                  <button class="btn-mini qent-sel" title="在屬性欄編輯">🎛️</button>
                </div>
              </div>`;
          }).join("")}
        </div>
      </div>`;

    container.innerHTML = html;

    const readFormToCfg = () => {
      cfg.enabled = container.querySelector("#qpEnabled").checked;
      cfg.title = container.querySelector("#qpTitle").value.trim() || "冒險任務關卡";
      cfg.goal_type = container.querySelector("#qpGoalType").value;
      const tcEl = container.querySelector("#qpTargetCount");
      if (tcEl) cfg.target_count = Math.max(1, Number(tcEl.value || 1));
      const culEl = container.querySelector("#qpCulpritSelect");
      if (culEl) cfg.culprit_entity_id = culEl.value || "";
      if (cfg.goal_type === "custom_checklist") {
        const items = [];
        container.querySelectorAll("[data-chk-idx]").forEach((row) => {
          const key = row.querySelector(".qp-chk-key").value.trim();
          const label = row.querySelector(".qp-chk-lbl").value.trim();
          if (key && label) items.push({ key, label });
        });
        cfg.checklist = items;
      }
      cfg.description = container.querySelector("#qpDesc").value;
      cfg.victory_title = container.querySelector("#qpVicTitle").value.trim() || "🎉 恭喜過關！";
      cfg.victory_message = container.querySelector("#qpVicMsg").value;
    };

    container.querySelector("#qpGoalType").addEventListener("change", () => {
      readFormToCfg();
      this.renderQuestPlannerPanel(container);
    });
    container.querySelector("#btnQpAddChk")?.addEventListener("click", () => {
      readFormToCfg();
      cfg.checklist = cfg.checklist || [];
      cfg.checklist.push({ key: `step_${cfg.checklist.length + 1}`, label: "新增任務步驟" });
      this.renderQuestPlannerPanel(container);
    });
    container.querySelectorAll(".qp-chk-del").forEach((btn) => {
      btn.addEventListener("click", () => {
        readFormToCfg();
        const idx = Number(btn.closest("[data-chk-idx]").dataset.chkIdx);
        cfg.checklist.splice(idx, 1);
        this.renderQuestPlannerPanel(container);
      });
    });

    container.querySelector("#btnQpSave").addEventListener("click", async () => {
      readFormToCfg();
      await this.saveConfig(cfg);
      this.app.showToast("過關目標設定已儲存至 SQLite！");
    });
    container.querySelector("#btnQpTestVictory").addEventListener("click", () => {
      readFormToCfg();
      this.triggerVictory();
    });
    container.querySelector("#btnQpResetProg").addEventListener("click", () => this.app.openResetModal());

    container.querySelectorAll("[data-qent]").forEach((row) => {
      const eid = row.dataset.qent;
      const ent = ents.find((x) => x.id === eid);
      if (!ent) return;
      row.querySelector(".qent-play").addEventListener("click", (ev) => {
        ev.stopPropagation();
        this.openEncounterModal(ent);
      });
      const focusEdit = () => {
        this.app.renderer.selectedEntityId = eid;
        const gz = this.app.renderer.getTerrainElevationAt(ent.x, ent.y);
        this.app.renderer.camera.target = [ent.x, ent.y, gz + 0.8];
        this.app.activeRightTab = "inspector";
        this.app._syncRightTabs();
        this.app.renderRightPanel();
      };
      row.querySelector(".qent-sel").addEventListener("click", (ev) => {
        ev.stopPropagation();
        focusEdit();
      });
      row.addEventListener("click", focusEdit);
    });
  }

  /// 內嵌在「🎛️ 物件／屬性」面板中的單一物件任務互動設定器
  renderEntityInteractionEditor(container, entity) {
    if (!container || !entity) return;
    const inter = parseEntityInteraction(entity);
    const kind = inter.kind || "none";

    const kindOptions = Object.entries(INTERACTION_META)
      .map(([k, m]) => `<option value="${k}" ${kind === k ? "selected" : ""}>${m.icon ? `${m.icon} ` : ""}${m.label}</option>`)
      .join("");

    let extraHtml = "";
    if (kind !== "none") {
      extraHtml += `
        <div class="prop-row">
          <label>互動標題</label>
          <input id="eiTitle" class="ctrl-input" style="flex:1;" value="${escapeHtml(inter.title || entity.name)}" />
        </div>
        <label class="field-label">對話／情境說明
          <textarea id="eiDialogue" class="ctrl-input" rows="2" style="resize:vertical;">${escapeHtml(inter.dialogue || "")}</textarea>
        </label>`;

      if (kind === "clue" || kind === "support") {
        extraHtml += `
          <div class="prop-row">
            <label>筆記線索</label>
            <input id="eiClueText" class="ctrl-input" style="flex:1;" placeholder="紀錄進筆記本的關鍵文字" value="${escapeHtml(inter.clue_text || "")}" />
          </div>`;
      }
      if (kind === "support") {
        extraHtml += `
          <div class="prop-row">
            <label>護盾／回血</label>
            <div class="row-gap">
              <input id="eiShield" type="number" min="0" max="10" class="ctrl-input" style="width:62px;" value="${Number(inter.grant_shield ?? 1)}" />
              <input id="eiHeal" type="number" min="0" max="10" class="ctrl-input" style="width:62px;" value="${Number(inter.heal_hp ?? 1)}" />
            </div>
          </div>`;
      }
      if (kind === "quiz" || kind === "robbery") {
        const opts = Array.isArray(inter.options) ? inter.options : ["A. 正確選項", "B. 干擾選項"];
        extraHtml += `
          <div class="prop-row">
            <label>題目內容</label>
            <input id="eiQuestion" class="ctrl-input" style="flex:1;" value="${escapeHtml(inter.question || "")}" />
          </div>
          <label class="field-label">選項清單（每行一個選項）
            <textarea id="eiOptions" class="ctrl-input" rows="3">${escapeHtml(opts.join("\n"))}</textarea>
          </label>
          <div class="prop-row">
            <label>正解編號</label>
            <input id="eiCorrectIdx" type="number" min="0" max="9" class="ctrl-input" style="width:68px;" value="${Number(inter.correct_index || 0)}" />
            <span class="hint-text">第 1 行為 0</span>
          </div>`;
      }
      if (kind === "robbery") {
        extraHtml += `
          <div class="prop-row">
            <label>攔截距離</label>
            <input id="eiAmbushRad" type="number" min="0" max="6" step="0.2" class="ctrl-input" style="width:68px;" value="${Number(inter.auto_trigger_radius ?? 2.0)}" />
            <span class="hint-text">0 表示僅點擊/F觸發</span>
          </div>
          <div class="row-gap">
            <label class="inline-field"><input id="eiAllowShield" type="checkbox" ${inter.allow_shield_counter !== false ? "checked" : ""} /> 允許護盾反制</label>
            <label class="inline-field"><input id="eiHideDefeat" type="checkbox" ${inter.hide_on_defeat ? "checked" : ""} /> 擊退後暫時退場</label>
          </div>`;
      }
      if (kind === "treasure") {
        extraHtml += `
          <div class="prop-row">
            <label>所需道具</label>
            <input id="eiReqItem" class="ctrl-input" style="flex:1;" placeholder="留空表示不需鑰匙" value="${escapeHtml(inter.required_item || "")}" />
          </div>`;
      }
      if (kind === "suspect") {
        extraHtml += `
          <label class="field-label">嫌疑人不在場證明／口供
            <textarea id="eiAlibi" class="ctrl-input" rows="2">${escapeHtml(inter.alibi || "")}</textarea>
          </label>
          <div class="row-gap space-between">
            <label class="inline-field"><input id="eiIsCulprit" type="checkbox" ${inter.is_culprit ? "checked" : ""} /> 🚨 此角色就是真兇歹徒</label>
          </div>`;
      }
      if (kind !== "suspect") {
        extraHtml += `
          <div class="prop-row">
            <label>獎勵分／物</label>
            <div class="row-gap" style="flex:1;">
              <input id="eiPts" type="number" step="5" class="ctrl-input" style="width:66px;" value="${Number(inter.reward_points ?? 20)}" />
              <input id="eiItem" class="ctrl-input" style="flex:1;" placeholder="贈與道具(選填)" value="${escapeHtml(inter.reward_item || "")}" />
            </div>
          </div>
          <div class="prop-row">
            <label>清單 Key</label>
            <input id="eiFlag" class="ctrl-input" style="flex:1;" placeholder="綜合清單旗標 key（選填）" value="${escapeHtml(inter.reward_flag || "")}" />
          </div>`;
      }
      extraHtml += `
        <div class="row-gap" style="margin-top:4px;">
          <button id="btnEiSave" class="btn btn-primary" style="flex:1;">💾 儲存任務設定</button>
          <button id="btnEiTest" class="btn btn-gold">▶ 體驗</button>
        </div>`;
    }

    container.innerHTML = `
      <div class="prop-card">
        <div class="row-gap space-between">
          <strong>🎭 任務角色／互動考驗</strong>
        </div>
        <div class="prop-row">
          <label>互動類型</label>
          <select id="eiKind" class="ctrl-select" style="flex:1;">${kindOptions}</select>
        </div>
        ${extraHtml}
      </div>`;

    const kindSel = container.querySelector("#eiKind");
    kindSel.addEventListener("change", async () => {
      const nextKind = kindSel.value;
      if (nextKind === "none") {
        entity.meta_json = buildEntityMetaWithInteraction(entity, { kind: "none" });
        await this.app.updateEntityFields(entity.id, { meta_json: entity.meta_json });
        this.renderEntityInteractionEditor(container, entity);
        this.app.showToast(`已移除「${entity.name}」的任務互動設定`);
        return;
      }
      const starter = {
        kind: nextKind,
        title: `${entity.name} 的${INTERACTION_META[nextKind]?.shortBadge || "互動"}`,
        dialogue: inter.dialogue || `你好，冒險者！我是「${entity.name}」。`,
        reward_points: inter.reward_points ?? 20,
        once: true,
        ...(nextKind === "quiz" ? {
          question: "這裡是選擇題範例，請輸入你的題目？",
          options: ["A. 正確選項", "B. 錯誤選項"],
          correct_index: 0,
          allow_retry: true,
        } : {}),
        ...(nextKind === "robbery" ? {
          auto_trigger_radius: 2.0,
          demand_points: 15,
          hp_damage: 1,
          allow_shield_counter: true,
          hide_on_defeat: true,
          question: "遇到搶劫如何巧妙脫困反擊？",
          options: ["A. 把握破綻制伏對方", "B. 原地投降"],
          correct_index: 0,
        } : {}),
        ...(nextKind === "support" ? { grant_shield: 1, heal_hp: 2 } : {}),
        ...(nextKind === "suspect" ? { alibi: "我一直待在原地，沒有靠近案發現場！", is_culprit: false, wrong_penalty: 20 } : {}),
      };
      entity.meta_json = buildEntityMetaWithInteraction(entity, starter);
      await this.app.updateEntityFields(entity.id, { meta_json: entity.meta_json });
      this.renderEntityInteractionEditor(container, entity);
    });

    const saveInteraction = async () => {
      const cur = parseEntityInteraction(entity);
      if (cur.kind === "none") return;
      const updated = {
        ...cur,
        kind: cur.kind,
        title: container.querySelector("#eiTitle")?.value.trim() || entity.name,
        dialogue: container.querySelector("#eiDialogue")?.value || "",
        once: true,
      };
      if (cur.kind === "clue" || cur.kind === "support") {
        updated.clue_text = container.querySelector("#eiClueText")?.value || updated.dialogue;
      }
      if (cur.kind === "support") {
        updated.grant_shield = Number(container.querySelector("#eiShield")?.value || 0);
        updated.heal_hp = Number(container.querySelector("#eiHeal")?.value || 0);
      }
      if (cur.kind === "quiz" || cur.kind === "robbery") {
        updated.question = container.querySelector("#eiQuestion")?.value || "";
        const rawOpts = (container.querySelector("#eiOptions")?.value || "").split("\n").map((s) => s.trim()).filter(Boolean);
        updated.options = rawOpts.length > 0 ? rawOpts : ["A. 確定"];
        updated.correct_index = Number(container.querySelector("#eiCorrectIdx")?.value || 0);
      }
      if (cur.kind === "robbery") {
        updated.auto_trigger_radius = Number(container.querySelector("#eiAmbushRad")?.value || 0);
        updated.allow_shield_counter = Boolean(container.querySelector("#eiAllowShield")?.checked);
        updated.hide_on_defeat = Boolean(container.querySelector("#eiHideDefeat")?.checked);
      }
      if (cur.kind === "treasure") {
        updated.required_item = container.querySelector("#eiReqItem")?.value.trim() || "";
      }
      if (cur.kind === "suspect") {
        updated.alibi = container.querySelector("#eiAlibi")?.value || "";
        updated.is_culprit = Boolean(container.querySelector("#eiIsCulprit")?.checked);
      } else {
        updated.reward_points = Number(container.querySelector("#eiPts")?.value || 0);
        updated.reward_item = container.querySelector("#eiItem")?.value.trim() || "";
        updated.reward_flag = container.querySelector("#eiFlag")?.value.trim() || "";
      }

      entity.meta_json = buildEntityMetaWithInteraction(entity, updated);
      await this.app.updateEntityFields(entity.id, { meta_json: entity.meta_json });
      // 若標記為真兇，順手更新 quest_config.culprit_entity_id
      if (cur.kind === "suspect" && updated.is_culprit && this.config.goal_type === "catch_culprit") {
        this.config.culprit_entity_id = entity.id;
        await this.saveConfig(this.config);
      }
      this.app.showToast(`已儲存「${entity.name}」的任務互動設定！`);
    };

    container.querySelector("#btnEiSave")?.addEventListener("click", saveInteraction);
    container.querySelector("#btnEiTest")?.addEventListener("click", async () => {
      await saveInteraction();
      this.openEncounterModal(entity);
    });
  }
}

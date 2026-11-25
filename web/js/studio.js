//! Sprite-Stacking 造物工坊 Modal：
//! - 快速文字與 68 種內建原型即時烘焙
//! - 3D 積木藍圖細部調教（位置 X/Y/Z、長寬高 W/D/H、半徑、材質、發光、鏡像、排序複製與即時預覽重烘）
//! - 個人跨世界資產庫與本世界資產清單之載入、改名/藍圖細調編輯與刪除
//! - 四視圖雕刻與 `.zip` / `.json` / `.png` 匯入
//! - AI 藍圖生成（Google Gemini、Google Gemma、OpenAI 相容端點、自訂模型名稱與 API Key 記憶/移除）

const AI_MODEL_SUGGESTIONS = {
  gemini: [
    "gemini-2.5-flash",
    "gemini-2.5-pro",
    "gemini-2.5-flash-lite",
    "gemini-3-flash-preview",
    "gemini-3.1-pro-preview",
  ],
  gemma: ["gemma-4-31b-it", "gemma-4-26b-a4b-it", "gemma-3-27b-it"],
  openai: ["gpt-4o-mini", "gpt-4.1-mini", "gpt-4.1", "qwen2.5:14b"],
};

const BP_COLOR_CHOICES = [
  "red", "blue", "green", "yellow", "gold", "silver", "wood", "dark_wood",
  "stone", "brick", "sakura", "leaf", "glass", "neon_cyan", "neon_yellow",
  "neon_pink", "white", "black",
];

const BP_MATERIALS = [
  { value: "stone", label: "stone（石材）" },
  { value: "wood", label: "wood（木質）" },
  { value: "glass", label: "glass（玻璃）" },
  { value: "metal", label: "metal（金屬）" },
  { value: "fabric", label: "fabric（布料）" },
  { value: "foliage", label: "foliage（葉片）" },
  { value: "crystal", label: "crystal（水晶）" },
  { value: "fur", label: "fur（毛皮）" },
  { value: "mane", label: "mane（鬃毛）" },
  { value: "flat", label: "flat（平塗）" },
];

const CAT_LABELS = {
  building: "建築",
  character: "人物",
  animal: "動物",
  plant: "植物",
  vehicle: "載具",
  prop: "道具/家具",
};

export class CreationStudio {
  constructor({
    getWorldFile,
    getWorldAssets,
    onAssetReady,
    onAssetUpdated,
    onAssetDeleted,
    showToast,
  }) {
    this.getWorldFile = getWorldFile;
    this.getWorldAssets = getWorldAssets || (() => []);
    this.onAssetReady = onAssetReady;
    this.onAssetUpdated = onAssetUpdated || (() => {});
    this.onAssetDeleted = onAssetDeleted || (() => {});
    this.showToast = showToast;

    this.currentAsset = null;
    this.currentDiag = null;
    this.currentBlueprint = null;
    // 標記使用者是否在 3D 積木分頁修改了尚未確認儲存的草稿
    this.hasUnsavedBlueprintChanges = false;
    // { scope: "library" | "world", id, name }
    this.editingTarget = null;
    this.turntableAngle = 35;
    this.turntableLight = "day";
    this.dragging = false;
    this.lastX = 0;
    this.previewImgs = null;
    this._autoBakeTimer = null;

    this._bindElements();
    this._syncAiProviderUi();
    this._startPreviewLoop();
  }

  _bindElements() {
    this.modal = document.getElementById("studioModal");
    this.previewCanvas = document.getElementById("studioPreviewCanvas");
    this.pctx = this.previewCanvas.getContext("2d");
    this.diagBox = document.getElementById("studioDiagBox");
    this.catSelect = document.getElementById("studioCategorySelect");
    this.btnUse = document.getElementById("btnStudioUseAsset");

    // 分頁切換
    document.querySelectorAll(".st-tab").forEach((btn) => {
      btn.addEventListener("click", () => this.switchTab(btn.dataset.st));
    });

    // 日／黃昏／夜景預覽切換
    document.querySelectorAll("#stLightGroup .seg-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        document.querySelectorAll("#stLightGroup .seg-btn").forEach((b) => b.classList.remove("active"));
        btn.classList.add("active");
        this.turntableLight = btn.dataset.stlight;
      });
    });

    // 轉盤滑鼠拖曳旋轉
    this.previewCanvas.addEventListener("mousedown", (e) => {
      this.dragging = true;
      this.lastX = e.clientX;
      this.previewCanvas.style.cursor = "grabbing";
    });
    window.addEventListener("mousemove", (e) => {
      if (!this.dragging) return;
      this.turntableAngle = (this.turntableAngle + (e.clientX - this.lastX) * 0.8) % 360;
      this.lastX = e.clientX;
    });
    window.addEventListener("mouseup", () => {
      this.dragging = false;
      this.previewCanvas.style.cursor = "grab";
    });

    // 快速造物：切片滑桿與生成按鈕
    const slSlider = document.getElementById("stSliceSlider");
    const slVal = document.getElementById("stSliceVal");
    slSlider.addEventListener("input", () => {
      slVal.textContent = slSlider.value;
    });
    document.getElementById("btnStGenerate").addEventListener("click", () => {
      const prompt = document.getElementById("stPromptInput").value.trim();
      if (!prompt) {
        this.showToast("請先輸入想製作的物件描述！", true);
        return;
      }
      this.editingTarget = null;
      this._renderEditBanner();
      this.generateByPrompt(prompt, Number(slSlider.value));
    });
    document.getElementById("stPromptInput").addEventListener("keydown", (e) => {
      if (e.key === "Enter") document.getElementById("btnStGenerate").click();
    });

    // 3D 積木藍圖標題與畫布尺寸編輯
    const bpTitleInput = document.getElementById("bpTitleInput");
    const bpSizeW = document.getElementById("bpSizeW");
    const bpSizeL = document.getElementById("bpSizeL");
    const bpSizeH = document.getElementById("bpSizeH");

    const onMetaChange = (autoBake = false) => {
      if (!this.currentBlueprint) {
        this.currentBlueprint = { title: "我的積木作品", size: [40, 40, 44], primitives: [] };
      }
      this.currentBlueprint.title = bpTitleInput.value.trim() || "我的積木作品";
      this.currentBlueprint.size = [
        Math.max(16, Math.min(96, parseInt(bpSizeW.value, 10) || 40)),
        Math.max(16, Math.min(96, parseInt(bpSizeL.value, 10) || 40)),
        Math.max(16, Math.min(80, parseInt(bpSizeH.value, 10) || 44)),
      ];
      this._setUnsavedFlag(true);
      this._syncJsonTextarea();
      if (autoBake) this._schedulePreviewBake();
    };
    bpTitleInput?.addEventListener("input", () => onMetaChange(false));
    [bpSizeW, bpSizeL, bpSizeH].forEach((el) => el?.addEventListener("change", () => onMetaChange(true)));

    // 3D 積木藍圖新增原語與純預覽／確認儲存按鈕
    document.getElementById("btnAddPrimBox").addEventListener("click", () => this._addPrim("box"));
    document.getElementById("btnAddPrimCyl").addEventListener("click", () => this._addPrim("cylinder"));
    document.getElementById("btnAddPrimSph").addEventListener("click", () => this._addPrim("sphere"));
    document.getElementById("btnAddPrimCone")?.addEventListener("click", () => this._addPrim("cone"));
    document.getElementById("btnAddPrimPyr")?.addEventListener("click", () => this._addPrim("pyramid"));
    document.getElementById("btnAddPrimClear")?.addEventListener("click", () => this._addPrim("clear_box"));
    document.getElementById("btnPreviewBp")?.addEventListener("click", () => this.previewCurrentBlueprint());
    document.getElementById("btnBakeBp").addEventListener("click", () => this.bakeCurrentBlueprint());

    document.getElementById("btnBpSaveOverwrite")?.addEventListener("click", () => this.saveEditedTargetOverwrite());
    document.getElementById("btnBpSaveAsNew")?.addEventListener("click", () => {
      this.editingTarget = null;
      this._renderEditBanner();
      this.bakeCurrentBlueprint();
    });
    document.getElementById("btnBpCancelEdit")?.addEventListener("click", () => {
      this.editingTarget = null;
      this._renderEditBanner();
      this.showToast("已結束既有物件覆寫編輯模式。");
    });

    document.getElementById("bpJsonEditor").addEventListener("change", (e) => {
      try {
        this.currentBlueprint = JSON.parse(e.target.value);
        this._setUnsavedFlag(true);
        this._renderBlueprintRows();
        this._schedulePreviewBake();
      } catch {
        this.showToast("藍圖 JSON 格式語法錯誤，請檢查括號或逗號。", true);
      }
    });

    // 四視圖雕刻與既有套件匯入
    document.getElementById("btnDoCarve").addEventListener("click", () => this.doCarve());
    document.getElementById("btnDoImportAsset").addEventListener("click", () => this.doImportFile());

    // AI 藍圖分頁（Gemma 4 / Gemini / OpenAI、查詢可用模型、自訂模型、API Key 記憶與移除）
    document.getElementById("aiProvider")?.addEventListener("change", () => this._syncAiProviderUi());
    document.getElementById("btnFetchAiModels")?.addEventListener("click", () => this.fetchAvailableAiModels());
    document.getElementById("btnSaveAiKey")?.addEventListener("click", () => this.saveAiKeyToStorage(true));
    document.getElementById("btnClearAiKey")?.addEventListener("click", () => this.clearAiKeyFromStorage());
    document.getElementById("btnAiGenerate").addEventListener("click", () => this.doAiGenerate());

    // 加入資產庫並開始投放（若有未儲存的積木草稿變更，自動確認儲存後再投放）
    this.btnUse.addEventListener("click", async () => {
      if (this.hasUnsavedBlueprintChanges && this.currentBlueprint) {
        const ok = this.editingTarget
          ? await this.saveEditedTargetOverwrite()
          : await this.bakeCurrentBlueprint();
        if (!ok) return;
      }
      if (!this.currentAsset) return;
      this.currentAsset.category = this.catSelect.value;
      this.modal.classList.add("hidden");
      this.onAssetReady(this.currentAsset, true);
    });
  }

  switchTab(tabKey) {
    document.querySelectorAll(".st-tab").forEach((b) => {
      b.classList.toggle("active", b.dataset.st === tabKey);
    });
    const map = {
      prompt: "stPanePrompt",
      blueprint: "stPaneBlueprint",
      import: "stPaneImport",
      ai: "stPaneAi",
    };
    document.querySelectorAll(".st-pane").forEach((p) => p.classList.remove("active"));
    document.getElementById(map[tabKey])?.classList.add("active");
    if (tabKey === "import") {
      this.loadLibrary();
      this.renderWorldAssetsList();
    }
    if (tabKey === "ai") {
      this.checkAiStatus();
      this._syncAiProviderUi();
    }
  }

  async open(initialAsset = null) {
    this.modal.classList.remove("hidden");
    await this.loadPresets();
    if (initialAsset) {
      await this._setActiveAsset(initialAsset, null);
    }
  }

  // ==================== AI 藍圖設定（Gemma＋自訂模型＋API Key 記憶與移除） ====================

  _aiStorageKey(provider) {
    return `stackcity_ai_key_${provider}`;
  }
  _aiModelStorageKey(provider) {
    return `stackcity_ai_model_${provider}`;
  }

  _syncAiProviderUi() {
    const prov = document.getElementById("aiProvider")?.value || "gemini";
    const modelInput = document.getElementById("aiModelInput");
    const chipsBox = document.getElementById("aiModelChips");
    const baseWrap = document.getElementById("aiBaseUrlWrap");
    const baseInput = document.getElementById("aiBaseUrlInput");
    const keyInput = document.getElementById("aiKeyInput");

    const suggestions = AI_MODEL_SUGGESTIONS[prov] || AI_MODEL_SUGGESTIONS.gemini;
    let savedModel = localStorage.getItem(this._aiModelStorageKey(prov)) || "";
    // 若先前記憶為舊版已下架的 gemma-3-27b-it，自動切換為新版官方 gemma-4-31b-it
    if (prov === "gemma" && savedModel === "gemma-3-27b-it") {
      savedModel = "gemma-4-31b-it";
      localStorage.setItem(this._aiModelStorageKey(prov), savedModel);
    }
    if (modelInput) {
      modelInput.value = savedModel || suggestions[0];
    }

    if (chipsBox) {
      this._renderAiModelChips(suggestions);
    }

    if (baseWrap) {
      baseWrap.classList.toggle("hidden", prov !== "openai");
    }
    if (baseInput && prov === "openai") {
      baseInput.value = localStorage.getItem("stackcity_ai_baseurl_openai") || "";
    }

    const savedKey = localStorage.getItem(this._aiStorageKey(prov)) || "";
    if (keyInput) {
      keyInput.value = savedKey;
    }
    this._updateAiKeySavedBadge(prov);
  }

  _renderAiModelChips(models) {
    const chipsBox = document.getElementById("aiModelChips");
    const modelInput = document.getElementById("aiModelInput");
    if (!chipsBox) return;
    chipsBox.innerHTML = "";
    for (const m of models) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "chip-btn";
      b.textContent = m;
      b.addEventListener("click", () => {
        if (modelInput) modelInput.value = m;
      });
      chipsBox.appendChild(b);
    }
  }

  async fetchAvailableAiModels() {
    const prov = document.getElementById("aiProvider")?.value || "gemini";
    const apiKey = document.getElementById("aiKeyInput")?.value.trim() || "";
    const baseUrl = document.getElementById("aiBaseUrlInput")?.value.trim() || "";
    const btn = document.getElementById("btnFetchAiModels");
    if (btn) btn.disabled = true;
    try {
      const res = await fetch("/api/ai/models", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          provider: prov,
          api_key: apiKey || null,
          base_url: prov === "openai" && baseUrl ? baseUrl : null,
        }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || "查詢可用模型失敗");
      const models = data.models || [];
      if (models.length === 0) {
        this.showToast("此金鑰未回傳任何可用模型。", true);
        return;
      }
      this._renderAiModelChips(models.slice(0, 12));
      const modelInput = document.getElementById("aiModelInput");
      if (modelInput && !models.includes(modelInput.value.trim())) {
        modelInput.value = models[0];
      }
      this.showToast(`已查得 ${models.length} 個可用模型，可直接點選晶片套用！`);
    } catch (err) {
      this.showToast(err.message, true);
    } finally {
      if (btn) btn.disabled = false;
    }
  }

  _updateAiKeySavedBadge(prov) {
    const badge = document.getElementById("aiKeySavedBadge");
    if (!badge) return;
    const hasSaved = Boolean(localStorage.getItem(this._aiStorageKey(prov)));
    badge.textContent = hasSaved ? `✅ 已記憶 (${prov})` : "尚未記憶";
    badge.style.color = hasSaved ? "#4ade80" : "#94a3b8";
  }

  saveAiKeyToStorage(showFeedback = true) {
    const prov = document.getElementById("aiProvider")?.value || "gemini";
    const key = document.getElementById("aiKeyInput")?.value.trim() || "";
    const model = document.getElementById("aiModelInput")?.value.trim() || "";
    const baseUrl = document.getElementById("aiBaseUrlInput")?.value.trim() || "";

    if (model) {
      localStorage.setItem(this._aiModelStorageKey(prov), model);
    }
    if (prov === "openai") {
      if (baseUrl) localStorage.setItem("stackcity_ai_baseurl_openai", baseUrl);
      else localStorage.removeItem("stackcity_ai_baseurl_openai");
    }

    if (!key) {
      if (showFeedback) this.showToast("請先在上方輸入框填入 API Key 後再點記憶。", true);
      this._updateAiKeySavedBadge(prov);
      return;
    }
    localStorage.setItem(this._aiStorageKey(prov), key);
    this._updateAiKeySavedBadge(prov);
    if (showFeedback) {
      this.showToast(`已於本機記憶 ${prov.toUpperCase()} 的 API Key 與模型設定！`);
    }
  }

  clearAiKeyFromStorage() {
    const prov = document.getElementById("aiProvider")?.value || "gemini";
    localStorage.removeItem(this._aiStorageKey(prov));
    const keyInput = document.getElementById("aiKeyInput");
    if (keyInput) keyInput.value = "";
    this._updateAiKeySavedBadge(prov);
    this.showToast(`已移除本機記憶的 ${prov.toUpperCase()} API Key！`);
  }

  async checkAiStatus() {
    try {
      const res = await fetch("/api/ai/status");
      const st = await res.json();
      document.getElementById("aiKeyStatusBadge").textContent =
        `系統環境變數偵測：Gemini ${st.gemini ? "✅ 已設定" : "未設定"} / Gemma ${st.gemma ? "✅ 可用" : "未設定"} / OpenAI ${st.openai ? "✅ 已設定" : "未設定"}`;
    } catch { /* ignore */ }
  }

  async doAiGenerate() {
    const wf = this.getWorldFile();
    const prompt = document.getElementById("aiPromptInput").value.trim();
    if (!wf || !prompt) {
      this.showToast("請輸入 AI 建模提示描述！", true);
      return;
    }
    const provider = document.getElementById("aiProvider").value;
    const model = document.getElementById("aiModelInput").value.trim();
    const apiKey = document.getElementById("aiKeyInput").value.trim();
    const baseUrl = document.getElementById("aiBaseUrlInput")?.value.trim() || "";
    const remember = document.getElementById("aiRememberKeyChk")?.checked;

    if (remember) {
      if (model) localStorage.setItem(this._aiModelStorageKey(provider), model);
      if (apiKey) {
        localStorage.setItem(this._aiStorageKey(provider), apiKey);
        this._updateAiKeySavedBadge(provider);
      }
      if (provider === "openai" && baseUrl) {
        localStorage.setItem("stackcity_ai_baseurl_openai", baseUrl);
      }
    }

    this.editingTarget = null;
    this._renderEditBanner();
    this.diagBox.textContent = `正在呼叫 ${provider.toUpperCase()} (${model || "預設模型"}) 設計 3D 幾何藍圖並由 Rust 即時烘焙...`;
    try {
      const res = await fetch(`/api/worlds/${encodeURIComponent(wf)}/assets/ai`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          prompt,
          provider,
          model: model || null,
          base_url: provider === "openai" && baseUrl ? baseUrl : null,
          api_key: apiKey || null,
          slices: Number(document.getElementById("stSliceSlider").value),
          category: this.catSelect.value,
          lang: window.__stackCityI18n?.getLang?.() || "zh-Hant",
        }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || "AI 建模失敗");
      await this._setActiveAsset(data.asset, data.diagnostics);
      this.onAssetReady(data.asset, false);
      this.showToast(`AI 創作完成：「${data.asset.name}」！`);
    } catch (err) {
      this.diagBox.textContent = err.message;
      this.showToast(err.message, true);
    }
  }

  // ==================== 快速造物 / 雕刻 / 套件匯入 ====================

  async loadPresets() {
    const box = document.getElementById("stPresetCatalog");
    if (box.childElementCount > 0) return;
    const res = await fetch("/api/assets/presets");
    const data = await res.json();
    box.innerHTML = "";
    for (const p of data.presets || []) {
      const chip = document.createElement("button");
      chip.className = "preset-chip";
      chip.textContent = `[${CAT_LABELS[p.category] || p.category}] ${p.name}`;
      chip.addEventListener("click", () => {
        document.getElementById("stPromptInput").value = p.name;
        const slices = Number(document.getElementById("stSliceSlider").value);
        this.editingTarget = null;
        this._renderEditBanner();
        this.generateByPrompt(p.key, slices);
      });
      box.appendChild(chip);
    }
  }

  async generateByPrompt(prompt, slices = 44) {
    const wf = this.getWorldFile();
    if (!wf) return;
    this.diagBox.textContent = "正在以 Rust 體素引擎烘焙 3D 精靈堆疊...";
    try {
      const res = await fetch(`/api/worlds/${encodeURIComponent(wf)}/assets/generate`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt, slices, seed: Math.floor(Math.random() * 9999) }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || "生成失敗");
      await this._setActiveAsset(data.asset, data.diagnostics);
      this.onAssetReady(data.asset, false);
      this.showToast(`已生成「${data.asset.name}」！`);
    } catch (err) {
      this.diagBox.textContent = `錯誤：${err.message}`;
      this.showToast(err.message, true);
    }
  }

  async doCarve() {
    const wf = this.getWorldFile();
    const fileInput = document.getElementById("carveFileInput");
    if (!wf || !fileInput.files?.[0]) {
      this.showToast("請先選擇四視圖 PNG/JPG 圖片！", true);
      return;
    }
    const fd = new FormData();
    fd.append("file", fileInput.files[0]);
    fd.append("title", document.getElementById("carveTitleInput").value.trim() || "四視圖雕刻作品");
    fd.append("category", this.catSelect.value);
    fd.append("slices", document.getElementById("stSliceSlider").value);
    fd.append("round", document.getElementById("carveRoundChk").checked ? "true" : "false");
    this.diagBox.textContent = "正在進行四視圖視覺外殼雕刻...";
    try {
      const res = await fetch(`/api/worlds/${encodeURIComponent(wf)}/assets/carve`, {
        method: "POST",
        body: fd,
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || "雕刻失敗");
      await this._setActiveAsset(data.asset, data.diagnostics);
      this.onAssetReady(data.asset, false);
      this.showToast(`四視圖雕刻完成：「${data.asset.name}」`);
    } catch (err) {
      this.diagBox.textContent = err.message;
      this.showToast(err.message, true);
    }
  }

  async doImportFile() {
    const wf = this.getWorldFile();
    const f = document.getElementById("importAssetFile").files?.[0];
    if (!wf || !f) {
      this.showToast("請選擇要匯入的 .zip、blueprint.json 或 slices.png 檔案！", true);
      return;
    }
    const fd = new FormData();
    fd.append("file", f);
    fd.append("category", document.getElementById("importAssetCat").value);
    fd.append("slices", document.getElementById("stSliceSlider").value);
    try {
      const res = await fetch(`/api/worlds/${encodeURIComponent(wf)}/assets/import`, {
        method: "POST",
        body: fd,
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || "匯入失敗");
      await this._setActiveAsset(data.asset, data.diagnostics);
      this.onAssetReady(data.asset, false);
      this.showToast(`成功匯入「${data.asset.name}」！`);
    } catch (err) {
      this.showToast(err.message, true);
    }
  }

  // ==================== 個人跨世界資產庫 & 本世界資產清單（載入 / 編輯 / 刪除） ====================

  async loadLibrary() {
    const listEl = document.getElementById("libraryList");
    if (!listEl) return;
    listEl.innerHTML = "<div class='hint-text'>讀取個人跨世界資產庫...</div>";
    try {
      const res = await fetch("/api/assets/library");
      const data = await res.json();
      listEl.innerHTML = "";
      for (const item of data.assets || []) {
        listEl.appendChild(this._buildManageCard(item, "library"));
      }
      if (!data.assets?.length) {
        listEl.innerHTML = "<div class='hint-text'>尚無跨世界資產，生成後會自動收錄於此。</div>";
      }
    } catch (err) {
      listEl.innerHTML = `<div class='hint-text'>讀取失敗：${err.message}</div>`;
    }
  }

  renderWorldAssetsList() {
    const listEl = document.getElementById("worldAssetManageList");
    if (!listEl) return;
    const assets = this.getWorldAssets() || [];
    listEl.innerHTML = "";
    if (!assets.length) {
      listEl.innerHTML = "<div class='hint-text'>目前世界尚無資產。</div>";
      return;
    }
    for (const item of assets) {
      listEl.appendChild(this._buildManageCard(item, "world"));
    }
  }

  _buildManageCard(item, scope) {
    const card = document.createElement("div");
    card.className = "lib-item-card";

    const top = document.createElement("div");
    top.className = "lib-item-top";

    const info = document.createElement("div");
    const sourceBadge = item.blueprint_json ? "🧱 積木藍圖" : (item.source_type || "精靈貼圖");
    info.innerHTML = `<strong>${escapeHtml(item.name)}</strong> <small style="color:#94a3b8;">(${CAT_LABELS[item.category] || item.category}・${item.slice_count}層・${escapeHtml(sourceBadge)})</small>`;

    const actions = document.createElement("div");
    actions.className = "lib-item-actions";

    if (scope === "library") {
      const btnLoad = document.createElement("button");
      btnLoad.className = "btn btn-sub";
      btnLoad.textContent = "➕ 載入世界";
      btnLoad.addEventListener("click", async () => {
        const wf = this.getWorldFile();
        const r2 = await fetch(`/api/worlds/${encodeURIComponent(wf)}/assets/import`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ library_asset_id: item.id }),
        });
        const d2 = await r2.json();
        if (r2.ok) {
          await this._setActiveAsset(d2.asset, null);
          this.onAssetReady(d2.asset, false);
          this.renderWorldAssetsList();
          this.showToast(`已從個人庫載入「${d2.asset.name}」！`);
        } else {
          this.showToast(d2.error || "載入失敗", true);
        }
      });
      actions.appendChild(btnLoad);
    }

    // 編輯面板（改名／改分類，或若有藍圖直接進 3D 積木編輯器細調覆寫）
    const inlineEdit = document.createElement("div");
    inlineEdit.className = "lib-inline-edit hidden";
    const nameIn = document.createElement("input");
    nameIn.className = "ctrl-input";
    nameIn.style.flex = "1";
    nameIn.value = item.name;
    const catSel = document.createElement("select");
    catSel.className = "ctrl-select ctrl-compact";
    for (const [k, lbl] of Object.entries(CAT_LABELS)) {
      const o = document.createElement("option");
      o.value = k;
      o.textContent = lbl;
      catSel.appendChild(o);
    }
    catSel.value = item.category || "prop";

    const btnSaveMeta = document.createElement("button");
    btnSaveMeta.className = "btn btn-primary";
    btnSaveMeta.textContent = "💾 儲存改名";
    btnSaveMeta.addEventListener("click", async () => {
      const nextName = nameIn.value.trim();
      if (!nextName) return this.showToast("名稱不可空白！", true);
      const url = scope === "library"
        ? `/api/assets/library/${encodeURIComponent(item.id)}`
        : `/api/worlds/${encodeURIComponent(this.getWorldFile())}/assets/${encodeURIComponent(item.id)}`;
      const res = await fetch(url, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: nextName, category: catSel.value }),
      });
      const d = await res.json();
      if (!res.ok) return this.showToast(d.error || "更新失敗", true);
      if (scope === "library") await this.loadLibrary();
      else {
        await this.onAssetUpdated(scope, d.asset);
        this.renderWorldAssetsList();
      }
      this.showToast(`已更新「${d.asset.name}」！`);
    });

    inlineEdit.append(nameIn, catSel, btnSaveMeta);

    if (item.blueprint_json) {
      const btnOpenBp = document.createElement("button");
      btnOpenBp.className = "btn btn-gold";
      btnOpenBp.textContent = "🧱 進入 3D 積木細調";
      btnOpenBp.addEventListener("click", async () => {
        await this._enterEditAssetBlueprint(item, scope);
      });
      inlineEdit.appendChild(btnOpenBp);
    }

    const btnEdit = document.createElement("button");
    btnEdit.className = "btn btn-sub";
    btnEdit.textContent = "✏️ 編輯";
    btnEdit.addEventListener("click", () => {
      inlineEdit.classList.toggle("hidden");
    });
    actions.appendChild(btnEdit);

    // 刪除按鈕（雙擊或展開內嵌確認，避免誤觸）
    const delConfirmRow = document.createElement("div");
    delConfirmRow.className = "lib-inline-edit hidden";
    const delMsg = document.createElement("span");
    delMsg.style.color = "#fca5a5";
    delMsg.textContent = scope === "world"
      ? "⚠️ 確定刪除？地圖上使用此物件的實體也會一併移除。"
      : "⚠️ 確定從個人跨世界資產庫永久刪除？";
    const btnDelYes = document.createElement("button");
    btnDelYes.className = "btn btn-danger";
    btnDelYes.textContent = "確定刪除";
    btnDelYes.addEventListener("click", async () => {
      const url = scope === "library"
        ? `/api/assets/library/${encodeURIComponent(item.id)}`
        : `/api/worlds/${encodeURIComponent(this.getWorldFile())}/assets/${encodeURIComponent(item.id)}`;
      const res = await fetch(url, { method: "DELETE" });
      const d = await res.json();
      if (!res.ok) return this.showToast(d.error || "刪除失敗", true);
      if (this.editingTarget?.id === item.id) {
        this.editingTarget = null;
        this._renderEditBanner();
      }
      if (scope === "library") {
        await this.loadLibrary();
      } else {
        await this.onAssetDeleted(scope, item.id, d.removed_entity_ids || []);
        this.renderWorldAssetsList();
      }
      this.showToast(`已刪除「${item.name}」！`);
    });
    const btnDelCancel = document.createElement("button");
    btnDelCancel.className = "btn btn-sub";
    btnDelCancel.textContent = "取消";
    btnDelCancel.addEventListener("click", () => delConfirmRow.classList.add("hidden"));
    delConfirmRow.append(delMsg, btnDelYes, btnDelCancel);

    const btnDel = document.createElement("button");
    btnDel.className = "btn btn-danger";
    btnDel.textContent = "🗑️ 刪除";
    btnDel.addEventListener("click", () => {
      delConfirmRow.classList.toggle("hidden");
    });
    actions.appendChild(btnDel);

    top.append(info, actions);
    card.append(top, inlineEdit, delConfirmRow);
    return card;
  }

  async _enterEditAssetBlueprint(item, scope) {
    try {
      this.currentBlueprint = JSON.parse(item.blueprint_json);
      // 同步物件最新名稱至藍圖標題，避免先改名後進細調又被舊 title 覆寫
      if (item.name && typeof this.currentBlueprint === "object") {
        this.currentBlueprint.title = item.name;
      }
    } catch {
      this.showToast("此物件的藍圖資料格式無法解析。", true);
      return;
    }
    this.editingTarget = { scope, id: item.id, name: item.name };
    this.catSelect.value = item.category || "prop";
    this._setUnsavedFlag(false);
    this._renderEditBanner();
    this._renderBlueprintRows();
    this.switchTab("blueprint");
    if (scope === "world") {
      await this._setActiveAsset(item, null, false);
    } else {
      // 僅在記憶體烘焙預覽，絕不重複寫入個人資產庫或世界 DB
      await this.previewCurrentBlueprint();
    }
    this.showToast(`已載入「${item.name}」至 3D 積木編輯器，調整後可覆寫儲存！`);
  }

  _renderEditBanner() {
    const banner = document.getElementById("bpEditBanner");
    if (!banner) return;
    if (!this.editingTarget) {
      banner.classList.add("hidden");
      return;
    }
    banner.classList.remove("hidden");
    document.getElementById("bpEditBannerTitle").textContent = `✏️ 正在細調編輯：「${this.editingTarget.name}」`;
    document.getElementById("bpEditBannerScope").textContent =
      this.editingTarget.scope === "library"
        ? "編輯對象：個人跨世界資產庫（儲存將直接更新個人庫中此物件）"
        : "編輯對象：目前世界資產（儲存將更新世界資產並立即反映在 3D 場景上）";
  }

  _setUnsavedFlag(flag) {
    this.hasUnsavedBlueprintChanges = Boolean(flag);
    const badge = document.getElementById("bpUnsavedBadge");
    if (badge) badge.classList.toggle("hidden", !this.hasUnsavedBlueprintChanges);
    if (this.hasUnsavedBlueprintChanges && this.currentBlueprint) {
      this.btnUse.disabled = false;
    }
  }

  _syncTitleFromInput() {
    if (!this.currentBlueprint) return "";
    const raw = document.getElementById("bpTitleInput")?.value.trim() || "";
    const resolved = raw || this.currentBlueprint.title || "我的積木作品";
    this.currentBlueprint.title = resolved;
    this._syncJsonTextarea();
    return resolved;
  }

  async saveEditedTargetOverwrite() {
    if (!this.editingTarget || !this.currentBlueprint) return false;
    const targetName = this._syncTitleFromInput();
    const { scope, id } = this.editingTarget;
    const url = scope === "library"
      ? `/api/assets/library/${encodeURIComponent(id)}`
      : `/api/worlds/${encodeURIComponent(this.getWorldFile())}/assets/${encodeURIComponent(id)}`;
    this.diagBox.textContent = "正在重烘並覆寫更新原物件...";
    try {
      const res = await fetch(url, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: targetName,
          category: this.catSelect.value,
          default_scale: this.currentAsset?.default_scale || 1.0,
          anim_preset: this.currentAsset?.anim_preset || "static",
          blueprint: this.currentBlueprint,
        }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || "覆寫更新失敗");
      this.editingTarget.name = data.asset.name;
      this._setUnsavedFlag(false);
      this._renderEditBanner();
      if (scope === "world") {
        await this._setActiveAsset(data.asset, data.diagnostics, false);
        await this.onAssetUpdated(scope, data.asset);
        this.renderWorldAssetsList();
      } else {
        await this.loadLibrary();
      }
      this.showToast(`已成功覆寫更新「${data.asset.name}」！`);
      return true;
    } catch (err) {
      this.showToast(err.message, true);
      return false;
    }
  }

  // ==================== 3D 積木藍圖純預覽（不寫入資產庫）＆確認儲存 ====================

  /// 純記憶體即時烘焙預覽：僅更新 360° 轉盤與診斷欄，絕對不寫入世界或個人跨世界資產庫
  async previewCurrentBlueprint() {
    if (!this.currentBlueprint) return;
    const targetName = this._syncTitleFromInput();
    try {
      const res = await fetch("/api/assets/preview-blueprint", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: targetName,
          blueprint: this.currentBlueprint,
          category: this.catSelect.value,
          default_scale: this.currentAsset?.default_scale || 1.0,
          anim_preset: this.currentAsset?.anim_preset || "static",
        }),
      });
      const data = await res.json();
      if (!res.ok) return;
      const loadDataUrl = (u) => new Promise((resolve, reject) => {
        const img = new Image();
        img.onload = () => resolve(img);
        img.onerror = reject;
        img.src = u;
      });
      const [slicesImg, emissiveImg] = await Promise.all([
        loadDataUrl(data.slices_data_url),
        loadDataUrl(data.emissive_data_url),
      ]);
      this.previewImgs = { slicesImg, emissiveImg, count: data.slice_count };
      this.btnUse.disabled = false;
      const d = data.diagnostics || {};
      this.diagBox.innerHTML =
        `<strong>${escapeHtml(data.name || targetName)}</strong>（預覽草稿・尚未存庫）｜尺寸 ${d.slice_width}×${d.slice_height}×${d.slices} 層` +
        (d.voxels ? `｜體素 ${d.voxels} 顆` : "") +
        `｜防閃爍：${d.blink_free ? "✅ 通過" : "⚠️ 含細柱"}`;
    } catch { /* 預覽過程中忽略暫時錯誤 */ }
  }

  /// 明確按下「💾 確認儲存至資產庫」時，才真正寫入 SQLite 世界資產與個人跨世界資產庫
  async bakeCurrentBlueprint() {
    const wf = this.getWorldFile();
    if (!wf || !this.currentBlueprint) return false;
    const targetName = this._syncTitleFromInput();
    this.diagBox.textContent = "正在烘焙並儲存至資產庫...";
    try {
      const updateId = this.editingTarget?.scope === "world" ? this.editingTarget.id : null;
      const res = await fetch(`/api/worlds/${encodeURIComponent(wf)}/assets/blueprint`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: targetName,
          blueprint: this.currentBlueprint,
          category: this.catSelect.value,
          default_scale: this.currentAsset?.default_scale || 1.0,
          anim_preset: this.currentAsset?.anim_preset || "static",
          update_asset_id: updateId,
        }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || "藍圖儲存失敗");
      this._setUnsavedFlag(false);
      await this._setActiveAsset(data.asset, data.diagnostics, false, false);
      if (updateId) {
        await this.onAssetUpdated("world", data.asset);
      } else {
        this.onAssetReady(data.asset, false);
      }
      await this.loadLibrary();
      this.renderWorldAssetsList();
      this.showToast(`已確認儲存「${data.asset.name}」至個人與本世界資產庫！`);
      return true;
    } catch (err) {
      this.showToast(err.message, true);
      return false;
    }
  }

  _schedulePreviewBake() {
    const chk = document.getElementById("bpAutoBakeChk");
    if (!chk?.checked || !this.currentBlueprint) return;
    if (this._autoBakeTimer) clearTimeout(this._autoBakeTimer);
    this._autoBakeTimer = setTimeout(() => {
      // 僅呼叫純記憶體預覽端點，絕不寫入資料庫
      this.previewCurrentBlueprint();
    }, 320);
  }

  async _setActiveAsset(assetMeta, diag, resetEditingTarget = false, keepCurrentBlueprint = false) {
    if (resetEditingTarget) {
      this.editingTarget = null;
      this._renderEditBanner();
    }
    this._setUnsavedFlag(false);
    this.currentAsset = assetMeta;
    this.currentDiag = diag;
    this.catSelect.value = assetMeta.category || "prop";
    this.btnUse.disabled = false;

    if (!keepCurrentBlueprint) {
      if (assetMeta.blueprint_json) {
        try {
          this.currentBlueprint = JSON.parse(assetMeta.blueprint_json);
          if (this.editingTarget && this.editingTarget.id === assetMeta.id && this.editingTarget.name) {
            this.currentBlueprint.title = this.editingTarget.name;
          }
        } catch {
          this.currentBlueprint = null;
        }
      } else if (!this.editingTarget) {
        this.currentBlueprint = null;
      }
      this._renderBlueprintRows();
    }

    const wf = this.getWorldFile();
    const base = `/api/worlds/${encodeURIComponent(wf)}/assets/${encodeURIComponent(assetMeta.id)}`;
    const cacheTag = `${assetMeta.sha256.slice(0, 8)}_${Date.now()}`;
    const loadImg = (u) => new Promise((res, rej) => {
      const img = new Image();
      img.onload = () => res(img);
      img.onerror = rej;
      img.src = `${u}?v=${cacheTag}`;
    });
    const [slicesImg, emissiveImg] = await Promise.all([
      loadImg(`${base}/slices.png`),
      loadImg(`${base}/emissive.png`),
    ]);
    this.previewImgs = { slicesImg, emissiveImg, count: assetMeta.slice_count };

    const d = diag || {
      slices: assetMeta.slice_count,
      slice_width: assetMeta.slice_w,
      slice_height: assetMeta.slice_h,
      blink_free: true,
    };
    this.diagBox.innerHTML =
      `<strong>${escapeHtml(assetMeta.name)}</strong>｜尺寸 ${d.slice_width}×${d.slice_height}×${d.slices} 層` +
      (d.voxels ? `｜體素 ${d.voxels} 顆` : "") +
      `｜防閃爍：${d.blink_free ? "✅ 通過" : "⚠️ 含細柱"}`;
  }

  _addPrim(type) {
    if (!this.currentBlueprint) {
      this.currentBlueprint = {
        title: document.getElementById("bpTitleInput")?.value.trim() || "我的積木作品",
        size: [40, 40, 44],
        primitives: [],
      };
    }
    if (!Array.isArray(this.currentBlueprint.primitives)) {
      this.currentBlueprint.primitives = [];
    }
    const sz = this.currentBlueprint.size || [40, 40, 44];
    const cx = Math.round(sz[0] / 2);
    const cy = Math.round(sz[1] / 2);
    if (type === "box") {
      this.currentBlueprint.primitives.push({
        type: "box", x0: cx - 5, y0: cy - 5, z0: 0, x1: cx + 5, y1: cy + 5, z1: 12,
        color: "blue", material: "stone", emissive: false, mirror_x: false,
      });
    } else if (type === "clear_box") {
      this.currentBlueprint.primitives.push({
        type: "clear_box", x0: cx - 2, y0: cy - 2, z0: 2, x1: cx + 2, y1: cy + 2, z1: 8,
        mirror_x: false,
      });
    } else if (type === "cylinder") {
      this.currentBlueprint.primitives.push({
        type: "cylinder", cx: cx - 0.5, cy: cy - 0.5, z0: 0, z1: 18, rx: 5, ry: 5,
        hollow: false, wall: 1.5, color: "gold", material: "metal", emissive: false, mirror_x: false,
      });
    } else if (type === "cone") {
      this.currentBlueprint.primitives.push({
        type: "cone", cx: cx - 0.5, cy: cy - 0.5, z0: 14, z1: 28, rx0: 7, ry0: 7, rx1: 1, ry1: 1,
        color: "red", material: "roof", emissive: false, mirror_x: false,
      });
    } else if (type === "pyramid") {
      this.currentBlueprint.primitives.push({
        type: "pyramid", cx: cx - 0.5, cy: cy - 0.5, z0: 0, z1: 18, w0: 14, l0: 14, w1: 2, l1: 2,
        stepped: true, steps: 5, color: "stone", material: "stone", emissive: false,
      });
    } else {
      this.currentBlueprint.primitives.push({
        type: "sphere", cx: cx - 0.5, cy: cy - 0.5, cz: 22, rx: 5, ry: 5, rz: 5,
        color: "neon_cyan", material: "crystal", emissive: true, mirror_x: false,
      });
    }
    this._setUnsavedFlag(true);
    this._renderBlueprintRows();
    this._schedulePreviewBake();
  }

  _syncJsonTextarea() {
    const area = document.getElementById("bpJsonEditor");
    if (area && this.currentBlueprint) {
      area.value = JSON.stringify(this.currentBlueprint, null, 2);
    }
  }

  _makeNumField(label, value, step, minVal, maxVal, onChange) {
    const wrap = document.createElement("label");
    wrap.className = "bp-num-field";
    const span = document.createElement("span");
    span.textContent = label;
    const input = document.createElement("input");
    input.type = "number";
    input.step = String(step);
    if (minVal !== null) input.min = String(minVal);
    if (maxVal !== null) input.max = String(maxVal);
    input.value = String(Number.isFinite(value) ? Number(Number(value).toFixed(2)) : 0);
    input.addEventListener("change", () => {
      const v = Number(input.value);
      if (Number.isFinite(v)) {
        onChange(v);
        this._setUnsavedFlag(true);
        this._syncJsonTextarea();
        this._schedulePreviewBake();
      }
    });
    wrap.append(span, input);
    return wrap;
  }

  _renderBlueprintRows() {
    const list = document.getElementById("bpPrimList");
    const area = document.getElementById("bpJsonEditor");
    const titleEl = document.getElementById("bpTitleInput");
    const wEl = document.getElementById("bpSizeW");
    const lEl = document.getElementById("bpSizeL");
    const hEl = document.getElementById("bpSizeH");

    if (!this.currentBlueprint) {
      list.innerHTML = "<div class='hint-text'>目前無可編輯藍圖，可先於「快速造物」生成物件或點上方按鈕新增幾何積木。</div>";
      area.value = "";
      return;
    }

    if (titleEl) titleEl.value = this.currentBlueprint.title || "我的積木作品";
    const sz = this.currentBlueprint.size || [40, 40, 44];
    if (wEl) wEl.value = String(sz[0] ?? 40);
    if (lEl) lEl.value = String(sz[1] ?? 40);
    if (hEl) hEl.value = String(sz[2] ?? 44);
    this._syncJsonTextarea();
    list.innerHTML = "";

    const typeNames = {
      box: "方塊", clear_box: "矩形挖空", cylinder: "圓柱",
      cone: "圓錐", sphere: "球體", pyramid: "角錐",
      arch_cut: "拱門挖空", line: "線段",
    };

    const prims = this.currentBlueprint.primitives || [];
    prims.forEach((p, idx) => {
      const card = document.createElement("div");
      card.className = "bp-prim-card";

      // 上方列：編號形狀 + 顏色/材質/發光/鏡像/複製/上下移動/刪除
      const head = document.createElement("div");
      head.className = "bp-prim-head";

      const titleBadge = document.createElement("strong");
      titleBadge.textContent = `#${idx + 1} ${typeNames[p.type] || p.type}`;

      const rightTools = document.createElement("div");
      rightTools.className = "row-gap";
      rightTools.style.flexWrap = "wrap";

      if (p.type !== "clear_box" && p.type !== "arch_cut") {
        const colSel = document.createElement("select");
        colSel.className = "ctrl-select ctrl-compact";
        if (typeof p.color === "string" && !BP_COLOR_CHOICES.includes(p.color)) {
          const o = document.createElement("option");
          o.value = p.color;
          o.textContent = p.color;
          colSel.appendChild(o);
        }
        BP_COLOR_CHOICES.forEach((c) => {
          const o = document.createElement("option");
          o.value = c;
          o.textContent = c;
          colSel.appendChild(o);
        });
        if (typeof p.color === "string") colSel.value = p.color;
        colSel.addEventListener("change", () => {
          p.color = colSel.value;
          this._setUnsavedFlag(true);
          this._syncJsonTextarea();
          this._schedulePreviewBake();
        });
        rightTools.appendChild(colSel);

        const matSel = document.createElement("select");
        matSel.className = "ctrl-select ctrl-compact";
        BP_MATERIALS.forEach((m) => {
          const o = document.createElement("option");
          o.value = m.value;
          o.textContent = m.label;
          matSel.appendChild(o);
        });
        matSel.value = p.material || "stone";
        matSel.addEventListener("change", () => {
          p.material = matSel.value;
          this._setUnsavedFlag(true);
          this._syncJsonTextarea();
          this._schedulePreviewBake();
        });
        rightTools.appendChild(matSel);

        const emLbl = document.createElement("label");
        emLbl.className = "inline-field";
        const emChk = document.createElement("input");
        emChk.type = "checkbox";
        emChk.checked = Boolean(p.emissive);
        emChk.addEventListener("change", () => {
          p.emissive = emChk.checked;
          this._setUnsavedFlag(true);
          this._syncJsonTextarea();
          this._schedulePreviewBake();
        });
        emLbl.append(emChk, "發光");
        rightTools.appendChild(emLbl);
      }

      const mirLbl = document.createElement("label");
      mirLbl.className = "inline-field";
      const mirChk = document.createElement("input");
      mirChk.type = "checkbox";
      mirChk.checked = Boolean(p.mirror_x);
      mirChk.addEventListener("change", () => {
        p.mirror_x = mirChk.checked;
        this._setUnsavedFlag(true);
        this._syncJsonTextarea();
        this._schedulePreviewBake();
      });
      mirLbl.append(mirChk, "X鏡像");
      rightTools.appendChild(mirLbl);

      const dupBtn = document.createElement("button");
      dupBtn.className = "btn-mini";
      dupBtn.title = "複製這塊積木";
      dupBtn.textContent = "📑";
      dupBtn.addEventListener("click", () => {
        prims.splice(idx + 1, 0, JSON.parse(JSON.stringify(p)));
        this._setUnsavedFlag(true);
        this._renderBlueprintRows();
        this._schedulePreviewBake();
      });

      const upBtn = document.createElement("button");
      upBtn.className = "btn-mini";
      upBtn.textContent = "↑";
      upBtn.disabled = idx === 0;
      upBtn.addEventListener("click", () => {
        [prims[idx - 1], prims[idx]] = [prims[idx], prims[idx - 1]];
        this._setUnsavedFlag(true);
        this._renderBlueprintRows();
        this._schedulePreviewBake();
      });

      const dnBtn = document.createElement("button");
      dnBtn.className = "btn-mini";
      dnBtn.textContent = "↓";
      dnBtn.disabled = idx === prims.length - 1;
      dnBtn.addEventListener("click", () => {
        [prims[idx + 1], prims[idx]] = [prims[idx], prims[idx + 1]];
        this._setUnsavedFlag(true);
        this._renderBlueprintRows();
        this._schedulePreviewBake();
      });

      const delBtn = document.createElement("button");
      delBtn.className = "btn btn-danger";
      delBtn.style.padding = "2px 6px";
      delBtn.textContent = "✕";
      delBtn.addEventListener("click", () => {
        prims.splice(idx, 1);
        this._setUnsavedFlag(true);
        this._renderBlueprintRows();
        this._schedulePreviewBake();
      });

      rightTools.append(dupBtn, upBtn, dnBtn, delBtn);
      head.append(titleBadge, rightTools);
      card.appendChild(head);

      // 下方列：依原語類型顯示位置 (X/Y/Z) 與 長寬高/半徑 細部調教欄位
      const fields = document.createElement("div");
      fields.className = "bp-prim-fields";

      if (p.type === "box" || p.type === "clear_box") {
        const w = Math.max(1, (p.x1 ?? 10) - (p.x0 ?? 0));
        const d = Math.max(1, (p.y1 ?? 10) - (p.y0 ?? 0));
        const h = Math.max(1, (p.z1 ?? 10) - (p.z0 ?? 0));
        fields.append(
          this._makeNumField("位置 X", p.x0 ?? 0, 1, 0, 96, (v) => {
            const span = Math.max(1, (p.x1 ?? 0) - (p.x0 ?? 0));
            p.x0 = Math.round(v);
            p.x1 = Math.round(v + span);
          }),
          this._makeNumField("位置 Y", p.y0 ?? 0, 1, 0, 96, (v) => {
            const span = Math.max(1, (p.y1 ?? 0) - (p.y0 ?? 0));
            p.y0 = Math.round(v);
            p.y1 = Math.round(v + span);
          }),
          this._makeNumField("底高 Z", p.z0 ?? 0, 1, 0, 80, (v) => {
            const span = Math.max(1, (p.z1 ?? 0) - (p.z0 ?? 0));
            p.z0 = Math.round(v);
            p.z1 = Math.round(v + span);
          }),
          this._makeNumField("長 W", w, 1, 1, 96, (v) => {
            p.x1 = Math.round((p.x0 ?? 0) + Math.max(1, v));
          }),
          this._makeNumField("寬 D", d, 1, 1, 96, (v) => {
            p.y1 = Math.round((p.y0 ?? 0) + Math.max(1, v));
          }),
          this._makeNumField("高 H", h, 1, 1, 80, (v) => {
            p.z1 = Math.round((p.z0 ?? 0) + Math.max(1, v));
          }),
        );
      } else if (p.type === "cylinder") {
        const h = Math.max(1, (p.z1 ?? 12) - (p.z0 ?? 0));
        fields.append(
          this._makeNumField("中心 X", p.cx ?? 20, 0.5, 0, 96, (v) => { p.cx = v; }),
          this._makeNumField("中心 Y", p.cy ?? 20, 0.5, 0, 96, (v) => { p.cy = v; }),
          this._makeNumField("底高 Z", p.z0 ?? 0, 1, 0, 80, (v) => {
            const span = Math.max(1, (p.z1 ?? 12) - (p.z0 ?? 0));
            p.z0 = Math.round(v);
            p.z1 = Math.round(v + span);
          }),
          this._makeNumField("半徑 X", p.rx ?? 5, 0.5, 1, 48, (v) => { p.rx = v; }),
          this._makeNumField("半徑 Y", p.ry ?? p.rx ?? 5, 0.5, 1, 48, (v) => { p.ry = v; }),
          this._makeNumField("高 H", h, 1, 1, 80, (v) => { p.z1 = Math.round((p.z0 ?? 0) + Math.max(1, v)); }),
          this._makeNumField("壁厚", p.wall ?? 1.5, 0.5, 0.5, 16, (v) => { p.wall = v; }),
        );
      } else if (p.type === "sphere") {
        fields.append(
          this._makeNumField("中心 X", p.cx ?? 20, 0.5, 0, 96, (v) => { p.cx = v; }),
          this._makeNumField("中心 Y", p.cy ?? 20, 0.5, 0, 96, (v) => { p.cy = v; }),
          this._makeNumField("中心 Z", p.cz ?? 20, 0.5, 0, 80, (v) => { p.cz = v; }),
          this._makeNumField("半徑 X", p.rx ?? 5, 0.5, 1, 48, (v) => { p.rx = v; }),
          this._makeNumField("半徑 Y", p.ry ?? 5, 0.5, 1, 48, (v) => { p.ry = v; }),
          this._makeNumField("半徑 Z", p.rz ?? 5, 0.5, 1, 48, (v) => { p.rz = v; }),
        );
      } else if (p.type === "cone") {
        const h = Math.max(1, (p.z1 ?? 20) - (p.z0 ?? 0));
        fields.append(
          this._makeNumField("中心 X", p.cx ?? 20, 0.5, 0, 96, (v) => { p.cx = v; }),
          this._makeNumField("中心 Y", p.cy ?? 20, 0.5, 0, 96, (v) => { p.cy = v; }),
          this._makeNumField("底高 Z", p.z0 ?? 0, 1, 0, 80, (v) => {
            const span = Math.max(1, (p.z1 ?? 20) - (p.z0 ?? 0));
            p.z0 = Math.round(v);
            p.z1 = Math.round(v + span);
          }),
          this._makeNumField("底半徑", p.rx0 ?? 7, 0.5, 1, 48, (v) => { p.rx0 = v; p.ry0 = v; }),
          this._makeNumField("頂半徑", p.rx1 ?? 1, 0.5, 0.5, 48, (v) => { p.rx1 = v; p.ry1 = v; }),
          this._makeNumField("高 H", h, 1, 1, 80, (v) => { p.z1 = Math.round((p.z0 ?? 0) + Math.max(1, v)); }),
        );
      } else if (p.type === "pyramid") {
        const h = Math.max(1, (p.z1 ?? 16) - (p.z0 ?? 0));
        fields.append(
          this._makeNumField("中心 X", p.cx ?? 20, 0.5, 0, 96, (v) => { p.cx = v; }),
          this._makeNumField("中心 Y", p.cy ?? 20, 0.5, 0, 96, (v) => { p.cy = v; }),
          this._makeNumField("底高 Z", p.z0 ?? 0, 1, 0, 80, (v) => {
            const span = Math.max(1, (p.z1 ?? 16) - (p.z0 ?? 0));
            p.z0 = Math.round(v);
            p.z1 = Math.round(v + span);
          }),
          this._makeNumField("底長 W", p.w0 ?? 14, 1, 2, 96, (v) => { p.w0 = v; }),
          this._makeNumField("底寬 D", p.l0 ?? 14, 1, 2, 96, (v) => { p.l0 = v; }),
          this._makeNumField("高 H", h, 1, 1, 80, (v) => { p.z1 = Math.round((p.z0 ?? 0) + Math.max(1, v)); }),
        );
      } else if (p.type === "arch_cut") {
        const h = Math.max(1, (p.z1 ?? 14) - (p.z0 ?? 0));
        fields.append(
          this._makeNumField("中心 X", p.cx ?? 20, 0.5, 0, 96, (v) => { p.cx = v; }),
          this._makeNumField("中心 Y", p.cy ?? 20, 0.5, 0, 96, (v) => { p.cy = v; }),
          this._makeNumField("底高 Z", p.z0 ?? 0, 1, 0, 80, (v) => {
            const span = Math.max(1, (p.z1 ?? 14) - (p.z0 ?? 0));
            p.z0 = Math.round(v);
            p.z1 = Math.round(v + span);
          }),
          this._makeNumField("半跨距", p.half_span ?? 4, 0.5, 1, 40, (v) => { p.half_span = v; }),
          this._makeNumField("深度", p.depth ?? 6, 0.5, 1, 40, (v) => { p.depth = v; }),
          this._makeNumField("高 H", h, 1, 1, 80, (v) => { p.z1 = Math.round((p.z0 ?? 0) + Math.max(1, v)); }),
        );
      } else if (p.type === "line") {
        const p0 = p.p0 || [10, 10, 0];
        const p1 = p.p1 || [20, 20, 10];
        p.p0 = p0;
        p.p1 = p1;
        fields.append(
          this._makeNumField("起 X", p0[0], 1, 0, 96, (v) => { p0[0] = v; }),
          this._makeNumField("起 Y", p0[1], 1, 0, 96, (v) => { p0[1] = v; }),
          this._makeNumField("起 Z", p0[2], 1, 0, 80, (v) => { p0[2] = v; }),
          this._makeNumField("終 X", p1[0], 1, 0, 96, (v) => { p1[0] = v; }),
          this._makeNumField("終 Y", p1[1], 1, 0, 96, (v) => { p1[1] = v; }),
          this._makeNumField("終 Z", p1[2], 1, 0, 80, (v) => { p1[2] = v; }),
          this._makeNumField("半徑", p.radius ?? 1.5, 0.5, 1, 20, (v) => { p.radius = v; }),
        );
      }

      if (fields.childElementCount > 0) {
        card.appendChild(fields);
      }
      list.appendChild(card);
    });
  }

  // ==================== 360° 預覽轉盤 ====================

  _startPreviewLoop() {
    const tick = () => {
      if (!this.modal.classList.contains("hidden")) {
        if (!this.dragging) this.turntableAngle = (this.turntableAngle + 0.65) % 360;
        this._drawTurntable();
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }

  _drawTurntable() {
    const ctx = this.pctx;
    const W = this.previewCanvas.width;
    const H = this.previewCanvas.height;
    const bgMap = {
      day: ["#bae6fd", "#e0f2fe"],
      sunset: ["#7c2d12", "#f59e0b"],
      night: ["#090d16", "#1e293b"],
    };
    const [topC, botC] = bgMap[this.turntableLight] || bgMap.day;
    const grad = ctx.createLinearGradient(0, 0, 0, H);
    grad.addColorStop(0, topC);
    grad.addColorStop(1, botC);
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, W, H);

    ctx.fillStyle = this.turntableLight === "night" ? "rgba(30,41,59,0.85)" : "rgba(255,255,255,0.45)";
    ctx.beginPath();
    ctx.ellipse(W * 0.5, H * 0.78, W * 0.34, H * 0.11, 0, 0, Math.PI * 2);
    ctx.fill();

    if (!this.previewImgs) return;
    const { slicesImg, emissiveImg, count } = this.previewImgs;
    const sw = slicesImg.width / Math.max(count, 1);
    const sh = slicesImg.height;
    const maxDim = Math.max(sw, sh, count * 1.05);
    const scale = (W * 0.56) / Math.max(maxDim, 1);
    const rad = (this.turntableAngle * Math.PI) / 180;
    const cx = W * 0.5;
    const baseY = H * 0.76;

    ctx.imageSmoothingEnabled = false;
    for (let z = 0; z < count; z++) {
      ctx.save();
      ctx.translate(cx, baseY - z * scale * 0.92);
      ctx.scale(scale, scale * 0.72);
      ctx.rotate(rad);
      if (this.turntableLight === "night") {
        ctx.filter = "brightness(0.45)";
      } else if (this.turntableLight === "sunset") {
        ctx.filter = "sepia(0.25) brightness(0.92)";
      } else {
        ctx.filter = "none";
      }
      ctx.drawImage(slicesImg, z * sw, 0, sw, sh, -sw * 0.5, -sh * 0.5, sw, sh);
      ctx.filter = "none";
      if (this.turntableLight === "night") {
        ctx.drawImage(emissiveImg, z * sw, 0, sw, sh, -sw * 0.5, -sh * 0.5, sw, sh);
      }
      ctx.restore();
    }
  }
}

export function escapeHtml(s) {
  return String(s ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

//! Web Audio API 程序化音效＋多風格多軌合成背景音樂＋自訂音檔播放器。
//! 支援 5 種拍號、編制與音色截然不同的現場合成配樂，並含防破音動態壓縮總線。

export const BGM_STYLES = {
  kingdom: {
    title: "🏰 奇幻王國・銅管定音鼓進行曲",
    meter: "4/4 拍",
    bpm: 112,
    instruments: "雙振盪銅管號角、豎琴琶音、定音鼓與進行軍鼓",
    stepMs: 134, // 16 分音符，32 步 = 2 小節循環
    totalSteps: 32,
  },
  city: {
    title: "🏙️ 霓虹都會・Synthwave Funk 律動",
    meter: "4/4 拍",
    bpm: 124,
    instruments: "共振濾波 Funk 貝斯、電鋼琴七和弦刺音與電子鼓組",
    stepMs: 121,
    totalSteps: 32,
  },
  farm: {
    title: "🌾 陽光農莊・3/4 田園吉他華爾滋",
    meter: "3/4 拍（咚-恰-恰）",
    bpm: 108,
    instruments: "顫音田園木笛、原聲木吉他三拍撥弦與輕沙鈴",
    stepMs: 185, // 每拍細分為 3 步 * 2 = 6 步一小節，共 24 步（4 小節）
    totalSteps: 24,
  },
  campus: {
    title: "🌸 青春校園・Chiptune 鐘琴輕快流行",
    meter: "4/4 拍",
    bpm: 140,
    instruments: "雙層復古方波＋亮麗鐘琴、彈跳八度貝斯與流行鼓",
    stepMs: 107,
    totalSteps: 32,
  },
  indoor: {
    title: "🛋️ 室內沙龍・Bossa Nova 爵士咖啡館",
    meter: "4/4 拍",
    bpm: 84,
    instruments: "Rhodes 爵士延伸九和弦、低音提琴行走貝斯與 Clave 邊擊",
    stepMs: 178,
    totalSteps: 32,
  },
  off: {
    title: "🔇 關閉背景音樂",
    meter: "-",
    bpm: 0,
    instruments: "僅保留環境與物件互動音效",
    stepMs: 250,
    totalSteps: 1,
  },
};

class SoundEngine {
  constructor() {
    this.ctx = null;
    this.masterGain = null;
    this.bgmGain = null;
    this.noiseBuf = null;
    this.muted = false;

    this.bgmPreset = "kingdom";
    this.bgmVolume = 0.45;
    this.hasCustomBgm = false;
    this.customBgmUrl = null;
    this.audioEl = null;

    this.bgmTimer = null;
    this.bgmStep = 0;
    this.unlocked = false;
  }

  _ensureCtx() {
    if (this.muted) return null;
    if (!this.ctx) {
      const Ctx = window.AudioContext || window.webkitAudioContext;
      if (!Ctx) return null;
      this.ctx = new Ctx();

      // 主總線加入壓縮器避免多軌和弦與鼓組重疊時破音失真
      const comp = this.ctx.createDynamicsCompressor();
      comp.threshold.value = -14;
      comp.knee.value = 18;
      comp.ratio.value = 4;
      comp.attack.value = 0.005;
      comp.release.value = 0.16;

      this.masterGain = this.ctx.createGain();
      this.masterGain.gain.value = 0.92;
      comp.connect(this.masterGain);
      this.masterGain.connect(this.ctx.destination);

      this.bgmGain = this.ctx.createGain();
      this.bgmGain.gain.value = this.bgmVolume * 0.45;
      this.bgmGain.connect(comp);
      this._compressor = comp;

      // 預先建立共用 1 秒白噪音緩衝供軍鼓／沙鈴／Hi-hat 取樣
      const len = Math.floor(this.ctx.sampleRate * 1.0);
      const buf = this.ctx.createBuffer(1, len, this.ctx.sampleRate);
      const ch = buf.getChannelData(0);
      for (let i = 0; i < len; i++) {
        ch[i] = Math.random() * 2 - 1;
      }
      this.noiseBuf = buf;
    }
    if (this.ctx.state === "suspended") {
      this.ctx.resume().catch(() => {});
    }
    return this.ctx;
  }

  /// 於首次使用者手勢時解鎖瀏覽器音訊播放限制並自動開始背景音樂
  unlockAudio() {
    this.unlocked = true;
    if (!this.muted) {
      this._ensureCtx();
      this._syncBgmPlayback();
    }
  }

  toggleMute() {
    this.muted = !this.muted;
    if (this.bgmGain && this.ctx) {
      this.bgmGain.gain.setTargetAtTime(
        this.muted ? 0 : this.bgmVolume * 0.45,
        this.ctx.currentTime,
        0.04,
      );
    }
    if (this.audioEl) {
      this.audioEl.muted = this.muted;
      if (!this.muted && this.hasCustomBgm) {
        this.audioEl.play().catch(() => {});
      }
    }
    this._syncBgmPlayback();
    return this.muted;
  }

  configureBgm({ preset, volume, hasCustom, customUrl }) {
    const presetChanged = preset !== undefined && preset !== this.bgmPreset;
    if (preset !== undefined) this.bgmPreset = preset || "kingdom";
    if (volume !== undefined) this.bgmVolume = Math.max(0, Math.min(1, Number(volume)));
    if (hasCustom !== undefined) this.hasCustomBgm = Boolean(hasCustom);
    if (customUrl !== undefined) this.customBgmUrl = customUrl;
    if (presetChanged) this.bgmStep = 0;

    if (this.bgmGain && this.ctx) {
      this.bgmGain.gain.setTargetAtTime(
        this.muted ? 0 : this.bgmVolume * 0.45,
        this.ctx.currentTime,
        0.05,
      );
    }
    if (this.audioEl) {
      this.audioEl.volume = this.bgmVolume;
      this.audioEl.muted = this.muted;
    }
    this._syncBgmPlayback();
  }

  setBgmVolume(vol) {
    this.bgmVolume = Math.max(0, Math.min(1, Number(vol)));
    if (this.bgmGain && this.ctx && !this.muted) {
      this.bgmGain.gain.setTargetAtTime(this.bgmVolume * 0.45, this.ctx.currentTime, 0.04);
    }
    if (this.audioEl) {
      this.audioEl.volume = this.bgmVolume;
    }
  }

  _stopSynthBgm() {
    if (this.bgmTimer) {
      clearInterval(this.bgmTimer);
      this.bgmTimer = null;
    }
  }

  _stopCustomAudio() {
    if (this.audioEl) {
      this.audioEl.pause();
    }
  }

  _syncBgmPlayback() {
    if (this.muted || (this.bgmPreset === "off" && !this.hasCustomBgm)) {
      this._stopSynthBgm();
      this._stopCustomAudio();
      return;
    }

    if (this.hasCustomBgm && this.customBgmUrl) {
      this._stopSynthBgm();
      if (!this.audioEl) {
        this.audioEl = new Audio();
        this.audioEl.loop = true;
      }
      if (this.audioEl.datasetSrc !== this.customBgmUrl) {
        this.audioEl.src = this.customBgmUrl;
        this.audioEl.datasetSrc = this.customBgmUrl;
      }
      this.audioEl.volume = this.bgmVolume;
      this.audioEl.muted = this.muted;
      if (this.unlocked) {
        this.audioEl.play().catch(() => {});
      }
      return;
    }

    this._stopCustomAudio();
    if (this.bgmPreset === "off") {
      this._stopSynthBgm();
      return;
    }

    this._stopSynthBgm();
    const style = BGM_STYLES[this.bgmPreset] || BGM_STYLES.kingdom;
    if (!this.unlocked) return;

    this.bgmTimer = setInterval(() => {
      if (this.muted || this.bgmPreset === "off" || (this.hasCustomBgm && this.customBgmUrl)) return;
      const ctx = this._ensureCtx();
      if (!ctx || !this.bgmGain) return;

      const step = this.bgmStep % style.totalSteps;
      this.bgmStep++;
      const now = ctx.currentTime + 0.01;
      this._dispatchStep(ctx, this.bgmPreset, step, now);
    }, style.stepMs);
  }

  _dispatchStep(ctx, preset, step, t) {
    if (preset === "kingdom") {
      this._stepKingdom(ctx, step, t);
    } else if (preset === "city") {
      this._stepCity(ctx, step, t);
    } else if (preset === "farm") {
      this._stepFarmWaltz(ctx, step, t);
    } else if (preset === "campus") {
      this._stepCampus(ctx, step, t);
    } else if (preset === "indoor") {
      this._stepIndoorBossa(ctx, step, t);
    }
  }

  // ---------- 1. 王國：4/4 銅管號角 ＋ 定音鼓 ＋ 豎琴琶音進行曲（C 大調）----------
  _stepKingdom(ctx, step, t) {
    // 定音鼓與進行軍鼓（小軍鼓滾奏節奏）
    if (step % 8 === 0) this._playTimpani(ctx, step < 16 ? 130.81 : 98.0, t, 0.28);
    if (step === 12 || step === 28) this._playTimpani(ctx, 196.0, t, 0.22);
    if (step % 8 === 4 || step === 14 || step === 30) this._playSnare(ctx, t, 0.08, 0.09);
    if (step === 15 || step === 31) this._playSnare(ctx, t, 0.05, 0.055);

    // 進行曲銅管號角（五度、八度莊嚴跳進）
    const fanfare = {
      0: [261.63, 0.24], 2: [392.0, 0.2], 4: [523.25, 0.36],
      8: [493.88, 0.16], 10: [440.0, 0.16], 12: [392.0, 0.36],
      16: [349.23, 0.24], 18: [440.0, 0.2], 20: [523.25, 0.36],
      24: [587.33, 0.22], 26: [523.25, 0.18], 28: [392.0, 0.42],
    };
    if (fanfare[step]) {
      const [freq, dur] = fanfare[step];
      this._playBrass(ctx, freq, t, dur, 0.16);
    }

    // 豎琴十六分伴奏琶音（輕柔點綴）
    const harpChord = step < 8 ? [261.63, 329.63, 392.0, 523.25]
      : step < 16 ? [196.0, 246.94, 293.66, 392.0]
      : step < 24 ? [174.61, 220.0, 261.63, 349.23]
      : [196.0, 246.94, 293.66, 392.0];
    if (step % 2 === 1) {
      this._playPluck(ctx, harpChord[Math.floor(step / 2) % 4], t, 0.26, 0.085);
    }

    // 厚實低音進行線
    const bassLine = { 0: 130.81, 4: 130.81, 8: 98.0, 12: 123.47, 16: 87.31, 20: 110.0, 24: 98.0, 28: 98.0 };
    if (bassLine[step]) {
      this._playBass(ctx, bassLine[step], t, 0.28, "march", 0.24);
    }
  }

  // ---------- 2. 城市：4/4 Synthwave Funk 律動（A 小調）----------
  _stepCity(ctx, step, t) {
    // 強烈四拍 Disco/Synthwave 底鼓 ＋ 2/4 拍清脆電子小鼓 ＋ 十六分閉音鈸
    if (step % 4 === 0) this._playKick(ctx, t, 118, 0.34);
    if (step % 8 === 4) this._playSnare(ctx, t, 0.14, 0.15);
    this._playHat(ctx, t, step % 2 === 0 ? 0.055 : 0.032, step % 4 === 2 ? 0.07 : 0.03);

    // 切分 Funk 濾波滑音貝斯
    const bassPattern = {
      0: 110.0, 3: 110.0, 6: 130.81, 7: 146.83,
      8: 87.31, 10: 87.31, 14: 110.0,
      16: 73.42, 19: 73.42, 22: 98.0,
      24: 82.41, 27: 98.0, 28: 123.47, 30: 110.0,
    };
    if (bassPattern[step]) {
      this._playBass(ctx, bassPattern[step], t, 0.18, "funk", 0.28);
    }

    // 電鋼琴小七／九和弦切分刺音 (Stab)
    if (step === 2 || step === 10 || step === 18 || step === 26 || step === 29) {
      const chords = step < 8 ? [220, 261.63, 329.63, 392.0]
        : step < 16 ? [174.61, 220, 261.63, 329.63]
        : step < 24 ? [146.83, 174.61, 220, 261.63]
        : [164.81, 196.0, 246.94, 293.66];
      this._playRhodesChord(ctx, chords, t, 0.14, 0.11);
    }

    // 都會霓虹合成主音（切分炫技樂句）
    const lead = {
      0: 659.25, 2: 587.33, 5: 523.25, 8: 440.0,
      11: 523.25, 13: 587.33, 16: 659.25, 18: 783.99,
      21: 659.25, 24: 587.33, 27: 493.88, 30: 440.0,
    };
    if (lead[step]) {
      this._playChiptune(ctx, lead[step], t, 0.16, 0.11, "sawtooth", 2400);
    }
  }

  // ---------- 3. 農莊：3/4 拍田園吉他華爾滋（G 大調：明顯 1-2-3 咚恰恰）----------
  _stepFarmWaltz(ctx, step, t) {
    // 每 6 步為一小節 3 拍（step%6: 0=第1拍, 2=第2拍, 4=第3拍）
    const bar = Math.floor(step / 6);
    const beatInBar = step % 6;

    const rootBass = [98.0, 130.81, 146.83, 98.0][bar % 4]; // G2 - C3 - D3 - G2
    const chordGuitar = [
      [196.0, 246.94, 293.66], // G
      [261.63, 329.63, 392.0], // C
      [293.66, 369.99, 440.0], // D
      [196.0, 246.94, 293.66], // G
    ][bar % 4];

    if (beatInBar === 0) {
      // 第一拍：溫暖木吉他低音＋大沙鈴
      this._playPluck(ctx, rootBass, t, 0.42, 0.25);
      this._playShaker(ctx, t, 0.06);
    } else if (beatInBar === 2 || beatInBar === 4) {
      // 第 2、3 拍：經典「恰、恰」吉他和弦刷弦
      chordGuitar.forEach((f, idx) => {
        this._playPluck(ctx, f, t + idx * 0.015, 0.2, 0.08);
      });
      this._playShaker(ctx, t, 0.032);
    }

    // 悠揚抖動長笛（五聲牧歌旋律）
    const flute = {
      0: [587.33, 0.34], 2: [659.25, 0.18], 3: [783.99, 0.3],
      6: [659.25, 0.34], 8: [587.33, 0.18], 10: [523.25, 0.34],
      12: [587.33, 0.34], 14: [493.88, 0.22], 16: [440.0, 0.32],
      18: [392.0, 0.36], 21: [493.88, 0.22], 22: [587.33, 0.26],
    };
    if (flute[step]) {
      const [f, d] = flute[step];
      this._playFlute(ctx, f, t, d, 0.15);
    }
  }

  // ---------- 4. 校園：4/4 拍 Chiptune ＋ 鐘琴活力流行（F 大調，140 BPM）----------
  _stepCampus(ctx, step, t) {
    // 流行青春四拍鼓
    if ([0, 6, 8, 14, 16, 22, 24, 30].includes(step)) this._playKick(ctx, t, 130, 0.28);
    if (step % 8 === 4) this._playSnare(ctx, t, 0.12, 0.11);
    if (step % 2 === 0) this._playHat(ctx, t, 0.04, 0.04);

    // 活力八度跳躍貝斯（F -> G -> Em -> Am）
    const roots = [87.31, 98.0, 82.41, 110.0];
    const root = roots[Math.floor(step / 8) % 4];
    if (step % 2 === 0) {
      const freq = step % 4 === 2 ? root * 2 : root;
      this._playBass(ctx, freq, t, 0.14, "pop", 0.22);
    }

    // 雙層亮麗主音：方波 + 閃亮鋼片鐘琴
    const melody = {
      0: 698.46, 2: 783.99, 4: 880.0, 6: 1046.5,
      8: 987.77, 10: 880.0, 12: 783.99,
      16: 659.25, 18: 783.99, 20: 880.0, 22: 783.99,
      24: 698.46, 26: 659.25, 28: 587.33, 30: 659.25,
    };
    if (melody[step]) {
      const f = melody[step];
      this._playChiptune(ctx, f, t, 0.16, 0.095, "square", 3200);
      this._playBell(ctx, f * 2, t, 0.24, 0.065);
    }
  }

  // ---------- 5. 室內：4/4 慢板 Bossa Nova 爵士咖啡廳（84 BPM）----------
  _stepIndoorBossa(ctx, step, t) {
    // 典型 Bossa Nova 3-2 Clave 邊擊（Rimshot）＋ 超細沙鈴 ＋ 柔和心跳底鼓
    const clave = [0, 3, 6, 10, 12, 16, 19, 22, 26, 28];
    if (clave.includes(step)) this._playRim(ctx, t, 0.09);
    if (step % 2 === 0) this._playShaker(ctx, t, 0.028);
    if (step % 8 === 0 || step % 8 === 3) this._playKick(ctx, t, 88, 0.18);

    // Rhodes 爵士九和弦／十三和弦：Cmaj9 -> Am9 -> Dm9 -> G13
    const progression = [
      { root: 130.81, fifth: 98.0, chord: [261.63, 329.63, 392.0, 493.88, 587.33] },
      { root: 110.0, fifth: 164.81, chord: [220.0, 261.63, 329.63, 392.0, 493.88] },
      { root: 146.83, fifth: 110.0, chord: [293.66, 349.23, 440.0, 523.25, 659.25] },
      { root: 98.0, fifth: 146.83, chord: [246.94, 329.63, 349.23, 440.0, 587.33] },
    ];
    const bar = progression[Math.floor(step / 8) % 4];
    const local = step % 8;

    // 溫暖低音提琴 Bossa 步進
    if (local === 0) this._playBass(ctx, bar.root, t, 0.42, "jazz", 0.22);
    else if (local === 3) this._playBass(ctx, bar.fifth, t, 0.28, "jazz", 0.17);
    else if (local === 6) this._playBass(ctx, bar.root * 1.059, t, 0.18, "jazz", 0.14);

    // Rhodes 切分暖和弦
    if (local === 0 || local === 5) {
      this._playRhodesChord(ctx, bar.chord, t, local === 0 ? 0.65 : 0.45, 0.075);
    }

    // 柔和長笛偶爾吹奏爵士即興裝飾音
    const solo = { 2: 783.99, 4: 659.25, 11: 587.33, 14: 659.25, 18: 698.46, 22: 587.33, 27: 493.88 };
    if (solo[step]) {
      this._playFlute(ctx, solo[step], t, 0.34, 0.085);
    }
  }

  // ---------- 合成樂器與鼓組實作 ----------

  _playBrass(ctx, freq, t, dur, vol) {
    // 雙微失諧鋸齒波模擬銅管齊奏 ＋ 低通濾波展開
    const filter = ctx.createBiquadFilter();
    filter.type = "lowpass";
    filter.frequency.setValueAtTime(freq * 1.4, t);
    filter.frequency.exponentialRampToValueAtTime(Math.min(4800, freq * 5.2), t + 0.045);
    filter.frequency.exponentialRampToValueAtTime(freq * 2.2, t + dur);

    const g = ctx.createGain();
    g.gain.setValueAtTime(0.001, t);
    g.gain.linearRampToValueAtTime(vol, t + 0.035);
    g.gain.exponentialRampToValueAtTime(0.0005, t + dur);

    [-6, 6].forEach((detune) => {
      const osc = ctx.createOscillator();
      osc.type = "sawtooth";
      osc.frequency.setValueAtTime(freq, t);
      osc.detune.setValueAtTime(detune, t);
      osc.connect(filter);
      osc.start(t);
      osc.stop(t + dur + 0.03);
    });
    filter.connect(g);
    g.connect(this.bgmGain);
  }

  _playPluck(ctx, freq, t, dur, vol) {
    const osc = ctx.createOscillator();
    const osc2 = ctx.createOscillator();
    const filter = ctx.createBiquadFilter();
    const g = ctx.createGain();
    osc.type = "triangle";
    osc.frequency.setValueAtTime(freq, t);
    osc2.type = "sine";
    osc2.frequency.setValueAtTime(freq * 2, t);
    filter.type = "lowpass";
    filter.frequency.setValueAtTime(3200, t);
    filter.frequency.exponentialRampToValueAtTime(620, t + dur);

    g.gain.setValueAtTime(0.001, t);
    g.gain.linearRampToValueAtTime(vol, t + 0.008);
    g.gain.exponentialRampToValueAtTime(0.0005, t + dur);

    const g2 = ctx.createGain();
    g2.gain.value = 0.25;
    osc.connect(filter);
    osc2.connect(g2);
    g2.connect(filter);
    filter.connect(g);
    g.connect(this.bgmGain);
    osc.start(t);
    osc2.start(t);
    osc.stop(t + dur + 0.02);
    osc2.stop(t + dur + 0.02);
  }

  _playRhodesChord(ctx, freqs, t, dur, volPerNote) {
    freqs.forEach((freq, i) => {
      const osc = ctx.createOscillator();
      const g = ctx.createGain();
      osc.type = "sine";
      osc.frequency.setValueAtTime(freq, t + i * 0.012);
      g.gain.setValueAtTime(0.001, t + i * 0.012);
      g.gain.linearRampToValueAtTime(volPerNote, t + i * 0.012 + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0004, t + i * 0.012 + dur);
      osc.connect(g);
      g.connect(this.bgmGain);
      osc.start(t + i * 0.012);
      osc.stop(t + i * 0.012 + dur + 0.03);
    });
  }

  _playFlute(ctx, freq, t, dur, vol) {
    const osc = ctx.createOscillator();
    const lfo = ctx.createOscillator();
    const lfoGain = ctx.createGain();
    const g = ctx.createGain();

    osc.type = "sine";
    osc.frequency.setValueAtTime(freq, t);
    lfo.frequency.setValueAtTime(5.2, t);
    lfoGain.gain.setValueAtTime(freq * 0.014, t);
    lfo.connect(lfoGain);
    lfoGain.connect(osc.frequency);

    g.gain.setValueAtTime(0.001, t);
    g.gain.linearRampToValueAtTime(vol, t + 0.055);
    g.gain.exponentialRampToValueAtTime(0.0005, t + dur);

    osc.connect(g);
    g.connect(this.bgmGain);
    osc.start(t);
    lfo.start(t);
    osc.stop(t + dur + 0.03);
    lfo.stop(t + dur + 0.03);
  }

  _playChiptune(ctx, freq, t, dur, vol, wave = "square", cutoff = 2800) {
    const osc = ctx.createOscillator();
    const filter = ctx.createBiquadFilter();
    const g = ctx.createGain();
    osc.type = wave;
    osc.frequency.setValueAtTime(freq, t);
    filter.type = "lowpass";
    filter.frequency.value = cutoff;
    g.gain.setValueAtTime(0.001, t);
    g.gain.linearRampToValueAtTime(vol, t + 0.01);
    g.gain.exponentialRampToValueAtTime(0.0005, t + dur);
    osc.connect(filter);
    filter.connect(g);
    g.connect(this.bgmGain);
    osc.start(t);
    osc.stop(t + dur + 0.02);
  }

  _playBell(ctx, freq, t, dur, vol) {
    const osc = ctx.createOscillator();
    const g = ctx.createGain();
    osc.type = "sine";
    osc.frequency.setValueAtTime(freq, t);
    g.gain.setValueAtTime(0.001, t);
    g.gain.linearRampToValueAtTime(vol, t + 0.006);
    g.gain.exponentialRampToValueAtTime(0.0003, t + dur);
    osc.connect(g);
    g.connect(this.bgmGain);
    osc.start(t);
    osc.stop(t + dur + 0.02);
  }

  _playBass(ctx, freq, t, dur, style, vol) {
    const osc = ctx.createOscillator();
    const filter = ctx.createBiquadFilter();
    const g = ctx.createGain();

    if (style === "funk") {
      osc.type = "sawtooth";
      filter.type = "lowpass";
      filter.Q.value = 4.2;
      filter.frequency.setValueAtTime(1350, t);
      filter.frequency.exponentialRampToValueAtTime(190, t + dur);
    } else if (style === "jazz") {
      osc.type = "triangle";
      filter.type = "lowpass";
      filter.frequency.setValueAtTime(340, t);
    } else if (style === "march") {
      osc.type = "sawtooth";
      filter.type = "lowpass";
      filter.frequency.setValueAtTime(520, t);
    } else {
      osc.type = "triangle";
      filter.type = "lowpass";
      filter.frequency.setValueAtTime(750, t);
    }

    osc.frequency.setValueAtTime(freq, t);
    g.gain.setValueAtTime(0.001, t);
    g.gain.linearRampToValueAtTime(vol, t + 0.014);
    g.gain.exponentialRampToValueAtTime(0.0005, t + dur);

    osc.connect(filter);
    filter.connect(g);
    g.connect(this.bgmGain);
    osc.start(t);
    osc.stop(t + dur + 0.02);
  }

  _playTimpani(ctx, freq, t, vol) {
    const osc = ctx.createOscillator();
    const g = ctx.createGain();
    osc.type = "triangle";
    osc.frequency.setValueAtTime(freq * 1.45, t);
    osc.frequency.exponentialRampToValueAtTime(freq, t + 0.09);
    g.gain.setValueAtTime(0.001, t);
    g.gain.linearRampToValueAtTime(vol, t + 0.012);
    g.gain.exponentialRampToValueAtTime(0.0005, t + 0.42);
    osc.connect(g);
    g.connect(this.bgmGain);
    osc.start(t);
    osc.stop(t + 0.44);
  }

  _playKick(ctx, t, startHz, vol) {
    const osc = ctx.createOscillator();
    const g = ctx.createGain();
    osc.type = "sine";
    osc.frequency.setValueAtTime(startHz, t);
    osc.frequency.exponentialRampToValueAtTime(42, t + 0.11);
    g.gain.setValueAtTime(vol, t);
    g.gain.exponentialRampToValueAtTime(0.0005, t + 0.16);
    osc.connect(g);
    g.connect(this.bgmGain);
    osc.start(t);
    osc.stop(t + 0.18);
  }

  _playSnare(ctx, t, dur, vol) {
    if (!this.noiseBuf) return;
    const src = ctx.createBufferSource();
    src.buffer = this.noiseBuf;
    const filter = ctx.createBiquadFilter();
    filter.type = "highpass";
    filter.frequency.value = 1100;
    const g = ctx.createGain();
    g.gain.setValueAtTime(vol, t);
    g.gain.exponentialRampToValueAtTime(0.0005, t + dur);
    src.connect(filter);
    filter.connect(g);
    g.connect(this.bgmGain);
    src.start(t);
    src.stop(t + dur + 0.01);
  }

  _playHat(ctx, t, dur, vol) {
    if (!this.noiseBuf) return;
    const src = ctx.createBufferSource();
    src.buffer = this.noiseBuf;
    const filter = ctx.createBiquadFilter();
    filter.type = "highpass";
    filter.frequency.value = 6500;
    const g = ctx.createGain();
    g.gain.setValueAtTime(vol, t);
    g.gain.exponentialRampToValueAtTime(0.0004, t + dur);
    src.connect(filter);
    filter.connect(g);
    g.connect(this.bgmGain);
    src.start(t);
    src.stop(t + dur + 0.01);
  }

  _playShaker(ctx, t, vol) {
    if (!this.noiseBuf) return;
    const src = ctx.createBufferSource();
    src.buffer = this.noiseBuf;
    const filter = ctx.createBiquadFilter();
    filter.type = "bandpass";
    filter.frequency.value = 4800;
    filter.Q.value = 2.0;
    const g = ctx.createGain();
    g.gain.setValueAtTime(0.001, t);
    g.gain.linearRampToValueAtTime(vol, t + 0.012);
    g.gain.exponentialRampToValueAtTime(0.0004, t + 0.065);
    src.connect(filter);
    filter.connect(g);
    g.connect(this.bgmGain);
    src.start(t);
    src.stop(t + 0.075);
  }

  _playRim(ctx, t, vol) {
    const osc = ctx.createOscillator();
    const g = ctx.createGain();
    osc.type = "triangle";
    osc.frequency.setValueAtTime(840, t);
    osc.frequency.exponentialRampToValueAtTime(460, t + 0.025);
    g.gain.setValueAtTime(vol, t);
    g.gain.exponentialRampToValueAtTime(0.0004, t + 0.035);
    osc.connect(g);
    g.connect(this.bgmGain);
    osc.start(t);
    osc.stop(t + 0.04);
  }

  // ---------- 互動／情境音效 (SFX) ----------

  play(kind) {
    const ctx = this._ensureCtx();
    if (!ctx) return;
    const now = ctx.currentTime;

    switch (kind) {
      case "bell": {
        [523.25, 659.25, 783.99].forEach((freq, i) => {
          this._tone(ctx, freq, now + i * 0.22, 0.75, "triangle", 0.14);
        });
        break;
      }
      case "chime": {
        [587.33, 880, 1174.66].forEach((freq, i) => {
          this._tone(ctx, freq, now + i * 0.08, 0.45, "sine", 0.1);
        });
        break;
      }
      case "encounter": {
        [523.25, 659.25, 987.77].forEach((freq, i) => {
          this._tone(ctx, freq, now + i * 0.06, 0.35, "sine", 0.09);
        });
        break;
      }
      case "interact": {
        this._tone(ctx, 740, now, 0.12, "sine", 0.08, 987.77);
        break;
      }
      case "jump": {
        this._tone(ctx, 260, now, 0.14, "sine", 0.08, 520);
        break;
      }
      case "firework": {
        this._tone(ctx, 220, now, 0.18, "sawtooth", 0.08, 680);
        this._noise(ctx, now + 0.18, 0.45, 0.14);
        break;
      }
      case "horn": {
        this._tone(ctx, 311.13, now, 0.16, "sawtooth", 0.08);
        this._tone(ctx, 392.0, now + 0.02, 0.25, "sawtooth", 0.08);
        break;
      }
      case "bird": {
        this._tone(ctx, 1480, now, 0.1, "sine", 0.08, 2100);
        this._tone(ctx, 1650, now + 0.14, 0.11, "sine", 0.08, 2320);
        break;
      }
      case "place": {
        this._tone(ctx, 290, now, 0.07, "square", 0.06, 440);
        break;
      }
      case "mount": {
        this._tone(ctx, 392.0, now, 0.1, "triangle", 0.1, 523.25);
        this._tone(ctx, 587.33, now + 0.07, 0.14, "sine", 0.11);
        break;
      }
      case "dismount": {
        this._tone(ctx, 523.25, now, 0.09, "triangle", 0.09, 392.0);
        this._tone(ctx, 349.23, now + 0.07, 0.14, "sine", 0.09);
        break;
      }
      case "door": {
        this._tone(ctx, 220.0, now, 0.11, "triangle", 0.1, 185.0);
        this._tone(ctx, 523.25, now + 0.08, 0.18, "sine", 0.1, 783.99);
        this._tone(ctx, 659.25, now + 0.15, 0.22, "sine", 0.09, 1046.5);
        break;
      }
      case "locked": {
        this._tone(ctx, 196.0, now, 0.09, "square", 0.07, 155.0);
        this._tone(ctx, 164.81, now + 0.11, 0.12, "square", 0.07, 130.0);
        break;
      }
      case "delete": {
        this._tone(ctx, 260, now, 0.09, "triangle", 0.07, 130);
        break;
      }
      case "victory": {
        // 凱旋過關號角琶音（C 大調上行）
        [523.25, 659.25, 783.99, 1046.5, 1318.51].forEach((freq, i) => {
          this._tone(ctx, freq, now + i * 0.1, i === 4 ? 0.65 : 0.28, "triangle", 0.14);
        });
        break;
      }
      case "quiz_ok": {
        this._tone(ctx, 659.25, now, 0.14, "sine", 0.11);
        this._tone(ctx, 987.77, now + 0.09, 0.28, "sine", 0.12);
        break;
      }
      case "quiz_fail": {
        this._tone(ctx, 329.63, now, 0.15, "sawtooth", 0.08);
        this._tone(ctx, 246.94, now + 0.12, 0.25, "sawtooth", 0.08);
        break;
      }
      case "alarm": {
        this._tone(ctx, 620, now, 0.12, "square", 0.07, 460);
        this._tone(ctx, 620, now + 0.16, 0.14, "square", 0.07, 460);
        break;
      }
      case "support": {
        [440, 554.37, 659.25, 880].forEach((freq, i) => {
          this._tone(ctx, freq, now + i * 0.055, 0.34, "sine", 0.095);
        });
        break;
      }
      default: {
        this._tone(ctx, 640, now, 0.08, "sine", 0.07);
      }
    }
  }

  _tone(ctx, freq, start, dur, type, vol, endFreq = null) {
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = type;
    osc.frequency.setValueAtTime(freq, start);
    if (endFreq) {
      osc.frequency.exponentialRampToValueAtTime(endFreq, start + dur);
    }
    gain.gain.setValueAtTime(0.001, start);
    gain.gain.linearRampToValueAtTime(vol, start + 0.015);
    gain.gain.exponentialRampToValueAtTime(0.0001, start + dur);
    osc.connect(gain);
    gain.connect(this._compressor || ctx.destination);
    osc.start(start);
    osc.stop(start + dur + 0.02);
  }

  _noise(ctx, start, dur, vol) {
    const len = Math.floor(ctx.sampleRate * dur);
    const buf = ctx.createBuffer(1, len, ctx.sampleRate);
    const data = buf.getChannelData(0);
    for (let i = 0; i < len; i++) {
      data[i] = (Math.random() * 2 - 1) * Math.exp(-i / (len * 0.35));
    }
    const src = ctx.createBufferSource();
    src.buffer = buf;
    const filter = ctx.createBiquadFilter();
    filter.type = "bandpass";
    filter.frequency.value = 1200;
    const gain = ctx.createGain();
    gain.gain.setValueAtTime(vol, start);
    gain.gain.exponentialRampToValueAtTime(0.001, start + dur);
    src.connect(filter);
    filter.connect(gain);
    gain.connect(this._compressor || ctx.destination);
    src.start(start);
    src.stop(start + dur + 0.02);
  }
}

export const soundEngine = new SoundEngine();

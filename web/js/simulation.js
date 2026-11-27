//! 活體導演 60fps 模擬與物理碰撞引擎：
//! - 24h 時光與環境系統、路徑巡航、自由漫遊避障 AI
//! - 統一地形可行走判定（防落水／防穿牆／防越陡崖／支援橋面步行）
//! - 2D 旋轉 OBB 實體碰撞箱（建築、家具、道具、樹幹防穿越＋分軸滑行＋卡點脫困保護）
//! - 玩家化身移動／跳躍重力／上帝視角與街道漫步共用物理、兩物件相遇與事件規則

import { soundEngine } from "./audio.js";

const PX_TO_WORLD = 1.0 / 26.0;

/// 安全解析實體 meta_json 中的交通工具搭乘設定
export function parseEntityVehicle(entity) {
  if (!entity || !entity.meta_json) return null;
  try {
    const root = typeof entity.meta_json === "string" ? JSON.parse(entity.meta_json) : entity.meta_json;
    const veh = root && typeof root === "object" ? root.vehicle : null;
    if (!veh || typeof veh !== "object") return null;
    return veh;
  } catch {
    return null;
  }
}

/// 將更新後的 vehicle 設定寫回實體的 meta_json（完整保留 interaction 等既有欄位）
export function buildEntityMetaWithVehicle(entity, vehicleCfg) {
  let root = {};
  try {
    const parsed = typeof entity?.meta_json === "string" ? JSON.parse(entity.meta_json || "{}") : {};
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) root = parsed;
  } catch {
    root = {};
  }
  if (!vehicleCfg) {
    delete root.vehicle;
  } else {
    root.vehicle = vehicleCfg;
  }
  return JSON.stringify(root);
}

/// 安全解析實體 meta_json 中的「門 (`portal`)」跨世界傳送設定（未啟用則回傳 null）
export function parseEntityPortal(entity) {
  if (!entity || !entity.meta_json) return null;
  try {
    const root = typeof entity.meta_json === "string" ? JSON.parse(entity.meta_json) : entity.meta_json;
    const portal = root && typeof root === "object" ? root.portal : null;
    if (!portal || typeof portal !== "object" || !portal.enabled) return null;
    return portal;
  } catch {
    return null;
  }
}

/// 將更新後的「門 (`portal`)」設定寫回實體的 meta_json（完整保留 interaction 與 vehicle 等既有欄位）
export function buildEntityMetaWithPortal(entity, portalCfg) {
  let root = {};
  try {
    const parsed = typeof entity?.meta_json === "string" ? JSON.parse(entity.meta_json || "{}") : {};
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) root = parsed;
  } catch {
    root = {};
  }
  if (!portalCfg || !portalCfg.enabled) {
    delete root.portal;
  } else {
    root.portal = portalCfg;
  }
  return JSON.stringify(root);
}

export class WorldSimulation {
  constructor(renderer, onUiSync) {
    this.renderer = renderer;
    this.onUiSync = onUiSync;

    this.paused = false;
    this.visitorMode = false;
    this.autoEncounterEnabled = true;
    this.timeOfDay = 10.5;
    this.timeSpeed = 1.0;
    this.weather = "clear";

    this.assetMap = new Map();
    this.entities = [];
    this.paths = [];
    this.wanderZones = [];
    this.events = [];

    // 任務關卡、跨世界門與互動系統回呼（由 main.js / standalone_bootstrap.js 掛接）
    this.onEntityQuestInteract = null;
    this.onProximityCheck = null;
    this.getQuestBadge = null;
    this.onPortalTrigger = null;
    this.checkPortalLock = null;

    // 玩家專屬可控 3D 化身物理狀態（z 為相對於地面高度之離地高度）
    this.player = {
      id: "__player__",
      asset_id: null,
      name: "冒險玩家",
      x: 20,
      y: 20,
      z: 0,
      velZ: 0,
      yaw: 0,
      scale: 1.0,
      scale_x: 1.0,
      scale_y: 1.0,
      scale_z: 1.0,
      tint_hex: "#ffffff",
      emissive_on: true,
      visible: true,
      anim_mode: "static",
      _phase: 0.4,
      _reveal: 1.0,
      _tweenMul: 1.0,
      moving: false,
      radius: 0.26,
      mountedEntityId: null,
      mountControlMode: "drive",
      mountConfig: null,
      playerHiddenInVehicle: false,
    };
    this.nearestToPlayer = null;
    this.nearestRideable = null;
    this.nearestPortal = null;
    this._lastHornMs = 0;

    this.pathStates = new Map();
    this.wanderStates = new Map();
    this.eventCooldowns = new Map();
    this.prevTimeOfDay = 10.5;
    this.lastIntervalTimes = new Map();
    this.lastProximityInside = new Set();
    this.lastEncounterInside = new Set();
    this.autoEncounterCooldowns = new Map();
    this.autoEncounterTimer = 0;
    this.prevIsNight = false;
    this.tweens = [];
  }

  loadState(fullState, assetMap) {
    if (assetMap) this.assetMap = assetMap;
    const meta = fullState.meta;
    this.timeOfDay = meta.time_of_day ?? 10.5;
    this.prevTimeOfDay = this.timeOfDay;
    this.timeSpeed = meta.time_speed ?? 1.0;
    this.weather = meta.weather || "clear";
    this.prevIsNight = this.timeOfDay < 6.0 || this.timeOfDay >= 18.5;

    this.entities = fullState.entities.map((e, i) => {
      const s = e.scale ?? 1.0;
      return {
        ...e,
        scale_x: e.scale_x > 0.01 ? e.scale_x : s,
        scale_y: e.scale_y > 0.01 ? e.scale_y : s,
        scale_z: e.scale_z > 0.01 ? e.scale_z : s,
        _baseX: e.x,
        _baseY: e.y,
        _baseZ: e.z,
        _baseYaw: e.yaw,
        _phase: (i * 1.37) % (Math.PI * 2),
        _reveal: 1.0,
        _tweenMul: 1.0,
      };
    });
    this.paths = fullState.paths || [];
    this.wanderZones = fullState.wander_zones || [];
    this.events = fullState.events || [];
    this.pathStates.clear();
    this.wanderStates.clear();
    this.eventCooldowns.clear();
    this.lastIntervalTimes.clear();
    this.lastProximityInside.clear();
    this.lastEncounterInside.clear();
    this.autoEncounterCooldowns.clear();
    this.nearestToPlayer = null;
    this.nearestRideable = null;
    this.nearestPortal = null;

    this.player.mountedEntityId = null;
    this.player.mountControlMode = "drive";
    this.player.mountConfig = null;
    this.player.playerHiddenInVehicle = false;

    this._initPlayerFromMeta(meta);
    this.onUiSync?.("mount", null);
  }

  // ---------- 移動類型、碰撞箱與地形物理系統 ----------

  /// 判斷物件／角色的棲息移動類型：'air'（飛行懸浮）｜'water'（水域船鴨）｜'land'（陸地角色/動物/車輛）
  getActorMobility(assetMeta, name = "", baseZ = 0) {
    const s = `${name || ""} ${assetMeta?.name || ""}`;
    if (
      baseZ >= 1.8 ||
      assetMeta?.anim_preset === "float" ||
      /(熱氣球|飛船|飛碟|海鷗|鳥|鷹|鶴|balloon|spaceship|bird)/i.test(s)
    ) {
      return "air";
    }
    if (/(帆船|遊船|小船|船|艇|鴨|魚|鯨|渡輪|boat|ship|duck)/i.test(s)) {
      return "water";
    }
    return "land";
  }

  /// 建立單一世界實體之 2D 旋轉 OBB 水平與垂直碰撞箱；若為懸空吊燈或可踩草叢則回傳 null
  getEntityCollider(ent) {
    if (!ent || !ent.visible) return null;
    const zOffset = ent.z || 0;
    if (zOffset >= 1.45) return null; // 吊燈、熱氣球與飛鳥位於頭頂上方，地面可通行

    const meta = this.assetMap.get(ent.asset_id);
    const nameStr = `${ent.name || ""} ${meta?.name || ""}`;
    if (/(稻麥|麥田|花圃|草叢|wheat)/i.test(nameStr)) {
      return null;
    }

    const cat = meta?.category || "prop";
    const sx = ent.scale_x > 0.01 ? ent.scale_x : (ent.scale || 1.0);
    const sy = ent.scale_y > 0.01 ? ent.scale_y : (ent.scale || 1.0);
    const sz = ent.scale_z > 0.01 ? ent.scale_z : (ent.scale || 1.0);
    const sw = (meta?.slice_w || 32) * PX_TO_WORLD * sx;
    const sl = (meta?.slice_h || 32) * PX_TO_WORLD * sy;
    const sh = (meta?.slice_count || 36) * PX_TO_WORLD * sz;

    let hx;
    let hy;
    if (cat === "building") {
      hx = Math.max(0.45, sw * 0.42);
      hy = Math.max(0.45, sl * 0.42);
    } else if (cat === "prop") {
      if (/(路燈|街燈|燈塔|柱|旗|牌|lamp|pillar)/i.test(nameStr)) {
        hx = Math.max(0.20, sw * 0.22);
        hy = Math.max(0.20, sl * 0.22);
      } else {
        hx = Math.max(0.30, sw * 0.36);
        hy = Math.max(0.30, sl * 0.36);
      }
    } else if (cat === "plant") {
      // 植物僅阻擋樹幹／花盆核心，不卡住樹冠陰影
      hx = Math.max(0.22, sw * 0.18);
      hy = Math.max(0.22, sl * 0.18);
    } else if (cat === "vehicle") {
      hx = Math.max(0.34, sw * 0.35);
      hy = Math.max(0.34, sl * 0.35);
    } else {
      // character / animal：輕微身位半徑避免互相穿透重疊
      hx = 0.22;
      hy = 0.22;
    }

    const baseZ = this.renderer.getTerrainElevationAt(ent.x, ent.y) + zOffset;
    const topZ = baseZ + Math.max(0.75, sh * 0.92);
    const yawRad = ((ent.yaw || 0) * Math.PI) / 180;
    return {
      id: ent.id,
      cat,
      cx: ent.x,
      cy: ent.y,
      hx,
      hy,
      cos: Math.cos(yawRad),
      sin: Math.sin(yawRad),
      baseZ,
      topZ,
    };
  }

  _isCircleHittingObb(px, py, radius, col) {
    const vx = px - col.cx;
    const vy = py - col.cy;
    // 反旋轉轉入物件局部座標系
    const lx = vx * col.cos + vy * col.sin;
    const ly = -vx * col.sin + vy * col.cos;
    const clampedX = Math.max(-col.hx, Math.min(col.hx, lx));
    const clampedY = Math.max(-col.hy, Math.min(col.hy, ly));
    const dx = lx - clampedX;
    const dy = ly - clampedY;
    return dx * dx + dy * dy < radius * radius;
  }

  /// 計算陸地角色足跡九宮格採樣點（中心＋四正向＋四對角）中落入無橋水域、實牆或邊界的違規點數
  _countInvalidTerrainSamples(mobility, wx, wy, footR) {
    if (mobility !== "land" || footR <= 0) return 0;
    const w = this.renderer.worldWidth || 40;
    const h = this.renderer.worldHeight || 40;
    const diag = footR * 0.72;
    const offsets = [
      [0, 0],
      [footR, 0],
      [-footR, 0],
      [0, footR],
      [0, -footR],
      [diag, diag],
      [diag, -diag],
      [-diag, diag],
      [-diag, -diag],
    ];
    let invalidCount = 0;
    for (const [ox, oy] of offsets) {
      const sx = wx + ox;
      const sy = wy + oy;
      if (sx < 0.45 || sy < 0.45 || sx >= w - 0.45 || sy >= h - 0.45) {
        invalidCount++;
        continue;
      }
      const tile = this.renderer.terrainGrid[Math.floor(sy) * w + Math.floor(sx)];
      if (!tile) {
        invalidCount++;
        continue;
      }
      const isWaterNoBridge = tile.ground_type === "water" && tile.road_type !== "bridge";
      const isSolidWall = Boolean(tile.ground_type && tile.ground_type.startsWith("wall_") && tile.elevation >= 2);
      if (isWaterNoBridge || isSolidWall) {
        invalidCount++;
      }
    }
    return invalidCount;
  }

  /// 檢查座標點在地形、水域、牆體、足跡半徑緩衝與台階落差上是否允許該類型角色踏入
  isTerrainWalkableFor(mobility, wx, wy, currentFootZ = null, footR = 0.0) {
    const w = this.renderer.worldWidth || 40;
    const h = this.renderer.worldHeight || 40;
    if (wx < 0.45 || wy < 0.45 || wx >= w - 0.45 || wy >= h - 0.45) return false;
    if (mobility === "air") return true;

    const gx = Math.floor(wx);
    const gy = Math.floor(wy);
    const tile = this.renderer.terrainGrid[gy * w + gx];
    if (!tile) return false;

    const isWaterNoBridge = tile.ground_type === "water" && tile.road_type !== "bridge";
    if (mobility === "water") {
      return isWaterNoBridge;
    }

    // 陸地角色禁止掉入無橋水域，禁止穿過建築剖面牆體
    if (isWaterNoBridge) return false;
    if (tile.ground_type && tile.ground_type.startsWith("wall_") && tile.elevation >= 2) {
      return false;
    }

    // 禁止瞬移爬上兩格以上的垂直高崖（一格台階 0.52 或橋面 0.58 可正常走上，亦可透過跳躍升高；僅測中心點避免上階梯卡頓）
    if (currentFootZ !== null) {
      const nextGround = this.renderer.getTerrainElevationAt(wx, wy);
      if (nextGround - currentFootZ > 0.65) {
        return false;
      }
    }

    // 檢查足跡外圍 9 點採樣，確保靠近岸邊／木橋邊緣時雙腳與模型邊緣不會懸空到水面或卡進牆面
    if (footR > 0 && this._countInvalidTerrainSamples(mobility, wx, wy, footR) > 0) {
      return false;
    }
    return true;
  }

  /// 檢驗指定座標是否無任何地形或實體碰撞阻塞（支援「已經卡在內部或邊緣時往安全側移動可脫困」）
  canStandAt(actorId, mobility, nx, ny, footZ, radius = 0.26, fromX = null, fromY = null, ignoreDynamicPeers = false) {
    const footR = mobility === "land" ? Math.max(0.30, radius) : 0.0;
    const terrainOk = this.isTerrainWalkableFor(mobility, nx, ny, footZ, footR);
    if (!terrainOk) {
      // 脫困保護：當中心點本身位於合法陸地／橋面，且新位置的邊緣違規採樣數嚴格少於起點時，允許向岸內脫困移動
      const centerOk = this.isTerrainWalkableFor(mobility, nx, ny, footZ, 0.0);
      if (
        centerOk &&
        fromX !== null &&
        fromY !== null &&
        this._countInvalidTerrainSamples(mobility, nx, ny, footR) <
          this._countInvalidTerrainSamples(mobility, fromX, fromY, footR)
      ) {
        // 允許通過地形邊緣緩衝檢查並繼續檢測後方實體碰撞箱
      } else {
        return false;
      }
    }

    const effFootZ = footZ !== null ? footZ : this.renderer.getTerrainElevationAt(nx, ny);

    for (const ent of this.entities) {
      if (ent.id === actorId) continue;
      if (ent.parent_id === actorId) continue;
      if (ignoreDynamicPeers && (ent.parent_id || this._isEntityDynamic(ent.id))) continue;
      const col = this.getEntityCollider(ent);
      if (!col) continue;
      // 垂直高度不相交（例如跳到物件頂部上方，或走在較高物件下方）則不阻擋
      if (effFootZ >= col.topZ - 0.12 || effFootZ + 1.15 <= col.baseZ) continue;

      if (this._isCircleHittingObb(nx, ny, radius, col)) {
        // 脫困保護：如果先前就已經卡在這個物件內，往外遠離中心的方向允許移動直到脫困
        if (fromX !== null && fromY !== null && this._isCircleHittingObb(fromX, fromY, radius, col)) {
          const prevDist = Math.hypot(fromX - col.cx, fromY - col.cy);
          const nextDist = Math.hypot(nx - col.cx, ny - col.cy);
          if (nextDist > prevDist + 0.0005) {
            continue;
          }
        }
        return false;
      }
    }

    // 玩家角色本身也成為 NPC 的實體身位避讓（避免漫步 NPC 直接穿過玩家身體；載具本身則略過其搭乘者）
    if (actorId !== "__player__" && actorId !== this.player.mountedEntityId && this.player.visible && !ignoreDynamicPeers) {
      const pFoot = this.renderer.getTerrainElevationAt(this.player.x, this.player.y) + (this.player.z || 0);
      if (Math.abs(effFootZ - pFoot) < 1.1) {
        const dNext = Math.hypot(nx - this.player.x, ny - this.player.y);
        if (dNext < radius + 0.20) {
          if (fromX !== null && fromY !== null) {
            const dPrev = Math.hypot(fromX - this.player.x, fromY - this.player.y);
            if (dNext > dPrev + 0.0005) return true;
          }
          return false;
        }
      }
    }

    return true;
  }

  _isEntityDynamic(entityId) {
    for (const p of this.paths) {
      if (p.active && p.entity_id === entityId) return true;
    }
    for (const z of this.wanderZones) {
      if ((z.entity_ids || []).includes(entityId)) return true;
    }
    return false;
  }

  /// 分軸防穿透安全移動（先測全位移，碰撞時改測單軸沿邊緣滑行）
  tryMoveActor(actorId, mobility, fromX, fromY, footZ, dx, dy, radius = 0.26, ignoreDynamicPeers = false) {
    const nx = fromX + dx;
    const ny = fromY + dy;
    if (this.canStandAt(actorId, mobility, nx, ny, footZ, radius, fromX, fromY, ignoreDynamicPeers)) {
      return { x: nx, y: ny, blocked: false };
    }
    if (
      Math.abs(dx) > 1e-6 &&
      this.canStandAt(actorId, mobility, nx, fromY, footZ, radius, fromX, fromY, ignoreDynamicPeers)
    ) {
      return { x: nx, y: fromY, blocked: false, slid: true };
    }
    if (
      Math.abs(dy) > 1e-6 &&
      this.canStandAt(actorId, mobility, fromX, ny, footZ, radius, fromX, fromY, ignoreDynamicPeers)
    ) {
      return { x: fromX, y: ny, blocked: false, slid: true };
    }
    return { x: fromX, y: fromY, blocked: true };
  }

  // ---------- 玩家初始化與操控物理 ----------

  _findSafeLandPoint(prefX, prefY, avoidBridge = true) {
    const w = this.renderer.worldWidth || 40;
    const h = this.renderer.worldHeight || 40;
    const check = (x, y, strictNoBridge) => {
      if (!this.canStandAt("__player__", "land", x, y, null, 0.30)) return false;
      if (strictNoBridge) {
        const t = this.renderer.terrainGrid[Math.floor(y) * w + Math.floor(x)];
        if (t && t.road_type === "bridge") return false;
      }
      return true;
    };

    if (check(prefX, prefY, avoidBridge)) return [prefX, prefY];

    const cx = Math.floor(prefX);
    const cy = Math.floor(prefY);
    for (const strict of [avoidBridge, false]) {
      for (let r = 1; r <= Math.max(w, h); r++) {
        for (let dy = -r; dy <= r; dy++) {
          for (let dx = -r; dx <= r; dx++) {
            if (Math.abs(dx) !== r && Math.abs(dy) !== r) continue;
            const px = cx + dx + 0.5;
            const py = cy + dy + 0.5;
            if (px < 1 || py < 1 || px >= w - 1 || py >= h - 1) continue;
            if (check(px, py, strict)) return [px, py];
          }
        }
      }
    }
    return [Math.max(1.5, Math.min(w - 1.5, prefX)), Math.max(1.5, Math.min(h - 1.5, prefY))];
  }

  _initPlayerFromMeta(meta) {
    const assets = Array.from(this.assetMap.values());
    let chosenId = meta.player_asset_id && this.assetMap.has(meta.player_asset_id)
      ? meta.player_asset_id
      : null;

    if (!chosenId && assets.length > 0) {
      const charAsset = assets.find((a) => a.category === "character");
      const animalAsset = assets.find((a) => a.category === "animal");
      chosenId = (charAsset || animalAsset || assets[0]).id;
    }

    const w = meta.width || 40;
    const h = meta.height || 40;
    const hasSavedPos = typeof meta.player_x === "number" && typeof meta.player_y === "number";
    const prefX = hasSavedPos ? meta.player_x : w * 0.5;
    const prefY = hasSavedPos ? meta.player_y : h * 0.5 + 3.0;
    const [spawnX, spawnY] = this._findSafeLandPoint(prefX, prefY, !hasSavedPos);

    const pScale = Number(meta.player_scale ?? 1.0);
    const assetMeta = chosenId ? this.assetMap.get(chosenId) : null;
    const baseScale = assetMeta ? (assetMeta.default_scale || 1.0) * pScale : pScale;

    this.player.asset_id = chosenId;
    this.player.name = meta.player_name || "冒險玩家";
    this.player.x = spawnX;
    this.player.y = spawnY;
    this.player.z = 0;
    this.player.velZ = 0;
    this.player.scale = baseScale;
    this.player.scale_x = baseScale;
    this.player.scale_y = baseScale;
    this.player.scale_z = baseScale;
    this.player.visible = Boolean(chosenId);
    this.player.anim_mode = "static";
    this.player._reveal = 1.0;
    this.player._tweenMul = 1.0;

    const gz = this.renderer.getTerrainElevationAt(spawnX, spawnY);
    this.renderer.camera.walkPos = [spawnX, spawnY, gz];
    this.renderer.camera.walkVelZ = 0;
  }

  applyPlayerScale(scaleMul) {
    const assetMeta = this.player.asset_id ? this.assetMap.get(this.player.asset_id) : null;
    const base = assetMeta ? (assetMeta.default_scale || 1.0) * scaleMul : scaleMul;
    this.player.scale = base;
    this.player.scale_x = base;
    this.player.scale_y = base;
    this.player.scale_z = base;
  }

  // ---------- 交通工具搭乘／駕駛／巡航系統 ----------

  /// 檢查某實體是否綁有啟用中的巡航路徑
  hasActivePathFor(entityId) {
    return this.paths.some(
      (p) => p.active && p.entity_id === entityId && Array.isArray(p.waypoints) && p.waypoints.length >= 2,
    );
  }

  /// 取得實體完整的載具搭乘設定（未設定者依類型與名稱智慧推斷；明確停用 rideable:false 則回傳 null）
  getVehicleConfig(ent) {
    if (!ent || !ent.visible) return null;
    const explicit = parseEntityVehicle(ent);
    if (explicit && explicit.rideable === false) return null;

    const meta = this.assetMap.get(ent.asset_id);
    const nameStr = `${ent.name || ""} ${meta?.name || ""}`;
    const isVehicleCat = meta?.category === "vehicle";
    const isMountAnimal =
      meta?.category === "animal" &&
      /(駿馬|白馬|戰馬|馬|獨角獸|駱駝|大象|horse|pony|camel|elephant)/i.test(nameStr);

    if (!explicit && !isVehicleCat && !isMountAnimal) return null;

    const mobility = this.getActorMobility(meta, ent.name, ent.z || 0);
    const hasPath = this.hasActivePathFor(ent.id);

    const isInsideDefault =
      /(巴士|公車|校車|卡車|貨車|火車|列車|電車|跑車|汽車|轎車|賽車|飛船|太空船|星艦|bus|truck|train|car|spaceship)/i.test(
        nameStr,
      );
    const isCarriage = /(馬車|篷車|雙馬車|carriage|wagon|coach)/i.test(nameStr);
    const isBoat = mobility === "water";
    const isAir = mobility === "air";

    let defaultSpeed = 1.5;
    if (/(跑車|汽車|轎車|賽車|car|sports)/i.test(nameStr)) defaultSpeed = 1.95;
    else if (isMountAnimal) defaultSpeed = 1.75;
    else if (isCarriage) defaultSpeed = 1.65;
    else if (isAir) defaultSpeed = 1.75;
    else if (isBoat || /(巴士|公車|火車|列車|bus|train)/i.test(nameStr)) defaultSpeed = 1.55;

    let defaultSeatZ = 0.52;
    if (isBoat) defaultSeatZ = 0.42;
    else if (isAir) defaultSeatZ = 0.24;
    else if (isMountAnimal || isCarriage) defaultSeatZ = 0.58;

    let defaultSeatFwd = 0.0;
    if (isCarriage) defaultSeatFwd = 0.18;
    else if (isBoat) defaultSeatFwd = -0.08;

    const mode = explicit?.mode || (hasPath ? "both" : "drive");
    const seat_style = explicit?.seat_style || (isInsideDefault ? "inside" : "ride_on");
    const speed_mul = Number(explicit?.speed_mul ?? defaultSpeed);
    const seat_z = Number(explicit?.seat_z ?? defaultSeatZ);
    const seat_forward = Number(explicit?.seat_forward ?? defaultSeatFwd);

    return {
      rideable: true,
      mode: ["drive", "passenger", "both"].includes(mode) ? mode : "drive",
      seat_style: seat_style === "inside" ? "inside" : "ride_on",
      speed_mul: Math.max(0.5, Math.min(4.0, Number.isFinite(speed_mul) ? speed_mul : defaultSpeed)),
      seat_z: Math.max(-0.5, Math.min(3.0, Number.isFinite(seat_z) ? seat_z : defaultSeatZ)),
      seat_forward: Math.max(-1.5, Math.min(1.5, Number.isFinite(seat_forward) ? seat_forward : defaultSeatFwd)),
      mobility,
      hasPath,
      isExplicit: Boolean(explicit),
    };
  }

  /// 尋找玩家身旁最近且可搭乘的交通工具或坐騎
  findNearestRideable() {
    if (!this.player.visible) return null;
    let best = null;
    let bestDist = Infinity;
    for (const ent of this.entities) {
      if (!ent.visible) continue;
      const cfg = this.getVehicleConfig(ent);
      if (!cfg) continue;
      if (cfg.mobility === "air" && (ent.z || 0) > 3.2) continue;
      const maxDist = cfg.mobility === "land" ? 2.9 : 3.9;
      const d = Math.hypot(ent.x - this.player.x, ent.y - this.player.y);
      if (d <= maxDist && d < bestDist) {
        bestDist = d;
        best = { entity: ent, config: cfg, dist: d };
      }
    }
    return best;
  }

  /// 取得目前玩家搭乘狀態摘要（供 UI 橫幅與渲染器顯示）
  getMountedState() {
    const pl = this.player;
    if (!pl.mountedEntityId) return null;
    const veh = this.entities.find((e) => e.id === pl.mountedEntityId);
    if (!veh) return null;
    const cfg = pl.mountConfig || this.getVehicleConfig(veh);
    const meta = this.assetMap.get(veh.asset_id);
    return {
      entity: veh,
      config: cfg,
      controlMode: pl.mountControlMode,
      canSwitchMode: Boolean(cfg && cfg.mode === "both" && this.hasActivePathFor(veh.id)),
      category: meta?.category || "vehicle",
    };
  }

  _getMountIcon(ent, cfg) {
    const s = `${ent?.name || ""} ${this.assetMap.get(ent?.asset_id)?.name || ""}`;
    if (cfg?.mobility === "water" || /(船|艇|boat|ship)/i.test(s)) return "⛵";
    if (cfg?.mobility === "air" || /(熱氣球|氣球|飛船|太空船|balloon|spaceship)/i.test(s)) return "🎈";
    if (/(馬車|篷車|carriage|wagon)/i.test(s)) return "🐴";
    if (/(馬|horse|pony)/i.test(s)) return "🐎";
    if (/(火車|列車|train)/i.test(s)) return "🚂";
    if (/(巴士|公車|bus)/i.test(s)) return "🚌";
    return "🚗";
  }

  /// 讓玩家搭乘指定載具／坐騎
  mountEntity(entityId, preferredControl = null) {
    const pl = this.player;
    if (!pl.visible) return { ok: false, reason: "player_hidden" };
    const ent = this.entities.find((e) => e.id === entityId && e.visible);
    if (!ent) return { ok: false, reason: "not_found" };
    const cfg = this.getVehicleConfig(ent);
    if (!cfg) return { ok: false, reason: "not_rideable" };

    if (pl.mountedEntityId && pl.mountedEntityId !== entityId) {
      this.dismountPlayer(true);
    }

    let controlMode = "drive";
    if (cfg.mode === "passenger") {
      controlMode = "passenger";
    } else if (cfg.mode === "both") {
      if (preferredControl === "drive" || preferredControl === "passenger") {
        controlMode = preferredControl;
      } else {
        controlMode = cfg.hasPath ? "passenger" : "drive";
      }
    }
    if (controlMode === "passenger" && !cfg.hasPath && cfg.mode === "both") {
      controlMode = "drive";
    }

    if (ent._origAnimMode === undefined) {
      ent._origAnimMode = ent.anim_mode || "auto";
    }
    ent._jumpZ = 0;
    ent._jumpVelZ = 0;
    ent._mountBaseZ = ent.z || 0;

    pl.mountedEntityId = ent.id;
    pl.mountControlMode = controlMode;
    pl.mountConfig = cfg;
    pl.playerHiddenInVehicle = cfg.seat_style === "inside";
    pl.velZ = 0;
    pl.moving = false;

    this._syncMountedPlayer(0);

    const gz = this.renderer.getTerrainElevationAt(ent.x, ent.y) + (ent.z || 0);
    if (cfg.mobility === "water") soundEngine.play("chime");
    else if (this.assetMap.get(ent.asset_id)?.category === "vehicle") soundEngine.play("horn");
    else soundEngine.play("mount");

    this.renderer.spawnParticles("sparkle", ent.x, ent.y, gz + 1.0, 14);
    const bubbleMsg = controlMode === "passenger"
      ? `乘上「${ent.name}」自動巡航中！`
      : `開始駕馭「${ent.name}」！`;
    this.renderer.addSpeechBubble("__player__", pl.x, pl.y, gz, bubbleMsg, 2.5);
    this.onUiSync?.("mount", this.getMountedState());
    return { ok: true, vehicle: ent, controlMode };
  }

  /// 依載具座標／朝向／座位參數同步玩家位置（含騎乘跳躍重力與座標平滑）
  _syncMountedPlayer(dt = 0) {
    const pl = this.player;
    if (!pl.mountedEntityId) return;
    const veh = this.entities.find((e) => e.id === pl.mountedEntityId && e.visible);
    if (!veh) {
      this.dismountPlayer(true);
      return;
    }

    const cfg = pl.mountConfig || this.getVehicleConfig(veh);
    if (!cfg) {
      this.dismountPlayer(true);
      return;
    }

    // 處理騎乘動物跳躍物理
    if (dt > 0 && ((veh._jumpZ || 0) > 0.001 || Math.abs(veh._jumpVelZ || 0) > 0.001)) {
      const baseZ = veh._mountBaseZ ?? 0;
      const prevVel = veh._jumpVelZ || 0;
      veh._jumpVelZ = prevVel - 14.5 * dt;
      veh._jumpZ = (veh._jumpZ || 0) + veh._jumpVelZ * dt;
      if (veh._jumpZ <= 0) {
        if (prevVel < -2.2) {
          const gz = this.renderer.getTerrainElevationAt(veh.x, veh.y);
          this.renderer.spawnParticles("smoke", veh.x, veh.y, gz + 0.12, 6);
        }
        veh._jumpZ = 0;
        veh._jumpVelZ = 0;
      }
      veh.z = baseZ + veh._jumpZ;
    }

    const sy = veh.scale_y > 0.01 ? veh.scale_y : (veh.scale || 1.0);
    const sz = veh.scale_z > 0.01 ? veh.scale_z : (veh.scale || 1.0);
    const yawRad = ((veh.yaw || 0) * Math.PI) / 180;
    const fwdWorld = cfg.seat_style === "inside" ? 0 : (cfg.seat_forward || 0) * sy;

    const px = veh.x + (-Math.sin(yawRad)) * fwdWorld;
    const py = veh.y + Math.cos(yawRad) * fwdWorld;

    const vehGround = this.renderer.getTerrainElevationAt(veh.x, veh.y);
    const plGround = this.renderer.getTerrainElevationAt(px, py);
    const meta = this.assetMap.get(veh.asset_id);
    const sh = (meta?.slice_count || 36) * PX_TO_WORLD * sz;
    const seatOffsetZ = cfg.seat_style === "inside" ? Math.max(0.55, sh * 0.45) : (cfg.seat_z ?? 0.52) * sz;

    pl.x = px;
    pl.y = py;
    pl.z = Math.max(0, vehGround + (veh.z || 0) + seatOffsetZ - plGround);
    pl.yaw = veh.yaw;
    pl.velZ = 0;
    pl.playerHiddenInVehicle = cfg.seat_style === "inside";
    pl.anim_mode = "static";

    this.renderer.camera.walkPos = [pl.x, pl.y, plGround + pl.z];
  }

  /// 讓玩家安全下車／下馬／下船（自動尋找合法陸地或橋面落點）
  dismountPlayer(force = false) {
    const pl = this.player;
    if (!pl.mountedEntityId) return { ok: false, reason: "not_mounted" };

    const veh = this.entities.find((e) => e.id === pl.mountedEntityId);
    const cfg = pl.mountConfig || (veh ? this.getVehicleConfig(veh) : null);

    let safeX = pl.x;
    let safeY = pl.y;
    let foundShore = false;

    if (veh && cfg) {
      if (cfg.mobility === "water") {
        // 從水域向外搜尋最近的合法陸地或橋面碼頭
        outerWater: for (const r of [1.0, 1.5, 2.0, 2.6, 3.2, 3.8]) {
          for (let i = 0; i < 16; i++) {
            const ang = (i / 16) * Math.PI * 2;
            const cx = veh.x + Math.cos(ang) * r;
            const cy = veh.y + Math.sin(ang) * r;
            if (this.canStandAt("__player__", "land", cx, cy, null, 0.30)) {
              safeX = cx;
              safeY = cy;
              foundShore = true;
              break outerWater;
            }
          }
        }
        if (!foundShore && !force) {
          soundEngine.play("interact");
          const gz = this.renderer.getTerrainElevationAt(veh.x, veh.y);
          this.renderer.addSpeechBubble("__player__", veh.x, veh.y, gz + 0.5, "附近都是水域，請靠近岸邊或木橋再按 G 下船！", 2.8);
          return { ok: false, reason: "water_no_shore" };
        }
      } else if (cfg.mobility === "air") {
        // 空中載具檢查正下方或半徑 3.5 格內是否有安全陸地可降落
        if (this.canStandAt("__player__", "land", veh.x, veh.y, null, 0.30)) {
          safeX = veh.x;
          safeY = veh.y;
          foundShore = true;
        } else {
          outerAir: for (const r of [0.9, 1.5, 2.2, 3.0, 3.5]) {
            for (let i = 0; i < 12; i++) {
              const ang = (i / 12) * Math.PI * 2;
              const cx = veh.x + Math.cos(ang) * r;
              const cy = veh.y + Math.sin(ang) * r;
              if (this.canStandAt("__player__", "land", cx, cy, null, 0.30)) {
                safeX = cx;
                safeY = cy;
                foundShore = true;
                break outerAir;
              }
            }
          }
        }
        if (!foundShore && !force) {
          soundEngine.play("interact");
          const gz = this.renderer.getTerrainElevationAt(veh.x, veh.y);
          this.renderer.addSpeechBubble("__player__", pl.x, pl.y, gz + (veh.z || 0), "下方是水域或障礙，請飛到陸地上空再按 G 降落！", 2.8);
          return { ok: false, reason: "air_no_land" };
        }
        if (pl.mountControlMode === "drive" && (veh.z || 0) > 0.3) {
          veh.z = 0;
          veh._mountBaseZ = 0;
        }
      } else {
        // 陸地載具／馬匹優先停在車側右／左／後／前
        const yawRad = ((veh.yaw || 0) * Math.PI) / 180;
        const rx = Math.cos(yawRad);
        const ry = Math.sin(yawRad);
        const fx = -Math.sin(yawRad);
        const fy = Math.cos(yawRad);
        const dirs = [
          [rx, ry],
          [-rx, -ry],
          [-fx, -fy],
          [fx, fy],
        ];
        let placed = false;
        for (const dist of [0.95, 1.35]) {
          for (const [dx, dy] of dirs) {
            const cx = veh.x + dx * dist;
            const cy = veh.y + dy * dist;
            if (this.canStandAt("__player__", "land", cx, cy, null, 0.28)) {
              safeX = cx;
              safeY = cy;
              placed = true;
              break;
            }
          }
          if (placed) break;
        }
        if (!placed) {
          const [fx2, fy2] = this._findSafeLandPoint(veh.x, veh.y, false);
          safeX = fx2;
          safeY = fy2;
        }
      }

      if (veh._jumpZ) {
        veh.z = veh._mountBaseZ ?? 0;
        veh._jumpZ = 0;
        veh._jumpVelZ = 0;
      }
      if (veh._origAnimMode !== undefined) {
        veh.anim_mode = veh._origAnimMode;
        delete veh._origAnimMode;
      }
    } else if (force) {
      const [fx2, fy2] = this._findSafeLandPoint(pl.x, pl.y, false);
      safeX = fx2;
      safeY = fy2;
    }

    pl.mountedEntityId = null;
    pl.mountControlMode = "drive";
    pl.mountConfig = null;
    pl.playerHiddenInVehicle = false;
    pl.x = safeX;
    pl.y = safeY;
    pl.z = 0;
    pl.velZ = 0;
    pl.moving = false;
    pl.anim_mode = "static";

    const gz = this.renderer.getTerrainElevationAt(pl.x, pl.y);
    this.renderer.camera.walkPos = [pl.x, pl.y, gz];
    soundEngine.play("dismount");
    this.onUiSync?.("mount", null);
    return { ok: true, vehicle: veh || null };
  }

  /// 熱鍵 G：若已搭乘則下車；否則搭乘身旁最近載具
  toggleMountNearest() {
    if (this.player.mountedEntityId) {
      return this.dismountPlayer(false);
    }
    const found = this.findNearestRideable();
    if (!found) {
      const gz = this.renderer.getTerrainElevationAt(this.player.x, this.player.y);
      this.renderer.addSpeechBubble(
        "__player__",
        this.player.x,
        this.player.y,
        gz,
        "靠近汽車、船、馬或馬車再按 G 就可以搭乘喔！",
        2.3,
      );
      return { ok: false, reason: "none_nearby" };
    }
    return this.mountEntity(found.entity.id);
  }

  /// 熱鍵 Tab：在「親自駕駛」與「自動巡航乘客」間切換（需載具支援 both 且綁有啟用路徑）
  toggleMountControlMode() {
    const pl = this.player;
    if (!pl.mountedEntityId) return { ok: false, reason: "not_mounted" };
    const veh = this.entities.find((e) => e.id === pl.mountedEntityId);
    const cfg = pl.mountConfig || (veh ? this.getVehicleConfig(veh) : null);
    if (!veh || !cfg || cfg.mode !== "both") {
      return { ok: false, reason: "mode_locked" };
    }
    const nextMode = pl.mountControlMode === "drive" ? "passenger" : "drive";
    if (nextMode === "passenger" && !this.hasActivePathFor(veh.id)) {
      const gz = this.renderer.getTerrainElevationAt(veh.x, veh.y) + (veh.z || 0);
      this.renderer.addSpeechBubble("__player__", pl.x, pl.y, gz, "此載具沒有設定巡航路徑，維持親自駕駛！", 2.4);
      return { ok: false, reason: "no_path" };
    }
    pl.mountControlMode = nextMode;
    soundEngine.play("mount");
    const gz = this.renderer.getTerrainElevationAt(veh.x, veh.y) + (veh.z || 0);
    this.renderer.addSpeechBubble(
      "__player__",
      pl.x,
      pl.y,
      gz,
      nextMode === "passenger" ? "已切換為「自動巡航乘客」" : "已接手「親自駕駛」方向盤！",
      2.2,
    );
    this.onUiSync?.("mount", this.getMountedState());
    return { ok: true, controlMode: nextMode };
  }

  /// 駕駛飛行載具時微調離地高度（dir: +1 上升 / -1 下降）
  adjustMountedAltitude(dir, dt) {
    const pl = this.player;
    if (!pl.mountedEntityId || pl.mountControlMode !== "drive") return false;
    if (pl.mountConfig?.mobility !== "air") return false;
    const veh = this.entities.find((e) => e.id === pl.mountedEntityId && e.visible);
    if (!veh) return false;
    veh.z = Math.max(0.25, Math.min(10.0, (veh.z || 0) + dir * 4.2 * dt));
    veh._mountBaseZ = veh.z;
    this._syncMountedPlayer(0);
    return true;
  }

  /// 讓玩家化身進行平面移動（上帝視角與街道漫步共同使用，徹底防落水／防穿建築家具牆體；搭乘時改由載具承接控制）
  movePlayerBy(dx, dy, dt) {
    const pl = this.player;
    if (!pl.visible) return false;

    // 搭乘交通工具中
    if (pl.mountedEntityId) {
      const veh = this.entities.find((e) => e.id === pl.mountedEntityId && e.visible);
      if (!veh) {
        this.dismountPlayer(true);
        return false;
      }
      if (pl.mountControlMode === "passenger") {
        pl.moving = false;
        return false;
      }

      const cfg = pl.mountConfig || this.getVehicleConfig(veh);
      const vehMeta = this.assetMap.get(veh.asset_id);
      const mobility = cfg?.mobility || this.getActorMobility(vehMeta, veh.name, veh.z || 0);
      const mul = cfg?.speed_mul || 1.6;
      const mdx = dx * mul;
      const mdy = dy * mul;

      let moved = false;
      if (mobility === "air") {
        const w = this.renderer.worldWidth || 40;
        const h = this.renderer.worldHeight || 40;
        const nx = Math.max(0.6, Math.min(w - 0.6, veh.x + mdx));
        const ny = Math.max(0.6, Math.min(h - 0.6, veh.y + mdy));
        moved = Math.hypot(nx - veh.x, ny - veh.y) > 0.0015;
        veh.x = nx;
        veh.y = ny;
        if ((veh.z || 0) < 0.35) {
          veh.z = 0.55;
          veh._mountBaseZ = veh.z;
        }
      } else {
        const vehCol = this.getEntityCollider(veh);
        const vehRadius = vehCol ? Math.max(0.30, Math.min(0.62, Math.min(vehCol.hx, vehCol.hy))) : 0.34;
        const curGround = this.renderer.getTerrainElevationAt(veh.x, veh.y);
        const footZ = curGround + (veh.z || 0);
        const res = this.tryMoveActor(veh.id, mobility, veh.x, veh.y, footZ, mdx, mdy, vehRadius, false);
        moved = Math.hypot(res.x - veh.x, res.y - veh.y) > 0.0015;
        veh.x = res.x;
        veh.y = res.y;
      }

      const turnSpeed = vehMeta?.category === "animal" ? 11.0 : 8.5;
      if (Math.hypot(mdx, mdy) > 0.001) {
        const targetYaw = (Math.atan2(-mdx, mdy) * 180) / Math.PI;
        veh.yaw = lerpAngleDeg(veh.yaw, targetYaw, Math.min(1, dt * turnSpeed));
      }

      if (moved) {
        if (vehMeta?.category === "animal") veh.anim_mode = "walk";
        else if (mobility !== "air") veh.anim_mode = "bob";
        pl.moving = true;
        if (Math.random() < 0.22) {
          const gz = this.renderer.getTerrainElevationAt(veh.x, veh.y);
          this.renderer.spawnParticles(mobility === "water" ? "sparkle" : "smoke", veh.x, veh.y, gz + 0.12, 2);
        }
        this._syncMountedPlayer(0);
        return true;
      }

      pl.moving = false;
      this._syncMountedPlayer(0);
      return false;
    }

    const curGround = this.renderer.getTerrainElevationAt(pl.x, pl.y);
    const footZ = curGround + (pl.z || 0);

    const res = this.tryMoveActor("__player__", "land", pl.x, pl.y, footZ, dx, dy, pl.radius);
    const movedDist = Math.hypot(res.x - pl.x, res.y - pl.y);

    if (movedDist > 0.0015) {
      const nextGround = this.renderer.getTerrainElevationAt(res.x, res.y);
      // 保持世界高度平滑過渡至新地面（避免走上橋面或台階時瞬間彈飛或沉沒）
      const prevWorldZ = curGround + (pl.z || 0);
      pl.x = res.x;
      pl.y = res.y;
      pl.z = Math.max(0, prevWorldZ - nextGround);

      const targetYaw = (Math.atan2(-dx, dy) * 180) / Math.PI;
      pl.yaw = lerpAngleDeg(pl.yaw, targetYaw, Math.min(1, dt * 12));
      pl.anim_mode = "walk";
      pl.moving = true;
      return true;
    }

    // 即使前方有牆或河流擋住，仍然讓玩家轉向面向操作方向以呈現推擠回饋
    if (Math.hypot(dx, dy) > 0.001) {
      const targetYaw = (Math.atan2(-dx, dy) * 180) / Math.PI;
      pl.yaw = lerpAngleDeg(pl.yaw, targetYaw, Math.min(1, dt * 10));
    }
    pl.moving = false;
    return false;
  }

  jumpPlayer() {
    const pl = this.player;
    if (!pl.visible) return false;
    if (pl.mountedEntityId) {
      if (pl.mountControlMode === "passenger" || pl.mountConfig?.mobility === "air") return false;
      const veh = this.entities.find((e) => e.id === pl.mountedEntityId && e.visible);
      if (!veh) return false;
      const meta = this.assetMap.get(veh.asset_id);
      if (meta?.category === "animal") {
        if ((veh._jumpZ || 0) > 0.05 || Math.abs(veh._jumpVelZ || 0) > 0.05) return false;
        veh._mountBaseZ = veh.z || 0;
        veh._jumpVelZ = 5.4;
        soundEngine.play("jump");
        return true;
      }
      // 車輛或船隻按空白鍵鳴笛提示
      const now = performance.now();
      if (now - this._lastHornMs > 800) {
        this._lastHornMs = now;
        soundEngine.play("horn");
        const gz = this.renderer.getTerrainElevationAt(veh.x, veh.y) + (veh.z || 0);
        this.renderer.addSpeechBubble(veh.id, veh.x, veh.y, gz, "嘟嘟！", 1.2);
      }
      return true;
    }
    if (pl.z > 0.05 || Math.abs(pl.velZ) > 0.05) return false;
    pl.velZ = 5.2;
    soundEngine.play("jump");
    return true;
  }

  _stepPlayerPhysics(dt) {
    const pl = this.player;
    if (!pl.visible) return;
    if (pl.mountedEntityId) return;
    const ground = this.renderer.getTerrainElevationAt(pl.x, pl.y);
    let worldZ = ground + (pl.z || 0);

    if (pl.z > 0.001 || Math.abs(pl.velZ) > 0.001) {
      const prevVel = pl.velZ;
      pl.velZ -= 14.0 * dt;
      worldZ += pl.velZ * dt;
      if (worldZ <= ground) {
        if (prevVel < -2.2) {
          this.renderer.spawnParticles("smoke", pl.x, pl.y, ground + 0.12, 6);
        }
        worldZ = ground;
        pl.velZ = 0;
      }
      pl.z = Math.max(0, worldZ - ground);
    }

    if (!pl.moving && pl.z <= 0.05) {
      pl.anim_mode = "static";
    }
    this.renderer.camera.walkPos = [pl.x, pl.y, ground + pl.z];
  }

  // ---------- 60fps 主模擬迴圈 ----------

  step(dt) {
    if (this.paused) return;
    const nowMs = performance.now();

    const prevT = this.timeOfDay;
    if (this.timeSpeed > 0) {
      this.timeOfDay = (this.timeOfDay + dt * 0.06 * this.timeSpeed) % 24.0;
    }

    for (const ent of this.entities) {
      if (ent._reveal !== undefined && ent._reveal < 1.0) {
        ent._reveal = Math.min(1.0, ent._reveal + dt * 2.4);
      }
    }
    this._stepTweens(dt);
    this._stepPlayerPhysics(dt);

    // 追蹤目前具有動態位移能力的物件 ID（供自動相遇檢測使用）
    const movingIds = new Set();

    // 1. 路徑巡航（亦遵循地形水陸與建築靜態阻擋檢查；玩家親自駕駛時暫停自動巡航）
    for (const p of this.paths) {
      if (!p.active || !p.entity_id || !p.waypoints || p.waypoints.length < 2) continue;
      const ent = this.entities.find((e) => e.id === p.entity_id);
      if (!ent) continue;
      if (ent.id === this.player.mountedEntityId && this.player.mountControlMode === "drive") continue;
      movingIds.add(ent.id);

      let st = this.pathStates.get(p.id);
      if (!st) {
        st = { seg: 0, dir: 1, progress: 0, waitLeft: 0 };
        this.pathStates.set(p.id, st);
      }

      if (st.waitLeft > 0) {
        st.waitLeft -= dt;
        continue;
      }

      const wps = p.waypoints;
      const idxA = st.seg;
      let idxB = idxA + st.dir;
      if (idxB >= wps.length || idxB < 0) {
        if (p.loop_mode === "loop") {
          st.seg = 0;
          idxB = 1;
        } else if (p.loop_mode === "pingpong") {
          st.dir *= -1;
          idxB = idxA + st.dir;
        } else {
          continue;
        }
      }

      const a = wps[idxA];
      const b = wps[idxB];
      const segLen = Math.max(0.05, Math.hypot(b.x - a.x, b.y - a.y, (b.z || 0) - (a.z || 0)));
      const nextProgress = st.progress + ((p.speed || 2.0) * dt) / segLen;

      const meta = this.assetMap.get(ent.asset_id);
      const mobility = this.getActorMobility(meta, ent.name, ent.z || 0);
      const footZ = this.renderer.getTerrainElevationAt(ent.x, ent.y) + (ent.z || 0);

      let desiredX;
      let desiredY;
      let desiredZ;
      if (nextProgress >= 1.0) {
        desiredX = b.x;
        desiredY = b.y;
        desiredZ = b.z || 0;
      } else {
        const t = p.smooth ? nextProgress * nextProgress * (3 - 2 * nextProgress) : nextProgress;
        desiredX = a.x + (b.x - a.x) * t;
        desiredY = a.y + (b.y - a.y) * t;
        desiredZ = (a.z || 0) + ((b.z || 0) - (a.z || 0)) * t;
      }

      const moveDx = desiredX - ent.x;
      const moveDy = desiredY - ent.y;

      if (mobility === "air") {
        ent.x = desiredX;
        ent.y = desiredY;
        ent.z = desiredZ;
        st.progress = nextProgress;
      } else {
        const res = this.tryMoveActor(ent.id, mobility, ent.x, ent.y, footZ, moveDx, moveDy, 0.24, true);
        if (!res.blocked || Math.hypot(moveDx, moveDy) < 0.002) {
          ent.x = res.x;
          ent.y = res.y;
          ent.z = desiredZ;
          st.progress = nextProgress;
        } else {
          // 遇到建築／水面或玩家新建的障礙時暫停轉向，不強行穿越
          st.waitLeft = 0.4;
          continue;
        }
      }

      if (st.progress >= 1.0) {
        st.progress = 0;
        st.seg = idxB;
        st.waitLeft = b.wait_sec || 0;
        if (p.loop_mode === "loop" && st.seg === wps.length - 1) {
          const wrapLen = Math.max(0.05, Math.hypot(wps[0].x - b.x, wps[0].y - b.y));
          if (wrapLen < 0.8) st.seg = 0;
        }
      } else {
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        if (Math.hypot(dx, dy) > 0.01) {
          const targetYaw = (Math.atan2(-dx, dy) * 180) / Math.PI;
          ent.yaw = lerpAngleDeg(ent.yaw, targetYaw, Math.min(1, dt * 8));
        }
      }
    }

    // 2. 自由漫遊避障 AI（嚴格避開水域、牆壁與建築/家具/道具/樹幹；玩家騎乘駕馭時暫停 AI）
    for (const zone of this.wanderZones) {
      for (const eid of zone.entity_ids || []) {
        const ent = this.entities.find((e) => e.id === eid);
        if (!ent) continue;
        if (ent.id === this.player.mountedEntityId && this.player.mountControlMode === "drive") continue;
        movingIds.add(ent.id);

        const meta = this.assetMap.get(ent.asset_id);
        const mobility = this.getActorMobility(meta, ent.name, ent.z || 0);
        const footZ = this.renderer.getTerrainElevationAt(ent.x, ent.y) + (ent.z || 0);

        let ws = this.wanderStates.get(eid);
        if (!ws) {
          ws = { tx: ent.x, ty: ent.y, waitTimer: 0.25, blockedTime: 0 };
          this.wanderStates.set(eid, ws);
        }

        // 若原有靜態生成點不慎在水裡或障礙物中，立即就近自動推出
        if (mobility !== "air" && !this.isTerrainWalkableFor(mobility, ent.x, ent.y, null)) {
          const [sx, sy] = this._findSafeLandPoint(ent.x, ent.y, false);
          if (mobility === "land") {
            ent.x = sx;
            ent.y = sy;
          }
        }

        if (ws.waitTimer > 0) {
          ws.waitTimer -= dt;
          if (ws.waitTimer <= 0) {
            this._pickValidWanderTarget(ent, zone, mobility, footZ, ws);
          }
          continue;
        }

        const dx = ws.tx - ent.x;
        const dy = ws.ty - ent.y;
        const dist = Math.hypot(dx, dy);
        if (dist < 0.16) {
          ws.waitTimer = 0.9 + Math.random() * 2.2;
          ws.blockedTime = 0;
        } else {
          const stepLen = Math.min(dist, (zone.speed || 1.2) * dt);
          const mx = (dx / dist) * stepLen;
          const my = (dy / dist) * stepLen;

          if (mobility === "air") {
            ent.x += mx;
            ent.y += my;
            const targetYaw = (Math.atan2(-dx, dy) * 180) / Math.PI;
            ent.yaw = lerpAngleDeg(ent.yaw, targetYaw, Math.min(1, dt * 6.5));
          } else {
            const res = this.tryMoveActor(ent.id, mobility, ent.x, ent.y, footZ, mx, my, 0.24, false);
            const actualMoved = Math.hypot(res.x - ent.x, res.y - ent.y);
            ent.x = res.x;
            ent.y = res.y;

            if (actualMoved < stepLen * 0.25) {
              ws.blockedTime = (ws.blockedTime || 0) + dt;
              if (ws.blockedTime >= 0.35) {
                // 遇水或遇建築家具阻擋即刻換向另尋出路
                ws.blockedTime = 0;
                ws.waitTimer = 0.15;
              }
            } else {
              ws.blockedTime = 0;
              const targetYaw = (Math.atan2(-(res.x - (ent.x - actualMoved * (dx / dist)) || dx), dy) * 180) / Math.PI;
              void targetYaw;
              const headingYaw = (Math.atan2(-mx, my) * 180) / Math.PI;
              ent.yaw = lerpAngleDeg(ent.yaw, headingYaw, Math.min(1, dt * 6.5));
            }
          }
        }
      }
    }

    // 父子附掛同步（高度乘數依父物件 Z 軸高度比例）
    const entMap = new Map(this.entities.map((e) => [e.id, e]));
    for (const ent of this.entities) {
      if (ent.parent_id && entMap.has(ent.parent_id)) {
        const parent = entMap.get(ent.parent_id);
        const pz = parent.scale_z ?? parent.scale;
        ent.x = parent.x;
        ent.y = parent.y;
        ent.z = (parent.z || 0) + 0.65 * pz;
        ent.yaw = parent.yaw;
      }
    }

    // 玩家搭乘交通工具同步（含停下時還原坐騎靜止姿態與騎乘跳躍重力）
    if (this.player.mountedEntityId) {
      const mVeh = entMap.get(this.player.mountedEntityId);
      if (mVeh && this.player.mountControlMode === "drive" && !this.player.moving) {
        const mMeta = this.assetMap.get(mVeh.asset_id);
        if (mMeta?.category === "animal") mVeh.anim_mode = "static";
        else if (mVeh._origAnimMode !== undefined) mVeh.anim_mode = mVeh._origAnimMode;
      }
      this._syncMountedPlayer(dt);
    }

    // 追蹤玩家身邊最近的可互動物件、可搭乘載具與可進出之門（上帝視角與街道漫步皆適用 [F]/[G] 鍵）
    if (this.player.visible) {
      this.nearestToPlayer = this._findNearestToPlayer(2.8);
      this.nearestRideable = this.player.mountedEntityId ? null : this.findNearestRideable();
      this.nearestPortal = this.findNearestPortal(2.35);
    } else {
      this.nearestToPlayer = null;
      this.nearestRideable = null;
      this.nearestPortal = null;
    }

    // 執行自訂事件規則（時鐘、週期、接近、日夜、兩物件相遇）
    this._evaluateRules(prevT, this.timeOfDay, nowMs);

    // 自動物件相遇互動檢查（包含玩家角色與場景角色）
    this.autoEncounterTimer += dt;
    if (this.autoEncounterEnabled && this.autoEncounterTimer >= 0.15) {
      this.autoEncounterTimer = 0;
      this._evaluateAutoEncounters(movingIds, nowMs);
    }

    // 偵測玩家是否踏入搶劫／埋伏等近距任務自動觸發區
    this.onProximityCheck?.(this.player.x, this.player.y, nowMs);

    this.prevTimeOfDay = this.timeOfDay;
  }

  _pickValidWanderTarget(ent, zone, mobility, footZ, ws) {
    for (let attempt = 0; attempt < 14; attempt++) {
      const ang = Math.random() * Math.PI * 2;
      const r = Math.sqrt(Math.random()) * zone.radius * 0.88;
      const candX = zone.cx + Math.cos(ang) * r;
      const candY = zone.cy + Math.sin(ang) * r;
      if (mobility === "air" || this.canStandAt(ent.id, mobility, candX, candY, footZ, 0.25)) {
        ws.tx = candX;
        ws.ty = candY;
        ws.blockedTime = 0;
        return;
      }
    }
    ws.tx = ent.x;
    ws.ty = ent.y;
    ws.waitTimer = 0.8;
  }

  _findNearestToPlayer(maxDist) {
    let best = null;
    let bestD = maxDist;
    for (const ent of this.entities) {
      if (!ent.visible || ent.id === this.player.mountedEntityId) continue;
      const d = Math.hypot(ent.x - this.player.x, ent.y - this.player.y);
      if (d < bestD) {
        bestD = d;
        best = ent;
      }
    }
    return best;
  }

  // ---------- 建築物「門」與跨世界傳送邏輯 ----------

  /// 取得實體門前世界座標（未傳 portal 時自動解析 entity.meta_json.portal）
  getPortalWorldPos(ent, portal = null) {
    if (!ent || !ent.visible) return null;
    const p = portal || parseEntityPortal(ent);
    if (!p) return null;
    const meta = this.assetMap.get(ent.asset_id);
    const sy = ent.scale_y > 0.01 ? ent.scale_y : (ent.scale || 1.0);
    const defaultFwd = meta?.category === "building" ? 1.15 : 0.45;
    const fwd = Number(p.door_offset_fwd ?? defaultFwd);
    const yawRad = ((ent.yaw || 0) * Math.PI) / 180;
    return {
      x: ent.x + (-Math.sin(yawRad)) * fwd * sy,
      y: ent.y + Math.cos(yawRad) * fwd * sy,
      portal: p,
    };
  }

  /// 尋找玩家附近最近且已啟用的門（兼顧大型建築物門口與實體邊緣距離）
  findNearestPortal(maxDist = 2.35) {
    if (!this.player.visible) return null;
    let best = null;
    let bestDist = Infinity;
    for (const ent of this.entities) {
      if (!ent.visible) continue;
      const p = parseEntityPortal(ent);
      if (!p) continue;
      const doorPos = this.getPortalWorldPos(ent, p);
      if (!doorPos) continue;
      const dDoor = Math.hypot(doorPos.x - this.player.x, doorPos.y - this.player.y);
      const col = this.getEntityCollider(ent);
      const bodyPad = col ? Math.max(col.hx, col.hy) : 0.5;
      const dBody = Math.max(0, Math.hypot(ent.x - this.player.x, ent.y - this.player.y) - bodyPad);
      const d = Math.min(dDoor, dBody + 0.35);
      if (d <= maxDist && d < bestDist) {
        bestDist = d;
        best = { entity: ent, portal: p, doorPos, dist: d };
      }
    }
    return best;
  }

  /// 檢查開門條件並執行進門／出門傳送
  _tryEnterPortal(ent, portal) {
    if (!ent || !portal) return false;
    const lockRes = this.checkPortalLock ? this.checkPortalLock(portal) : { locked: false };
    if (lockRes && lockRes.locked) {
      soundEngine.play("locked");
      const pPos = this.getPortalWorldPos(ent, portal) || { x: ent.x, y: ent.y };
      const gz = this.renderer.getTerrainElevationAt(pPos.x, pPos.y) + (ent.z || 0);
      this.renderer.addSpeechBubble(
        ent.id,
        pPos.x,
        pPos.y,
        gz,
        lockRes.reason || "門鎖住了，還缺少開門條件！",
        3.0,
      );
      return false;
    }

    // 若玩家正在駕駛／搭乘載具，進門前自動先於門口安全下車
    if (this.player.mountedEntityId) {
      this.dismountPlayer(true);
    }

    const pPos = this.getPortalWorldPos(ent, portal) || { x: ent.x, y: ent.y };
    const gz = this.renderer.getTerrainElevationAt(pPos.x, pPos.y) + (ent.z || 0);
    soundEngine.play("door");
    this.renderer.spawnParticles("sparkle", pPos.x, pPos.y, gz + 0.8, 22);
    if (this.onPortalTrigger) {
      this.onPortalTrigger(ent, portal);
    }
    return true;
  }

  /// 按 F 鍵與玩家身旁最近物件（或門）即時互動（上帝視角與街道漫步皆可使用）
  interactNearestToPlayer(maxDist = 2.8) {
    const target = this._findNearestToPlayer(maxDist);
    const nearPortal = this.nearestPortal;

    if (nearPortal) {
      const targetIsOtherNonPortal =
        target &&
        target.id !== nearPortal.entity.id &&
        !parseEntityPortal(target) &&
        Math.hypot(target.x - this.player.x, target.y - this.player.y) < nearPortal.dist - 0.55;
      if (!targetIsOtherNonPortal) {
        return this._tryEnterPortal(nearPortal.entity, nearPortal.portal);
      }
    }

    if (!target) {
      const gz = this.renderer.getTerrainElevationAt(this.player.x, this.player.y);
      this.renderer.addSpeechBubble(
        "__player__",
        this.player.x,
        this.player.y,
        gz,
        "走近物件或門邊一點再按 F 就可以互動喔！",
        2.2,
      );
      return false;
    }
    const dx = target.x - this.player.x;
    const dy = target.y - this.player.y;
    if (Math.hypot(dx, dy) > 0.08) {
      this.player.yaw = (Math.atan2(-dx, dy) * 180) / Math.PI;
    }
    this._bounceEntity("__player__", 1.12, 0.35);
    return this.interactWithEntity(target.id, true);
  }

  _stepTweens(dt) {
    const rest = [];
    for (const tw of this.tweens) {
      tw.elapsed += dt;
      const ent = tw.entityId === "__player__"
        ? this.player
        : this.entities.find((e) => e.id === tw.entityId);
      if (!ent) continue;
      if (tw.elapsed >= tw.duration) {
        ent._tweenMul = 1.0;
      } else {
        const p = tw.elapsed / tw.duration;
        const wave = Math.sin(p * Math.PI);
        ent._tweenMul = 1.0 + (tw.targetMul - 1.0) * wave;
        rest.push(tw);
      }
    }
    this.tweens = rest;
  }

  _resolveActorPos(id) {
    if (id === "__player__") {
      return this.player;
    }
    return this.entities.find((e) => e.id === id) || null;
  }

  _evaluateRules(prevT, currT, nowMs) {
    const isNight = currT < 6.0 || currT >= 18.5;

    for (const rule of this.events) {
      if (!rule.enabled) continue;
      const lastFired = this.eventCooldowns.get(rule.id) || 0;
      if ((nowMs - lastFired) * 0.001 < (rule.cooldown_sec || 1.0)) continue;

      const cfg = rule.trigger_config || {};
      let fired = false;

      if (rule.trigger_type === "clock" && this.timeSpeed > 0) {
        const targetH = (Number(cfg.hour || 0) + Number(cfg.minute || 0) / 60.0) % 24.0;
        if (crossedHour(prevT, currT, targetH)) fired = true;
      } else if (rule.trigger_type === "interval") {
        const sec = Math.max(2, Number(cfg.seconds || 10));
        const prevTick = this.lastIntervalTimes.get(rule.id) || nowMs;
        if ((nowMs - prevTick) * 0.001 >= sec) {
          this.lastIntervalTimes.set(rule.id, nowMs);
          fired = true;
        }
      } else if (rule.trigger_type === "proximity") {
        const targetEnt = this._resolveActorPos(cfg.entity_id);
        if (targetEnt) {
          const dist = Math.hypot(targetEnt.x - Number(cfg.x || 0), targetEnt.y - Number(cfg.y || 0));
          const inside = dist <= Number(cfg.radius || 2.5);
          const wasInside = this.lastProximityInside.has(rule.id);
          if (inside && !wasInside) {
            this.lastProximityInside.add(rule.id);
            fired = true;
          } else if (!inside && wasInside) {
            this.lastProximityInside.delete(rule.id);
          }
        }
      } else if (rule.trigger_type === "encounter") {
        const a = this._resolveActorPos(cfg.entity_a);
        const b = this._resolveActorPos(cfg.entity_b);
        if (a && b && cfg.entity_a !== cfg.entity_b) {
          const dist = Math.hypot(a.x - b.x, a.y - b.y);
          const inside = dist <= Number(cfg.radius || 1.8);
          const wasInside = this.lastEncounterInside.has(rule.id);
          if (inside && !wasInside) {
            this.lastEncounterInside.add(rule.id);
            fired = true;
          } else if (!inside && dist > Number(cfg.radius || 1.8) + 0.6) {
            this.lastEncounterInside.delete(rule.id);
          }
        }
      } else if (rule.trigger_type === "day_night") {
        if (isNight !== this.prevIsNight) {
          const want = cfg.mode || "night";
          if ((want === "night" && isNight) || (want === "day" && !isNight)) fired = true;
        }
      }

      if (fired) {
        this.executeRule(rule, nowMs);
      }
    }
    this.prevIsNight = isNight;
  }

  /// 兩物件在世界空間相遇（或玩家控制接近場景角色／動物）時自動產生互動演出
  _evaluateAutoEncounters(movingIds, nowMs) {
    // 1. 玩家角色與周圍人物／動物／載具相遇問候（上帝視角或街道漫步皆觸發；略過目前正搭乘的載具）
    if (this.player.visible && this.player.asset_id) {
      for (const ent of this.entities) {
        if (!ent.visible || ent.id === this.player.mountedEntityId) continue;
        const dist = Math.hypot(ent.x - this.player.x, ent.y - this.player.y);
        const dz = Math.abs((ent.z || 0) - (this.player.z || 0));
        if (dist > 1.75 || dz > 1.5) continue;

        const key = `__player__|${ent.id}`;
        const lastMs = this.autoEncounterCooldowns.get(key) || 0;
        if (nowMs - lastMs < 11000) continue;
        this.autoEncounterCooldowns.set(key, nowMs);

        const cat = this.assetMap.get(ent.asset_id)?.category || "prop";
        const baseZ = this.renderer.getTerrainElevationAt(ent.x, ent.y) + (ent.z || 0);
        if (cat === "character") {
          const dx = this.player.x - ent.x;
          const dy = this.player.y - ent.y;
          if (Math.hypot(dx, dy) > 0.1) ent.yaw = (Math.atan2(-dx, dy) * 180) / Math.PI;
          soundEngine.play("encounter");
          this.renderer.addSpeechBubble(ent.id, ent.x, ent.y, baseZ, `嗨 ${this.player.name}！歡迎來到這裡 ✨`, 3.2);
          this.renderer.addSpeechBubble("__player__", this.player.x, this.player.y, baseZ, `你好呀，${ent.name}！`, 2.8);
          this.renderer.spawnParticles("sparkle", ent.x, ent.y, baseZ + 1.3, 12);
          this._bounceEntity(ent.id, 1.14);
        } else if (cat === "animal") {
          soundEngine.play("interact");
          this.renderer.addSpeechBubble(ent.id, ent.x, ent.y, baseZ, animalCry(ent.name), 3.0);
          this.renderer.addSpeechBubble("__player__", this.player.x, this.player.y, baseZ, `好可愛的 ${ent.name} 🐾`, 2.6);
          this._bounceEntity(ent.id, 1.2);
        } else if (cat === "vehicle") {
          soundEngine.play("horn");
          this.renderer.addSpeechBubble(ent.id, ent.x, ent.y, baseZ, `嗶嗶！${this.player.name} 小心通行喔！`, 2.8);
        }
      }
    }

    // 2. 場景實體彼此之間的相遇互動
    const n = this.entities.length;
    for (let i = 0; i < n; i++) {
      const a = this.entities[i];
      if (!a.visible) continue;
      for (let j = i + 1; j < n; j++) {
        const b = this.entities[j];
        if (!b.visible) continue;
        if (a.parent_id === b.id || b.parent_id === a.id) continue;
        if (!movingIds.has(a.id) && !movingIds.has(b.id)) continue;

        const pairKey = a.id < b.id ? `${a.id}|${b.id}` : `${b.id}|${a.id}`;
        const dist = Math.hypot(a.x - b.x, a.y - b.y);
        const dz = Math.abs((a.z || 0) - (b.z || 0));

        if (dist > 1.45 || dz > 1.4) continue;

        const lastMs = this.autoEncounterCooldowns.get(pairKey) || 0;
        if (nowMs - lastMs < 9500) continue;
        this.autoEncounterCooldowns.set(pairKey, nowMs);

        const catA = this.assetMap.get(a.asset_id)?.category || "prop";
        const catB = this.assetMap.get(b.asset_id)?.category || "prop";
        const midX = (a.x + b.x) * 0.5;
        const midY = (a.y + b.y) * 0.5;
        const baseZ = this.renderer.getTerrainElevationAt(midX, midY);

        if (catA === "vehicle" || catB === "vehicle") {
          const veh = catA === "vehicle" ? a : b;
          const other = catA === "vehicle" ? b : a;
          soundEngine.play("horn");
          this.renderer.addSpeechBubble(veh.id, veh.x, veh.y, baseZ, `嗶嗶！${other.name} 請小心通行～`, 3.2);
          this._bounceEntity(veh.id, 1.12);
        } else if (catA === "character" && catB === "character") {
          soundEngine.play("encounter");
          this.renderer.addSpeechBubble(a.id, a.x, a.y, baseZ, `嗨 ${b.name}！今天真棒！`, 3.2);
          this.renderer.addSpeechBubble(b.id, b.x, b.y, baseZ, `你好呀 ${a.name} ✨`, 3.2);
          this.renderer.spawnParticles("sparkle", midX, midY, baseZ + 1.4, 14);
          this._bounceEntity(a.id, 1.14);
          this._bounceEntity(b.id, 1.14);
        } else if (
          (catA === "character" && catB === "animal") ||
          (catA === "animal" && catB === "character")
        ) {
          const ch = catA === "character" ? a : b;
          const an = catA === "character" ? b : a;
          soundEngine.play("interact");
          this.renderer.addSpeechBubble(ch.id, ch.x, ch.y, baseZ, `好可愛的 ${an.name}！`, 3.0);
          this.renderer.addSpeechBubble(an.id, an.x, an.y, baseZ, animalCry(an.name), 3.0);
          this.renderer.spawnParticles("sparkle", midX, midY, baseZ + 1.2, 16);
          this._bounceEntity(an.id, 1.2);
        } else if (catA === "animal" && catB === "animal") {
          soundEngine.play("bird");
          this.renderer.addSpeechBubble(a.id, a.x, a.y, baseZ, animalCry(a.name), 2.8);
          this.renderer.addSpeechBubble(b.id, b.x, b.y, baseZ, animalCry(b.name), 2.8);
          this._bounceEntity(a.id, 1.18);
          this._bounceEntity(b.id, 1.18);
        } else {
          const actor = movingIds.has(a.id) ? a : b;
          const prop = actor.id === a.id ? b : a;
          if (catA === "prop" || catB === "prop") {
            soundEngine.play("chime");
            this.renderer.addSpeechBubble(actor.id, actor.x, actor.y, baseZ, `逛到「${prop.name}」囉！`, 2.8);
            this.renderer.spawnParticles("sparkle", prop.x, prop.y, baseZ + 1.2, 12);
          }
        }
      }
    }
  }

  _bounceEntity(entityId, mul = 1.16, duration = 0.55) {
    this.tweens.push({
      entityId,
      startMul: 1.0,
      targetMul: mul,
      elapsed: 0,
      duration,
    });
  }

  /// 點擊某實體時執行自訂 click 事件，並於允許完整互動時觸發任務考驗／智慧回應
  interactWithEntity(entityId, triggerDefaultFallback = true) {
    const ent = this.entities.find((e) => e.id === entityId);
    const firedCustom = this.triggerEntityClick(entityId);
    if (!triggerDefaultFallback) return firedCustom;

    if (!ent) return firedCustom;
    const portal = parseEntityPortal(ent);
    if (portal) {
      return this._tryEnterPortal(ent, portal);
    }
    const cat = this.assetMap.get(ent.asset_id)?.category || "prop";
    const baseZ = this.renderer.getTerrainElevationAt(ent.x, ent.y) + (ent.z || 0);

    // 若該實體掛有任務互動（線索／選擇題／搶劫／支援／寶藏／嫌疑犯），優先開啟任務對話考驗視窗
    if (this.onEntityQuestInteract) {
      if (cat === "character" && this.player.visible) {
        const dx = this.player.x - ent.x;
        const dy = this.player.y - ent.y;
        if (Math.hypot(dx, dy) > 0.1) {
          ent.yaw = (Math.atan2(-dx, dy) * 180) / Math.PI;
        }
      }
      if (this.onEntityQuestInteract(ent)) {
        this._bounceEntity(ent.id, 1.18, 0.5);
        return true;
      }
    }

    if (firedCustom) return true;

    this._bounceEntity(ent.id, 1.18, 0.5);

    if (cat === "character") {
      if (this.player.visible) {
        const dx = this.player.x - ent.x;
        const dy = this.player.y - ent.y;
        if (Math.hypot(dx, dy) > 0.1) {
          ent.yaw = (Math.atan2(-dx, dy) * 180) / Math.PI;
        }
      }
      soundEngine.play("interact");
      this.renderer.addSpeechBubble(
        ent.id,
        ent.x,
        ent.y,
        baseZ,
        `你好 ${this.player.name}！我是 ${ent.name}，歡迎來逛逛！`,
        3.5,
      );
      this.renderer.spawnParticles("sparkle", ent.x, ent.y, baseZ + 1.3, 14);
    } else if (cat === "animal") {
      soundEngine.play("bird");
      this.renderer.addSpeechBubble(ent.id, ent.x, ent.y, baseZ, animalCry(ent.name), 3.0);
      this.renderer.spawnParticles("sparkle", ent.x, ent.y, baseZ + 1.1, 16);
    } else if (cat === "vehicle") {
      soundEngine.play("horn");
      this.renderer.addSpeechBubble(ent.id, ent.x, ent.y, baseZ, `嘟嘟！「${ent.name}」準備出發囉！`, 3.0);
    } else if (cat === "plant") {
      soundEngine.play("chime");
      this.renderer.spawnParticles("sakura", ent.x, ent.y, baseZ + 1.8, 22);
      this.renderer.addSpeechBubble(ent.id, ent.x, ent.y, baseZ, `微風吹過「${ent.name}」🍃`, 2.6);
    } else {
      ent.emissive_on = !ent.emissive_on;
      soundEngine.play("chime");
      this.renderer.spawnParticles("sparkle", ent.x, ent.y, baseZ + 1.3, 14);
      this.renderer.addSpeechBubble(
        ent.id,
        ent.x,
        ent.y,
        baseZ,
        `${ent.name}（燈光${ent.emissive_on ? "開啟" : "關閉"}）`,
        2.8,
      );
    }
    return true;
  }

  triggerEntityClick(entityId) {
    const nowMs = performance.now();
    let any = false;
    for (const rule of this.events) {
      if (!rule.enabled || rule.trigger_type !== "click") continue;
      const cfg = rule.trigger_config || {};
      if (cfg.entity_id === entityId) {
        const last = this.eventCooldowns.get(rule.id) || 0;
        if ((nowMs - last) * 0.001 >= (rule.cooldown_sec || 0.5)) {
          this.executeRule(rule, nowMs);
          any = true;
        }
      }
    }
    return any;
  }

  executeRule(rule, nowMs = performance.now()) {
    this.eventCooldowns.set(rule.id, nowMs);
    for (const act of rule.actions || []) {
      switch (act.type) {
        case "play_sound":
          soundEngine.play(act.sound || "chime");
          break;
        case "particle_burst": {
          let px = Number(act.x ?? 20);
          let py = Number(act.y ?? 20);
          let pz = Number(act.z ?? 5);
          if (act.entity_id) {
            const target = this._resolveActorPos(act.entity_id);
            if (target) {
              px = target.x;
              py = target.y;
              pz = this.renderer.getTerrainElevationAt(px, py) + (target.z || 0) + (act.effect === "firework" ? 3.4 : 1.8);
            }
          }
          this.renderer.spawnParticles(act.effect || "firework", px, py, pz, Number(act.count ?? 32));
          break;
        }
        case "speech_bubble": {
          const ent = this._resolveActorPos(act.entity_id);
          this.renderer.addSpeechBubble(
            act.entity_id || "",
            ent ? ent.x : Number(act.x || 20),
            ent ? ent.y : Number(act.y || 20),
            ent ? this.renderer.getTerrainElevationAt(ent.x, ent.y) + (ent.z || 0) : 2.0,
            act.text || "✨",
            Number(act.duration || 4.0),
          );
          break;
        }
        case "emissive_toggle": {
          const on = act.on !== false;
          for (const e of this.entities) {
            if ((act.tag && e.tag === act.tag) || (act.entity_id && e.id === act.entity_id)) {
              e.emissive_on = on;
            }
          }
          break;
        }
        case "spawn_toggle": {
          const vis = act.visible !== false;
          for (const e of this.entities) {
            if ((act.tag && e.tag === act.tag) || (act.entity_id && e.id === act.entity_id)) {
              e.visible = vis;
              if (vis) e._reveal = 0.05;
            }
          }
          break;
        }
        case "path_control": {
          for (const p of this.paths) {
            if (p.id === act.path_id) p.active = act.active !== false;
          }
          break;
        }
        case "scale_tween": {
          this._bounceEntity(act.entity_id, Number(act.scale_mul || 1.25), Number(act.duration || 0.7));
          break;
        }
        case "set_weather": {
          this.weather = act.weather || "clear";
          this.onUiSync?.("weather", this.weather);
          break;
        }
        case "set_time": {
          this.timeOfDay = Number(act.hour ?? 12.0) % 24;
          this.onUiSync?.("time", this.timeOfDay);
          break;
        }
        case "camera_focus": {
          const ent = this._resolveActorPos(act.entity_id);
          if (ent) {
            this.renderer.camera.target = [
              ent.x,
              ent.y,
              this.renderer.getTerrainElevationAt(ent.x, ent.y) + 0.8,
            ];
          }
          break;
        }
        default:
          break;
      }
    }
  }
}

function animalCry(name) {
  const s = name || "";
  if (s.includes("犬") || s.includes("狗")) return "汪汪！🐾";
  if (s.includes("羊")) return "咩咩～☁️";
  if (s.includes("獅")) return "吼——！👑";
  if (s.includes("鴨")) return "呱呱！💧";
  if (s.includes("鳥") || s.includes("鷗")) return "啾啾！🕊️";
  if (s.includes("馬")) return "嘶——！🐎";
  return "好開心！✨";
}

function lerpAngleDeg(a, b, t) {
  let diff = ((b - a + 540) % 360) - 180;
  return (a + diff * t + 360) % 360;
}

function crossedHour(prev, curr, target) {
  if (Math.abs(curr - prev) > 4.0) return false;
  if (prev <= curr) return prev < target && curr >= target;
  return prev < target || curr >= target;
}

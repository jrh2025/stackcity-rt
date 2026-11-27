//! WebGL2 3D GPU Instanced Sprite-Stacking 與微縮地景渲染引擎。
//!
//! 特點：
//! - 真 3D Z-Buffer 深度遮擋與透視投影（支援上帝微縮視角與 CityRT 街道漫步視角）
//! - 每層精靈切片採三疊微厚度重疊，即使貼近街道低角度觀看依然紮實無縫隙
//! - 即時讀取 `normals.png` 計算 24 小時日夜三階像素光影，並依時段自動點亮 `emissive.png`
//! - 階梯體素地形、動態波光水面與四向鄰居智慧拼接路網（柏油／石板／泥土／橋樑）
//! - 2D Overlay HUD 繪製路徑、漫遊圈、粒子特效（煙火／櫻花／雨雪／炊煙）與頭頂對話氣泡

const MAX_SLICES = 80;
const SUB_LAYERS = 3; // 每層切片重疊 3 薄片，徹底消除低視角縫隙
const ELEV_UNIT = 0.52;
const PX_TO_WORLD = 1.0 / 26.0;

function mat4Perspective(fovy, aspect, near, far) {
  const f = 1.0 / Math.tan(fovy * 0.5);
  const nf = 1 / (near - far);
  return new Float32Array([
    f / aspect, 0, 0, 0,
    0, f, 0, 0,
    0, 0, (far + near) * nf, -1,
    0, 0, 2 * far * near * nf, 0,
  ]);
}

function mat4LookAt(eye, center, up) {
  let zx = eye[0] - center[0];
  let zy = eye[1] - center[1];
  let zz = eye[2] - center[2];
  let zlen = Math.hypot(zx, zy, zz) || 1;
  zx /= zlen; zy /= zlen; zz /= zlen;

  let xx = up[1] * zz - up[2] * zy;
  let xy = up[2] * zx - up[0] * zz;
  let xz = up[0] * zy - up[1] * zx;
  let xlen = Math.hypot(xx, xy, xz) || 1;
  xx /= xlen; xy /= xlen; xz /= xlen;

  const yx = zy * xz - zz * xy;
  const yy = zz * xx - zx * xz;
  const yz = zx * xy - zy * xx;

  return new Float32Array([
    xx, yx, zx, 0,
    xy, yy, zy, 0,
    xz, yz, zz, 0,
    -(xx * eye[0] + xy * eye[1] + xz * eye[2]),
    -(yx * eye[0] + yy * eye[1] + yz * eye[2]),
    -(zx * eye[0] + zy * eye[1] + zz * eye[2]),
    1,
  ]);
}

function mat4Mul(a, b) {
  const out = new Float32Array(16);
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) {
      out[c * 4 + r] =
        a[0 * 4 + r] * b[c * 4 + 0] +
        a[1 * 4 + r] * b[c * 4 + 1] +
        a[2 * 4 + r] * b[c * 4 + 2] +
        a[3 * 4 + r] * b[c * 4 + 3];
    }
  }
  return out;
}

function mat4Invert(m) {
  const out = new Float32Array(16);
  const a00 = m[0], a01 = m[1], a02 = m[2], a03 = m[3];
  const a10 = m[4], a11 = m[5], a12 = m[6], a13 = m[7];
  const a20 = m[8], a21 = m[9], a22 = m[10], a23 = m[11];
  const a30 = m[12], a31 = m[13], a32 = m[14], a33 = m[15];

  const b00 = a00 * a11 - a01 * a10;
  const b01 = a00 * a12 - a02 * a10;
  const b02 = a00 * a13 - a03 * a10;
  const b03 = a01 * a12 - a02 * a11;
  const b04 = a01 * a13 - a03 * a11;
  const b05 = a02 * a13 - a03 * a12;
  const b06 = a20 * a31 - a21 * a30;
  const b07 = a20 * a32 - a22 * a30;
  const b08 = a20 * a33 - a23 * a30;
  const b09 = a21 * a32 - a22 * a31;
  const b10 = a21 * a33 - a23 * a31;
  const b11 = a22 * a33 - a23 * a32;

  let det = b00 * b11 - b01 * b10 + b02 * b09 + b03 * b08 - b04 * b07 + b05 * b06;
  if (!det) return null;
  det = 1.0 / det;

  out[0] = (a11 * b11 - a12 * b10 + a13 * b09) * det;
  out[1] = (a02 * b10 - a01 * b11 - a03 * b09) * det;
  out[2] = (a31 * b05 - a32 * b04 + a33 * b03) * det;
  out[3] = (a22 * b04 - a21 * b05 - a23 * b03) * det;
  out[4] = (a12 * b08 - a10 * b11 - a13 * b07) * det;
  out[5] = (a00 * b11 - a02 * b08 + a03 * b07) * det;
  out[6] = (a32 * b02 - a30 * b05 - a33 * b01) * det;
  out[7] = (a20 * b05 - a22 * b02 + a23 * b01) * det;
  out[8] = (a10 * b10 - a11 * b08 + a13 * b06) * det;
  out[9] = (a01 * b08 - a00 * b10 - a03 * b06) * det;
  out[10] = (a30 * b04 - a31 * b02 + a33 * b00) * det;
  out[11] = (a21 * b02 - a20 * b04 - a23 * b00) * det;
  out[12] = (a11 * b07 - a10 * b09 - a12 * b06) * det;
  out[13] = (a00 * b09 - a01 * b07 + a02 * b06) * det;
  out[14] = (a31 * b01 - a30 * b03 - a32 * b00) * det;
  out[15] = (a20 * b03 - a21 * b01 + a22 * b00) * det;
  return out;
}

function hexToRgb01(hex) {
  const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex || "#ffffff");
  if (!m) return [1, 1, 1];
  return [
    parseInt(m[1], 16) / 255,
    parseInt(m[2], 16) / 255,
    parseInt(m[3], 16) / 255,
  ];
}

function compileShader(gl, type, src) {
  const s = gl.createShader(type);
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(s);
    gl.deleteShader(s);
    throw new Error(`著色器編譯錯誤：${log}`);
  }
  return s;
}

function createProgram(gl, vsSrc, fsSrc) {
  const p = gl.createProgram();
  gl.attachShader(p, compileShader(gl, gl.VERTEX_SHADER, vsSrc));
  gl.attachShader(p, compileShader(gl, gl.FRAGMENT_SHADER, fsSrc));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
    throw new Error(`著色器連結錯誤：${gl.getProgramInfoLog(p)}`);
  }
  return p;
}

// ---------- 著色器程式 ----------

const TERRAIN_VS = `#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNorm;
layout(location=2) in vec3 aColor;
layout(location=3) in vec2 aUv;
layout(location=4) in float aIsRoad;

uniform mat4 uViewProj;
out vec3 vNorm;
out vec3 vColor;
out vec2 vUv;
out float vIsRoad;
out vec3 vWorldPos;

void main() {
  vNorm = aNorm;
  vColor = aColor;
  vUv = aUv;
  vIsRoad = aIsRoad;
  vWorldPos = aPos;
  gl_Position = uViewProj * vec4(aPos, 1.0);
}`;

const TERRAIN_FS = `#version 300 es
precision highp float;
in vec3 vNorm;
in vec3 vColor;
in vec2 vUv;
in float vIsRoad;
in vec3 vWorldPos;

uniform vec3 uSunDir;
uniform vec3 uSunColor;
uniform vec3 uAmbient;
uniform float uSnowFactor;
uniform float uRainFactor;
uniform float uNightFactor;

out vec4 outColor;

void main() {
  vec3 base = vColor;
  // 細微像素格紋與道路標線
  vec2 cellUv = fract(vWorldPos.xy);
  float edge = step(0.95, max(cellUv.x, cellUv.y)) + step(min(cellUv.x, cellUv.y), 0.03);
  if (vNorm.z > 0.8) {
    base *= mix(1.0, 0.94, min(edge, 1.0) * 0.65);
    if (uSnowFactor > 0.01 && vIsRoad < 0.5) {
      float n = fract(sin(dot(floor(vWorldPos.xy * 6.0), vec2(12.9898, 78.233))) * 43758.5453);
      if (n > 0.25) {
        base = mix(base, vec3(0.92, 0.96, 1.0), uSnowFactor * 0.82);
      }
    }
  }
  if (uRainFactor > 0.01) {
    base *= (1.0 - 0.14 * uRainFactor);
  }
  float ndl = max(dot(normalize(vNorm), normalize(uSunDir)), 0.0);
  float band = ndl > 0.45 ? 1.05 : (ndl > 0.1 ? 0.86 : 0.68);
  vec3 lit = base * (uAmbient + uSunColor * band * 0.72);
  // 夜晚道路微泛路燈光感
  if (vIsRoad > 0.5 && uNightFactor > 0.1) {
    lit += vec3(0.06, 0.05, 0.03) * uNightFactor;
  }
  outColor = vec4(lit, 1.0);
}`;

const WATER_VS = `#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
uniform mat4 uViewProj;
uniform float uTime;
out vec3 vPos;
void main() {
  vec3 p = aPos;
  p.z += sin(p.x * 1.9 + uTime * 2.1) * 0.028 + cos(p.y * 2.3 + uTime * 1.7) * 0.022;
  vPos = p;
  gl_Position = uViewProj * vec4(p, 1.0);
}`;

const WATER_FS = `#version 300 es
precision highp float;
in vec3 vPos;
uniform float uTime;
uniform vec3 uSunColor;
uniform vec3 uAmbient;
uniform float uNightFactor;
out vec4 outColor;
void main() {
  float wave = sin(vPos.x * 5.0 + uTime * 3.0) * cos(vPos.y * 5.0 - uTime * 2.4);
  vec3 deepCol = mix(vec3(0.11, 0.42, 0.68), vec3(0.05, 0.16, 0.34), uNightFactor);
  vec3 foam = vec3(0.78, 0.94, 1.0);
  float spec = smoothstep(0.65, 0.95, wave);
  vec3 col = deepCol * (uAmbient + uSunColor * 0.55) + foam * spec * 0.25;
  outColor = vec4(col, 0.84);
}`;

const STACK_VS = `#version 300 es
precision highp float;

// 靜態切片網格頂點：xy in [-0.5, 0.5], zLayer = 0..MAX_SLICES-1, zSub in [0, 0.34, 0.68]
layout(location=0) in vec2 aCorner;
layout(location=1) in float aLayer;
layout(location=2) in float aSubOffset;

// 每實例資料（Instanced attributes）
layout(location=3) in vec3 iPos;
layout(location=4) in float iYaw;
layout(location=5) in vec3 iScale;
layout(location=6) in vec3 iTint;
layout(location=7) in vec4 iFlags; // x: emissiveOn, y: animType (0=static,1=sway,2=walk,3=float,4=bob,5=spin), z: phase, w: reveal

uniform mat4 uViewProj;
uniform float uSliceCount;
uniform float uSliceW;
uniform float uSliceH;
uniform float uTime;
uniform float uPxToWorld;

out vec2 vLocalUv;
out float vLayer;
out float vYaw;
out vec3 vTint;
out float vEmissiveOn;
out float vHeightFrac;

void main() {
  if (aLayer >= uSliceCount * iFlags.w) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }

  float zFrac = aLayer / max(uSliceCount - 1.0, 1.0);
  vec3 pxW = uPxToWorld * iScale;
  float worldW = uSliceW * pxW.x;
  float worldH = uSliceH * pxW.y;
  float worldZ = (aLayer + aSubOffset) * pxW.z;
  float horizScale = max(iScale.x, iScale.y);
  float heightScale = iScale.z;

  float yaw = iYaw;
  vec2 offsetXY = vec2(0.0);
  float offsetZ = 0.0;
  int anim = int(iFlags.y + 0.5);
  float ph = iFlags.z;

  if (anim == 1) {
    // sway：樹冠／植物隨風搖曳（高度二次方加權）
    float w = zFrac * zFrac * 0.08 * horizScale;
    offsetXY.x += sin(uTime * 2.1 + ph) * w;
    offsetXY.y += cos(uTime * 1.7 + ph * 1.3) * w * 0.6;
  } else if (anim == 2) {
    // walk：人物／動物行進間彈性微跳與輕微擺動
    float stepWave = abs(sin(uTime * 7.5 + ph));
    offsetZ += stepWave * 0.06 * heightScale;
    yaw += sin(uTime * 7.5 + ph) * 0.08 * zFrac;
  } else if (anim == 3) {
    // float：熱氣球／飛船／水晶優雅漂浮
    offsetZ += sin(uTime * 1.8 + ph) * 0.16 * heightScale;
  } else if (anim == 4) {
    // bob：車船輕微震動／水面搖晃
    offsetZ += sin(uTime * 3.4 + ph) * 0.03 * heightScale;
    offsetXY.x += sin(uTime * 2.2 + ph) * 0.02 * horizScale * zFrac;
  } else if (anim == 5) {
    yaw += uTime * 1.1;
  }

  float c = cos(yaw);
  float s = sin(yaw);
  vec2 localScaled = vec2(aCorner.x * worldW, aCorner.y * worldH);
  vec2 rotated = vec2(
    localScaled.x * c - localScaled.y * s,
    localScaled.x * s + localScaled.y * c
  );

  vec3 worldPos = vec3(
    iPos.x + rotated.x + offsetXY.x,
    iPos.y + rotated.y + offsetXY.y,
    iPos.z + worldZ + offsetZ
  );

  vLocalUv = aCorner + 0.5;
  vLayer = aLayer;
  vYaw = yaw;
  vTint = iTint;
  vEmissiveOn = iFlags.x;
  vHeightFrac = zFrac;
  gl_Position = uViewProj * vec4(worldPos, 1.0);
}`;

const STACK_FS = `#version 300 es
precision highp float;

in vec2 vLocalUv;
in float vLayer;
in float vYaw;
in vec3 vTint;
in float vEmissiveOn;
in float vHeightFrac;

uniform sampler2D uSlicesTex;
uniform sampler2D uNormalsTex;
uniform sampler2D uEmissiveTex;
uniform float uSliceCount;
uniform vec3 uSunDir;
uniform vec3 uSunColor;
uniform vec3 uAmbient;
uniform float uNightFactor;
uniform float uSnowFactor;

out vec4 outColor;

void main() {
  float localU = clamp(vLocalUv.x, 0.002, 0.998);
  float localV = clamp(vLocalUv.y, 0.002, 0.998);
  vec2 uv = vec2((vLayer + localU) / uSliceCount, localV);

  vec4 albedo = texture(uSlicesTex, uv);
  if (albedo.a < 0.5) discard;

  vec4 normTex = texture(uNormalsTex, uv);
  vec3 nLocal = normTex.a > 0.1 ? (normTex.xyz * 2.0 - 1.0) : vec3(0.0, 0.0, 1.0);

  // 將物體局部法線旋轉回世界空間
  float c = cos(vYaw);
  float s = sin(vYaw);
  vec3 nWorld = normalize(vec3(
    nLocal.x * c - nLocal.y * s,
    nLocal.x * s + nLocal.y * c,
    nLocal.z
  ));

  float ndotl = dot(nWorld, normalize(uSunDir));
  float band = ndotl > 0.34 ? 1.12 : (ndotl > -0.15 ? 0.95 : 0.72);
  float smoothL = 0.85 + 0.25 * max(ndotl, -0.4);
  float shade = mix(band, smoothL, 0.35);

  vec3 baseCol = albedo.rgb * vTint;
  // 下雪時於朝上表面覆蓋柔白積雪
  if (uSnowFactor > 0.05 && nWorld.z > 0.55) {
    baseCol = mix(baseCol, vec3(0.93, 0.97, 1.0), uSnowFactor * 0.78);
  }

  vec3 lit = baseCol * (uAmbient + uSunColor * shade * 0.68);

  // 夜間或手動點亮自發光遮罩（燈火、窗戶、眼睛、魔法水晶）
  vec4 emis = texture(uEmissiveTex, uv);
  if (emis.a > 0.3 && vEmissiveOn > 0.5) {
    float glowPow = mix(0.45, 1.45, uNightFactor);
    lit = max(lit, emis.rgb * glowPow) + emis.rgb * 0.28 * uNightFactor;
  }

  outColor = vec4(min(lit, vec3(1.2)), 1.0);
}`;

const SHADOW_VS = `#version 300 es
precision highp float;
layout(location=0) in vec2 aCorner;
layout(location=1) in vec3 iPos;
layout(location=2) in float iRadius;
uniform mat4 uViewProj;
uniform vec2 uShadowShift;
out vec2 vUv;
void main() {
  vec2 p = iPos.xy + uShadowShift + aCorner * iRadius * 2.0;
  vUv = aCorner * 2.0;
  gl_Position = uViewProj * vec4(p, iPos.z + 0.015, 1.0);
}`;

const SHADOW_FS = `#version 300 es
precision highp float;
in vec2 vUv;
uniform float uNightFactor;
out vec4 outColor;
void main() {
  float d = dot(vUv, vUv);
  if (d > 1.0) discard;
  float alpha = (1.0 - d) * mix(0.32, 0.12, uNightFactor);
  outColor = vec4(0.05, 0.08, 0.16, alpha);
}`;

export class StackCityRenderer {
  constructor(glCanvas, overlayCanvas) {
    this.canvas = glCanvas;
    this.overlay = overlayCanvas;
    this.octx = overlayCanvas.getContext("2d");

    const gl = glCanvas.getContext("webgl2", { antialias: false, alpha: false });
    if (!gl) {
      throw new Error("您的瀏覽器不支援 WebGL2，請改用新版 Chrome / Edge / Firefox。");
    }
    this.gl = gl;

    this.terrainProg = createProgram(gl, TERRAIN_VS, TERRAIN_FS);
    this.waterProg = createProgram(gl, WATER_VS, WATER_FS);
    this.stackProg = createProgram(gl, STACK_VS, STACK_FS);
    this.shadowProg = createProgram(gl, SHADOW_VS, SHADOW_FS);

    this._initStackGeometry();
    this._initShadowGeometry();

    this.terrainVao = gl.createVertexArray();
    this.terrainVbo = gl.createBuffer();
    this.terrainVertCount = 0;

    this.waterVao = gl.createVertexArray();
    this.waterVbo = gl.createBuffer();
    this.waterVertCount = 0;

    this.assetTextures = new Map(); // assetId -> { slicesTex, normalsTex, emissiveTex, thumbCanvas, meta }
    this.worldWidth = 40;
    this.worldHeight = 40;
    this.terrainGrid = []; // [y * W + x] -> tile

    // 相機參數
    this.camera = {
      target: [20, 20, 0.8],
      yaw: 0.65,
      pitch: 0.68,
      dist: 24.0,
      walkMode: false,
      walkPos: [20, 22, 0.6],
      walkVelZ: 0,
      walkYaw: 0,
      followEntityId: null,
    };

    // 特效粒子與氣泡
    this.particles = [];
    this.bubbles = []; // { entityId, x, y, z, text, expiresAt }
    this.hoverTile = null;
    this.hoverWorldPos = null;
    this.hoverEntity = null;
    this.brushPreview = { active: false, radius: 3.0 };
    this.selectedEntityId = null;
    this.draftPathWaypoints = [];
    this.lastViewProj = new Float32Array(16);
    this.lastAspect = 1.0;
  }

  getMaxZoomDist() {
    return Math.max(65, Math.max(this.worldWidth, this.worldHeight) * 1.65);
  }

  _initStackGeometry() {
    const gl = this.gl;
    // 每層 3 個平行微錯層 quad，共 MAX_SLICES * SUB_LAYERS * 6 頂點，每頂點 4 floats: (cx, cy, layer, subOffset)
    const verts = [];
    const subOffsets = [0.0, 0.34, 0.68];
    const quad = [
      [-0.5, -0.5], [0.5, -0.5], [0.5, 0.5],
      [-0.5, -0.5], [0.5, 0.5], [-0.5, 0.5],
    ];
    for (let z = 0; z < MAX_SLICES; z++) {
      for (const sub of subOffsets) {
        for (const [cx, cy] of quad) {
          verts.push(cx, cy, z, sub);
        }
      }
    }
    this.stackVertCount = MAX_SLICES * SUB_LAYERS * 6;
    this.stackStaticVbo = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, this.stackStaticVbo);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(verts), gl.STATIC_DRAW);

    this.stackInstanceVbo = gl.createBuffer();
    this.stackVao = gl.createVertexArray();
    gl.bindVertexArray(this.stackVao);

    gl.bindBuffer(gl.ARRAY_BUFFER, this.stackStaticVbo);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 16, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 1, gl.FLOAT, false, 16, 8);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 1, gl.FLOAT, false, 16, 12);

    // 每個實例 14 floats (56 bytes): pos(3), yaw(1), scaleXYZ(3), tint(3), flags(4)
    gl.bindBuffer(gl.ARRAY_BUFFER, this.stackInstanceVbo);
    const stride = 56;
    const setupInst = (loc, size, offset) => {
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc, size, gl.FLOAT, false, stride, offset);
      gl.vertexAttribDivisor(loc, 1);
    };
    setupInst(3, 3, 0);
    setupInst(4, 1, 12);
    setupInst(5, 3, 16);
    setupInst(6, 3, 28);
    setupInst(7, 4, 40);
    gl.bindVertexArray(null);
  }

  _initShadowGeometry() {
    const gl = this.gl;
    const quad = new Float32Array([
      -0.5, -0.5, 0.5, -0.5, 0.5, 0.5,
      -0.5, -0.5, 0.5, 0.5, -0.5, 0.5,
    ]);
    this.shadowStaticVbo = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, this.shadowStaticVbo);
    gl.bufferData(gl.ARRAY_BUFFER, quad, gl.STATIC_DRAW);

    this.shadowInstVbo = gl.createBuffer();
    this.shadowVao = gl.createVertexArray();
    gl.bindVertexArray(this.shadowVao);

    gl.bindBuffer(gl.ARRAY_BUFFER, this.shadowStaticVbo);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 8, 0);

    gl.bindBuffer(gl.ARRAY_BUFFER, this.shadowInstVbo);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 3, gl.FLOAT, false, 16, 0);
    gl.vertexAttribDivisor(1, 1);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 1, gl.FLOAT, false, 16, 12);
    gl.vertexAttribDivisor(2, 1);
    gl.bindVertexArray(null);
  }

  resize() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const rect = this.canvas.getBoundingClientRect();
    const w = Math.max(1, Math.floor(rect.width * dpr));
    const h = Math.max(1, Math.floor(rect.height * dpr));
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
      this.overlay.width = w;
      this.overlay.height = h;
    }
    this.dpr = dpr;
  }

  resetCamera() {
    this.camera.walkMode = false;
    this.camera.followEntityId = null;
    this.camera.target = [this.worldWidth * 0.5, this.worldHeight * 0.5, 0.8];
    this.camera.yaw = 0.65;
    this.camera.pitch = 0.68;
    this.camera.dist = Math.max(this.worldWidth, this.worldHeight) * 0.62;
  }

  getTerrainElevationAt(wx, wy) {
    const gx = Math.floor(wx);
    const gy = Math.floor(wy);
    if (gx < 0 || gy < 0 || gx >= this.worldWidth || gy >= this.worldHeight) {
      return 0.0;
    }
    const tile = this.terrainGrid[gy * this.worldWidth + gx];
    if (!tile) return ELEV_UNIT;
    if (tile.ground_type === "water") {
      return tile.road_type === "bridge" ? ELEV_UNIT + 0.06 : 0.24;
    }
    return tile.elevation * ELEV_UNIT;
  }

  /// 根據地形陣列重建階梯體素地貌與智慧拼接道路網格
  rebuildTerrainMesh(width, height, tiles) {
    this.worldWidth = width;
    this.worldHeight = height;
    this.terrainGrid = new Array(width * height);
    for (const t of tiles) {
      if (t.x >= 0 && t.y >= 0 && t.x < width && t.y < height) {
        this.terrainGrid[t.y * width + t.x] = t;
      }
    }

    const groundColors = {
      grass: [0.34, 0.64, 0.30],
      sand: [0.82, 0.72, 0.48],
      stone: [0.55, 0.58, 0.63],
      brick: [0.74, 0.34, 0.28],
      dirt: [0.56, 0.42, 0.28],
      snow: [0.89, 0.94, 0.99],
      water: [0.16, 0.48, 0.72],
      // 室內地板與立體牆體
      wood_floor: [0.68, 0.48, 0.30],
      tile_floor: [0.85, 0.87, 0.90],
      carpet_red: [0.72, 0.16, 0.20],
      carpet_blue: [0.22, 0.36, 0.56],
      concrete: [0.46, 0.49, 0.54],
      wall_stone: [0.42, 0.44, 0.49],
      wall_cream: [0.88, 0.83, 0.74],
      wall_glass: [0.46, 0.68, 0.82],
    };

    const roadColors = {
      asphalt: [0.22, 0.25, 0.30],
      stone: [0.64, 0.66, 0.70],
      dirt: [0.63, 0.49, 0.32],
      bridge: [0.56, 0.38, 0.22],
    };

    const verts = []; // 12 floats per vertex: pos(3), norm(3), col(3), uv(2), isRoad(1)
    const waterVerts = []; // 3 floats per vertex

    const pushTri = (arr, a, b, c, nx, ny, nz, col, isRoad) => {
      arr.push(
        a[0], a[1], a[2], nx, ny, nz, col[0], col[1], col[2], 0, 0, isRoad,
        b[0], b[1], b[2], nx, ny, nz, col[0], col[1], col[2], 1, 0, isRoad,
        c[0], c[1], c[2], nx, ny, nz, col[0], col[1], col[2], 1, 1, isRoad,
      );
    };

    const pushQuad = (arr, p0, p1, p2, p3, nx, ny, nz, col, isRoad) => {
      pushTri(arr, p0, p1, p2, nx, ny, nz, col, isRoad);
      pushTri(arr, p0, p2, p3, nx, ny, nz, col, isRoad);
    };

    const tileAt = (x, y) =>
      x >= 0 && y >= 0 && x < width && y < height ? this.terrainGrid[y * width + x] : null;

    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const t = this.terrainGrid[y * width + x];
        if (!t) continue;
        const isWater = t.ground_type === "water";
        const topZ = isWater ? -0.18 : t.elevation * ELEV_UNIT;
        const baseZ = -0.6;

        // 微小棋盤色差讓地塊層次更生動
        const checker = ((x + y) & 1) === 0 ? 1.0 : 0.96;
        let topCol = groundColors[t.ground_type] || groundColors.grass;
        let isRoadFlag = 0.0;
        if (t.road_type && t.road_type !== "none") {
          topCol = roadColors[t.road_type] || roadColors.stone;
          isRoadFlag = 1.0;
        }
        const topC = [topCol[0] * checker, topCol[1] * checker, topCol[2] * checker];
        const sideC = isWater
          ? [0.22, 0.30, 0.42]
          : [topCol[0] * 0.68, topCol[1] * 0.64, topCol[2] * 0.60];

        if (isWater) {
          // 河床底板
          pushQuad(
            verts,
            [x, y, topZ], [x + 1, y, topZ], [x + 1, y + 1, topZ], [x, y + 1, topZ],
            0, 0, 1, [0.15, 0.32, 0.48], 0.0,
          );
          // 半透明動態水面
          const wz = 0.24;
          waterVerts.push(
            x, y, wz, x + 1, y, wz, x + 1, y + 1, wz,
            x, y, wz, x + 1, y + 1, wz, x, y + 1, wz,
          );
          // 橋樑直接跨過水面上方並補齊四周厚度側面
          if (t.road_type === "bridge") {
            const bz = ELEV_UNIT + 0.06;
            const bThick = 0.16;
            const bCol = roadColors.bridge;
            const bSide = [0.42, 0.28, 0.16];
            pushQuad(
              verts,
              [x, y, bz], [x + 1, y, bz], [x + 1, y + 1, bz], [x, y + 1, bz],
              0, 0, 1, bCol, 1.0,
            );
            const isBridgeAt = (gx, gy) => {
              const nb = tileAt(gx, gy);
              return nb && nb.road_type === "bridge";
            };
            if (!isBridgeAt(x, y + 1)) {
              pushQuad(verts, [x, y + 1, bz - bThick], [x + 1, y + 1, bz - bThick], [x + 1, y + 1, bz], [x, y + 1, bz], 0, 1, 0, bSide, 1.0);
            }
            if (!isBridgeAt(x, y - 1)) {
              pushQuad(verts, [x + 1, y, bz - bThick], [x, y, bz - bThick], [x, y, bz], [x + 1, y, bz], 0, -1, 0, bSide, 1.0);
            }
            if (!isBridgeAt(x - 1, y)) {
              pushQuad(verts, [x, y, bz - bThick], [x, y + 1, bz - bThick], [x, y + 1, bz], [x, y, bz], -1, 0, 0, bSide, 1.0);
            }
            if (!isBridgeAt(x + 1, y)) {
              pushQuad(verts, [x + 1, y + 1, bz - bThick], [x + 1, y, bz - bThick], [x + 1, y, bz], [x + 1, y + 1, bz], 1, 0, 0, bSide, 1.0);
            }
          }
          continue;
        }

        // 頂面
        pushQuad(
          verts,
          [x, y, topZ], [x + 1, y, topZ], [x + 1, y + 1, topZ], [x, y + 1, topZ],
          0, 0, 1, topC, isRoadFlag,
        );

        // 四周露出的崖面（僅當鄰居低於本格時繪製，大幅減少三角面）
        const checkSide = (nx, ny, p0, p1, dx, dy) => {
          const nb = tileAt(nx, ny);
          const nbZ = !nb ? baseZ : nb.ground_type === "water" ? -0.18 : nb.elevation * ELEV_UNIT;
          if (topZ > nbZ + 0.01) {
            const low = nb ? nbZ : baseZ;
            pushQuad(
              verts,
              [p0[0], p0[1], low],
              [p1[0], p1[1], low],
              [p1[0], p1[1], topZ],
              [p0[0], p0[1], topZ],
              dx, dy, 0, sideC, 0.0,
            );
          }
        };
        checkSide(x, y + 1, [x, y + 1], [x + 1, y + 1], 0, 1);
        checkSide(x, y - 1, [x + 1, y], [x, y], 0, -1);
        checkSide(x + 1, y, [x + 1, y + 1], [x + 1, y], 1, 0);
        checkSide(x - 1, y, [x, y], [x, y + 1], -1, 0);

        // 若是道路，在頂面繪製精細中線標線（柏油白虛線 / 斑馬線）
        if (t.road_type === "asphalt") {
          const nUp = tileAt(x, y - 1)?.road_type === "asphalt";
          const nDown = tileAt(x, y + 1)?.road_type === "asphalt";
          const nLeft = tileAt(x - 1, y)?.road_type === "asphalt";
          const nRight = tileAt(x + 1, y)?.road_type === "asphalt";
          const rz = topZ + 0.012;
          const lineC = [0.92, 0.85, 0.45];
          if ((nUp || nDown) && !nLeft && !nRight) {
            pushQuad(
              verts,
              [x + 0.46, y + 0.2, rz], [x + 0.54, y + 0.2, rz],
              [x + 0.54, y + 0.8, rz], [x + 0.46, y + 0.8, rz],
              0, 0, 1, lineC, 1.0,
            );
          } else if ((nLeft || nRight) && !nUp && !nDown) {
            pushQuad(
              verts,
              [x + 0.2, y + 0.46, rz], [x + 0.8, y + 0.46, rz],
              [x + 0.8, y + 0.54, rz], [x + 0.2, y + 0.54, rz],
              0, 0, 1, lineC, 1.0,
            );
          }
        }
      }
    }

    const gl = this.gl;
    gl.bindVertexArray(this.terrainVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.terrainVbo);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(verts), gl.STATIC_DRAW);
    const stride = 12 * 4;
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, stride, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 3, gl.FLOAT, false, stride, 12);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 3, gl.FLOAT, false, stride, 24);
    gl.enableVertexAttribArray(3);
    gl.vertexAttribPointer(3, 2, gl.FLOAT, false, stride, 36);
    gl.enableVertexAttribArray(4);
    gl.vertexAttribPointer(4, 1, gl.FLOAT, false, stride, 44);
    this.terrainVertCount = verts.length / 12;

    gl.bindVertexArray(this.waterVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.waterVbo);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(waterVerts), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 12, 0);
    this.waterVertCount = waterVerts.length / 3;
    gl.bindVertexArray(null);
  }

  async ensureAssetTextures(worldFile, assetMeta) {
    if (this.assetTextures.has(assetMeta.id)) {
      const cached = this.assetTextures.get(assetMeta.id);
      if (cached.sha256 === assetMeta.sha256) return cached;
    }
    const standalone = window.__STACKCITY_STANDALONE__;
    const embedded =
      standalone?.texturePool?.[assetMeta.sha256] ||
      standalone?.textures?.[assetMeta.id];
    const base = `/api/worlds/${encodeURIComponent(worldFile)}/assets/${encodeURIComponent(assetMeta.id)}`;
    const loadImg = (url, isDataUrl = false) =>
      new Promise((resolve, reject) => {
        const img = new Image();
        if (!isDataUrl) img.crossOrigin = "anonymous";
        img.onload = () => resolve(img);
        img.onerror = () => reject(new Error(`載入貼圖失敗：${isDataUrl ? assetMeta.id : url}`));
        // Data URL 不可附加 ?v=... 查詢參數，否則會損壞 base64 圖片解碼
        img.src = isDataUrl ? url : `${url}?v=${assetMeta.sha256.slice(0, 8)}`;
      });

    const [slicesImg, normalsImg, emissiveImg] = embedded
      ? await Promise.all([
          loadImg(embedded.slices, true),
          loadImg(embedded.normals, true),
          loadImg(embedded.emissive, true),
        ])
      : await Promise.all([
          loadImg(`${base}/slices.png`),
          loadImg(`${base}/normals.png`),
          loadImg(`${base}/emissive.png`),
        ]);

    const makeTex = (img) => {
      const gl = this.gl;
      const tex = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
      return tex;
    };

    const entry = {
      sha256: assetMeta.sha256,
      meta: assetMeta,
      slicesImg,
      normalsImg,
      emissiveImg,
      slicesTex: makeTex(slicesImg),
      normalsTex: makeTex(normalsImg),
      emissiveTex: makeTex(emissiveImg),
      thumbCanvas: renderStackThumbnail(slicesImg, assetMeta.slice_count, 68),
    };
    this.assetTextures.set(assetMeta.id, entry);
    return entry;
  }

  /// 計算目前日夜時間對應的光影與天空色彩
  computeEnvironment(timeOfDay, weather) {
    const t = ((timeOfDay % 24) + 24) % 24;
    // 日出 ~6:00，正午 12:00，日落 ~18:30
    const dayAngle = ((t - 6.0) / 12.5) * Math.PI;
    const sunElev = Math.sin(dayAngle);
    const sunAz = (t / 24.0) * Math.PI * 2.0 - 0.8;
    const sunDir = [
      Math.cos(sunAz) * 0.65,
      Math.sin(sunAz) * 0.65,
      Math.max(0.35, Math.abs(sunElev) * 0.9 + 0.25),
    ];

    let nightFactor = 0.0;
    if (t < 5.5 || t > 19.5) nightFactor = 1.0;
    else if (t < 7.0) nightFactor = 1.0 - (t - 5.5) / 1.5;
    else if (t > 17.5) nightFactor = (t - 17.5) / 2.0;

    const sunsetFactor = Math.max(
      0,
      1.0 - Math.min(Math.abs(t - 6.2), Math.abs(t - 18.2)) / 1.6,
    );

    let sky = [0.56, 0.78, 0.96];
    let sunCol = [1.0, 0.98, 0.90];
    let ambient = [0.46, 0.50, 0.58];

    if (sunsetFactor > 0.05) {
      sky = [
        sky[0] * (1 - sunsetFactor) + 0.86 * sunsetFactor,
        sky[1] * (1 - sunsetFactor) + 0.46 * sunsetFactor,
        sky[2] * (1 - sunsetFactor) + 0.42 * sunsetFactor,
      ];
      sunCol = [1.0, 0.66, 0.36];
    }
    if (nightFactor > 0.01) {
      sky = [
        sky[0] * (1 - nightFactor) + 0.05 * nightFactor,
        sky[1] * (1 - nightFactor) + 0.08 * nightFactor,
        sky[2] * (1 - nightFactor) + 0.16 * nightFactor,
      ];
      sunCol = [
        sunCol[0] * (1 - nightFactor) + 0.38 * nightFactor,
        sunCol[1] * (1 - nightFactor) + 0.48 * nightFactor,
        sunCol[2] * (1 - nightFactor) + 0.78 * nightFactor,
      ];
      ambient = [
        ambient[0] * (1 - nightFactor) + 0.16 * nightFactor,
        ambient[1] * (1 - nightFactor) + 0.20 * nightFactor,
        ambient[2] * (1 - nightFactor) + 0.34 * nightFactor,
      ];
    }
    if (weather === "rain") {
      sky = [sky[0] * 0.68 + 0.14, sky[1] * 0.72 + 0.16, sky[2] * 0.78 + 0.20];
      sunCol = [sunCol[0] * 0.75, sunCol[1] * 0.78, sunCol[2] * 0.85];
    } else if (weather === "snow") {
      sky = [sky[0] * 0.72 + 0.22, sky[1] * 0.75 + 0.23, sky[2] * 0.78 + 0.24];
    }

    return {
      sunDir,
      sunCol,
      ambient,
      sky,
      nightFactor,
      snowFactor: weather === "snow" ? 1.0 : 0.0,
      rainFactor: weather === "rain" ? 1.0 : 0.0,
    };
  }

  getCameraMatrices(simEntities, player = null) {
    const cam = this.camera;
    let target = cam.target;
    let pitch = cam.pitch;
    let yaw = cam.yaw;
    let dist = cam.dist;
    let fov = (36 * Math.PI) / 180;

    if (cam.walkMode) {
      if (cam.followEntityId && simEntities) {
        const ent = simEntities.find((e) => e.id === cam.followEntityId);
        if (ent) {
          const gz = this.getTerrainElevationAt(ent.x, ent.y);
          target = [ent.x, ent.y, gz + ent.z + 0.65];
          yaw = -((ent.yaw * Math.PI) / 180) + Math.PI;
        }
      } else {
        if (player && player.visible) {
          const gz = this.getTerrainElevationAt(player.x, player.y);
          const isMounted = Boolean(player.mountedEntityId);
          target = [player.x, player.y, gz + (player.z || 0) + (isMounted ? 1.15 : 0.95)];
        } else {
          target = [cam.walkPos[0], cam.walkPos[1], cam.walkPos[2] + 0.95];
        }
        yaw = cam.walkYaw;
      }
      const isMounted = Boolean(player && player.mountedEntityId && !cam.followEntityId);
      pitch = isMounted ? 0.40 : 0.36;
      dist = isMounted ? 6.6 : 5.2;
      fov = (50 * Math.PI) / 180;
    }

    const eye = [
      target[0] + dist * Math.cos(pitch) * Math.sin(yaw),
      target[1] + dist * Math.cos(pitch) * Math.cos(yaw),
      target[2] + dist * Math.sin(pitch),
    ];
    const aspect = this.canvas.width / Math.max(this.canvas.height, 1);
    const proj = mat4Perspective(fov, aspect, 0.2, 260);
    const view = mat4LookAt(eye, target, [0, 0, 1]);
    const viewProj = mat4Mul(proj, view);
    this.lastViewProj = viewProj;
    this.lastAspect = aspect;
    return { viewProj, eye, target };
  }

  /// 將螢幕滑鼠位置轉為 3D 世界射線並精準拾取地形格點或物件實體
  pickScene(clientX, clientY, simEntities, assetMap, skipPlayer = false) {
    const rect = this.canvas.getBoundingClientRect();
    const nx = ((clientX - rect.left) / Math.max(rect.width, 1)) * 2 - 1;
    const ny = 1 - ((clientY - rect.top) / Math.max(rect.height, 1)) * 2;
    const inv = mat4Invert(this.lastViewProj);
    if (!inv) return { entity: null, tile: null, worldPos: null };

    const unproj = (x, y, z) => {
      const w = inv[3] * x + inv[7] * y + inv[11] * z + inv[15];
      return [
        (inv[0] * x + inv[4] * y + inv[8] * z + inv[12]) / w,
        (inv[1] * x + inv[5] * y + inv[9] * z + inv[13]) / w,
        (inv[2] * x + inv[6] * y + inv[10] * z + inv[14]) / w,
      ];
    };
    const pNear = unproj(nx, ny, -1);
    const pFar = unproj(nx, ny, 1);
    const dx = pFar[0] - pNear[0];
    const dy = pFar[1] - pNear[1];
    const dz = pFar[2] - pNear[2];
    const len = Math.hypot(dx, dy, dz) || 1;
    const rd = [dx / len, dy / len, dz / len];

    // 1. 使用精準 3D Ray-AABB Slab 相交檢測場景實體（支援 XYZ 三軸獨立縮放與旋轉包圍盒）
    let bestEntity = null;
    let bestDist = Infinity;
    for (const ent of simEntities) {
      if (!ent.visible) continue;
      if (skipPlayer && ent.id === "__player__") continue;
      const meta = assetMap.get(ent.asset_id);
      const sx = ent.scale_x > 0.01 ? ent.scale_x : (ent.scale || 1.0);
      const sy = ent.scale_y > 0.01 ? ent.scale_y : (ent.scale || 1.0);
      const sz = ent.scale_z > 0.01 ? ent.scale_z : (ent.scale || 1.0);
      const sw = (meta?.slice_w || 32) * PX_TO_WORLD * sx;
      const sl = (meta?.slice_h || 32) * PX_TO_WORLD * sy;
      const sh = (meta?.slice_count || 36) * PX_TO_WORLD * sz;
      const yawRad = ((ent.yaw || 0) * Math.PI) / 180;
      const absC = Math.abs(Math.cos(yawRad));
      const absS = Math.abs(Math.sin(yawRad));
      const hw = Math.max(0.45, (sw * absC + sl * absS) * 0.56);
      const hl = Math.max(0.45, (sw * absS + sl * absC) * 0.56);
      const baseZ = this.getTerrainElevationAt(ent.x, ent.y) + ent.z;
      const topZ = baseZ + Math.max(0.85, sh * 1.05);

      const minB = [ent.x - hw, ent.y - hl, baseZ - 0.1];
      const maxB = [ent.x + hw, ent.y + hl, topZ];

      let tMin = 0.0;
      let tMax = bestDist;
      let hit = true;
      for (let axis = 0; axis < 3; axis++) {
        const d = rd[axis];
        if (Math.abs(d) < 1e-6) {
          if (pNear[axis] < minB[axis] || pNear[axis] > maxB[axis]) {
            hit = false;
            break;
          }
        } else {
          let t1 = (minB[axis] - pNear[axis]) / d;
          let t2 = (maxB[axis] - pNear[axis]) / d;
          if (t1 > t2) { const tmp = t1; t1 = t2; t2 = tmp; }
          tMin = Math.max(tMin, t1);
          tMax = Math.min(tMax, t2);
          if (tMin > tMax) {
            hit = false;
            break;
          }
        }
      }
      if (hit && tMin < bestDist) {
        bestDist = tMin;
        bestEntity = ent;
      }
    }

    // 2. 步進尋找與地形網格之交點
    let hitPos = null;
    let hitTile = null;
    const maxT = 160;
    const step = 0.22;
    let prevT = 0;
    for (let t = 0.5; t < maxT; t += step) {
      const wx = pNear[0] + rd[0] * t;
      const wy = pNear[1] + rd[1] * t;
      const wz = pNear[2] + rd[2] * t;
      if (wx >= 0 && wy >= 0 && wx < this.worldWidth && wy < this.worldHeight) {
        const gz = this.getTerrainElevationAt(wx, wy);
        if (wz <= gz) {
          // 二分收斂精確交點
          let lo = prevT;
          let hi = t;
          for (let k = 0; k < 5; k++) {
            const mid = (lo + hi) * 0.5;
            const mx = pNear[0] + rd[0] * mid;
            const my = pNear[1] + rd[1] * mid;
            const mz = pNear[2] + rd[2] * mid;
            if (mz <= this.getTerrainElevationAt(mx, my)) hi = mid;
            else lo = mid;
          }
          const fx = pNear[0] + rd[0] * hi;
          const fy = pNear[1] + rd[1] * hi;
          const fz = this.getTerrainElevationAt(fx, fy);
          hitPos = [fx, fy, fz];
          hitTile = { x: Math.floor(fx), y: Math.floor(fy) };
          break;
        }
      }
      prevT = t;
    }

    return { entity: bestEntity, tile: hitTile, worldPos: hitPos };
  }

  projectToScreen(wx, wy, wz) {
    const m = this.lastViewProj;
    const cx = m[0] * wx + m[4] * wy + m[8] * wz + m[12];
    const cy = m[1] * wx + m[5] * wy + m[9] * wz + m[13];
    const cz = m[2] * wx + m[6] * wy + m[10] * wz + m[14];
    const cw = m[3] * wx + m[7] * wy + m[11] * wz + m[15];
    if (cw <= 0.01) return null;
    const ndcX = cx / cw;
    const ndcY = cy / cw;
    const ndcZ = cz / cw;
    if (ndcZ < -1 || ndcZ > 1.05) return null;
    return [
      (ndcX * 0.5 + 0.5) * this.canvas.width,
      (0.5 - ndcY * 0.5) * this.canvas.height,
    ];
  }

  spawnParticles(effect, x, y, z, count = 28) {
    const palettes = {
      firework: ["#fde047", "#f43f5e", "#38bdf8", "#a855f7", "#4ade80"],
      sparkle: ["#fde047", "#67e8f9", "#ffffff"],
      sakura: ["#f9a8d4", "#fbcfe8", "#fdf2f8"],
      smoke: ["#cbd5e1", "#94a3b8"],
    };
    const cols = palettes[effect] || palettes.sparkle;
    for (let i = 0; i < count; i++) {
      const ang = Math.random() * Math.PI * 2;
      const spd = effect === "sakura" ? 0.5 + Math.random() * 0.8 : 1.2 + Math.random() * 3.4;
      const vz = effect === "sakura" ? -0.4 - Math.random() * 0.5 : (effect === "smoke" ? 1.0 : 1.5 + Math.random() * 3.8);
      this.particles.push({
        effect,
        x,
        y,
        z,
        vx: Math.cos(ang) * spd,
        vy: Math.sin(ang) * spd,
        vz,
        life: 0,
        maxLife: effect === "sakura" ? 3.5 : 1.1 + Math.random() * 0.9,
        color: cols[i % cols.length],
        size: effect === "firework" ? 5 : 4,
      });
    }
  }

  addSpeechBubble(entityId, x, y, z, text, durationSec = 4.0) {
    this.bubbles = this.bubbles.filter((b) => b.entityId !== entityId);
    this.bubbles.push({
      entityId,
      x,
      y,
      z,
      text,
      expiresAt: performance.now() + durationSec * 1000,
    });
  }

  updateAndRender(dt, simState) {
    this.resize();
    const gl = this.gl;
    const nowSec = performance.now() * 0.001;
    const env = this.computeEnvironment(simState.timeOfDay, simState.weather);
    const { viewProj } = this.getCameraMatrices(simState.entities, simState.player);

    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.clearColor(env.sky[0], env.sky[1], env.sky[2], 1.0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    gl.disable(gl.BLEND);

    // 1. 繪製階梯地形與智慧路網
    if (this.terrainVertCount > 0) {
      gl.useProgram(this.terrainProg);
      gl.uniformMatrix4fv(gl.getUniformLocation(this.terrainProg, "uViewProj"), false, viewProj);
      gl.uniform3fv(gl.getUniformLocation(this.terrainProg, "uSunDir"), env.sunDir);
      gl.uniform3fv(gl.getUniformLocation(this.terrainProg, "uSunColor"), env.sunCol);
      gl.uniform3fv(gl.getUniformLocation(this.terrainProg, "uAmbient"), env.ambient);
      gl.uniform1f(gl.getUniformLocation(this.terrainProg, "uSnowFactor"), env.snowFactor);
      gl.uniform1f(gl.getUniformLocation(this.terrainProg, "uRainFactor"), env.rainFactor);
      gl.uniform1f(gl.getUniformLocation(this.terrainProg, "uNightFactor"), env.nightFactor);
      gl.bindVertexArray(this.terrainVao);
      gl.drawArrays(gl.TRIANGLES, 0, this.terrainVertCount);
    }

    // 2. 合併世界物件與玩家角色化身，依資產分組以 GPU Instancing 繪製
    const renderList = simState.entities.slice();
    if (
      simState.player &&
      simState.player.visible &&
      simState.player.asset_id &&
      !simState.player.playerHiddenInVehicle
    ) {
      renderList.push(simState.player);
    }

    const groups = new Map();
    const shadows = [];
    for (const ent of renderList) {
      if (!ent.visible) continue;
      const texEntry = this.assetTextures.get(ent.asset_id);
      if (!texEntry) continue;
      if (!groups.has(ent.asset_id)) groups.set(ent.asset_id, []);
      groups.get(ent.asset_id).push(ent);

      const baseZ = this.getTerrainElevationAt(ent.x, ent.y);
      const sx = ent.scale_x > 0.01 ? ent.scale_x : (ent.scale || 1.0);
      const sy = ent.scale_y > 0.01 ? ent.scale_y : (ent.scale || 1.0);
      const sw = texEntry.meta.slice_w * PX_TO_WORLD * Math.max(sx, sy);
      shadows.push(ent.x, ent.y, baseZ, Math.max(0.28, sw * 0.45));
    }

    // 2A. 繪製地面投影陰影
    if (shadows.length > 0) {
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.depthMask(false);
      gl.useProgram(this.shadowProg);
      gl.uniformMatrix4fv(gl.getUniformLocation(this.shadowProg, "uViewProj"), false, viewProj);
      gl.uniform2f(
        gl.getUniformLocation(this.shadowProg, "uShadowShift"),
        -env.sunDir[0] * 0.25,
        -env.sunDir[1] * 0.25,
      );
      gl.uniform1f(gl.getUniformLocation(this.shadowProg, "uNightFactor"), env.nightFactor);
      gl.bindVertexArray(this.shadowVao);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.shadowInstVbo);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(shadows), gl.DYNAMIC_DRAW);
      gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, shadows.length / 4);
      gl.depthMask(true);
      gl.disable(gl.BLEND);
    }

    // 2B. 繪製精靈堆疊體素實例（支援 XYZ 獨立縮放）
    gl.useProgram(this.stackProg);
    gl.uniformMatrix4fv(gl.getUniformLocation(this.stackProg, "uViewProj"), false, viewProj);
    gl.uniform3fv(gl.getUniformLocation(this.stackProg, "uSunDir"), env.sunDir);
    gl.uniform3fv(gl.getUniformLocation(this.stackProg, "uSunColor"), env.sunCol);
    gl.uniform3fv(gl.getUniformLocation(this.stackProg, "uAmbient"), env.ambient);
    gl.uniform1f(gl.getUniformLocation(this.stackProg, "uNightFactor"), env.nightFactor);
    gl.uniform1f(gl.getUniformLocation(this.stackProg, "uSnowFactor"), env.snowFactor);
    gl.uniform1f(gl.getUniformLocation(this.stackProg, "uTime"), nowSec);
    gl.uniform1f(gl.getUniformLocation(this.stackProg, "uPxToWorld"), PX_TO_WORLD);
    gl.bindVertexArray(this.stackVao);

    const animCodeMap = { static: 0, sway: 1, walk: 2, float: 3, bob: 4, spin: 5 };

    for (const [assetId, list] of groups.entries()) {
      const texEntry = this.assetTextures.get(assetId);
      const m = texEntry.meta;
      const slices = Math.min(m.slice_count, MAX_SLICES);

      gl.uniform1f(gl.getUniformLocation(this.stackProg, "uSliceCount"), slices);
      gl.uniform1f(gl.getUniformLocation(this.stackProg, "uSliceW"), m.slice_w);
      gl.uniform1f(gl.getUniformLocation(this.stackProg, "uSliceH"), m.slice_h);

      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, texEntry.slicesTex);
      gl.uniform1i(gl.getUniformLocation(this.stackProg, "uSlicesTex"), 0);

      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, texEntry.normalsTex);
      gl.uniform1i(gl.getUniformLocation(this.stackProg, "uNormalsTex"), 1);

      gl.activeTexture(gl.TEXTURE2);
      gl.bindTexture(gl.TEXTURE_2D, texEntry.emissiveTex);
      gl.uniform1i(gl.getUniformLocation(this.stackProg, "uEmissiveTex"), 2);

      const instData = new Float32Array(list.length * 14);
      for (let i = 0; i < list.length; i++) {
        const e = list[i];
        const baseZ = this.getTerrainElevationAt(e.x, e.y);
        const tint = hexToRgb01(e.tint_hex);
        const modeStr = e.anim_mode === "auto" ? m.anim_preset : e.anim_mode;
        const animCode = animCodeMap[modeStr] ?? 0;
        const mul = e._tweenMul || 1.0;
        const sx = (e.scale_x > 0.01 ? e.scale_x : (e.scale || 1.0)) * mul;
        const sy = (e.scale_y > 0.01 ? e.scale_y : (e.scale || 1.0)) * mul;
        const sz = (e.scale_z > 0.01 ? e.scale_z : (e.scale || 1.0)) * mul;
        const off = i * 14;
        instData[off + 0] = e.x;
        instData[off + 1] = e.y;
        instData[off + 2] = baseZ + e.z;
        instData[off + 3] = (e.yaw * Math.PI) / 180;
        instData[off + 4] = sx;
        instData[off + 5] = sy;
        instData[off + 6] = sz;
        instData[off + 7] = tint[0];
        instData[off + 8] = tint[1];
        instData[off + 9] = tint[2];
        instData[off + 10] = e.emissive_on ? 1.0 : 0.0;
        instData[off + 11] = animCode;
        instData[off + 12] = e._phase || 0.0;
        instData[off + 13] = e._reveal ?? 1.0;
      }

      gl.bindBuffer(gl.ARRAY_BUFFER, this.stackInstanceVbo);
      gl.bufferData(gl.ARRAY_BUFFER, instData, gl.DYNAMIC_DRAW);
      gl.drawArraysInstanced(gl.TRIANGLES, 0, slices * SUB_LAYERS * 6, list.length);
    }

    // 3. 繪製半透明波光水面
    if (this.waterVertCount > 0) {
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.depthMask(false);
      gl.useProgram(this.waterProg);
      gl.uniformMatrix4fv(gl.getUniformLocation(this.waterProg, "uViewProj"), false, viewProj);
      gl.uniform1f(gl.getUniformLocation(this.waterProg, "uTime"), nowSec);
      gl.uniform3fv(gl.getUniformLocation(this.waterProg, "uSunColor"), env.sunCol);
      gl.uniform3fv(gl.getUniformLocation(this.waterProg, "uAmbient"), env.ambient);
      gl.uniform1f(gl.getUniformLocation(this.waterProg, "uNightFactor"), env.nightFactor);
      gl.bindVertexArray(this.waterVao);
      gl.drawArrays(gl.TRIANGLES, 0, this.waterVertCount);
      gl.depthMask(true);
      gl.disable(gl.BLEND);
    }

    gl.bindVertexArray(null);

    // 4. 繪製 2D Overlay HUD（路徑線、漫遊圈、選取框、天氣雨雪、煙火粒子、對話氣泡）
    this._renderOverlay(dt, simState);
  }

  _renderOverlay(dt, simState) {
    const ctx = this.octx;
    const W = this.overlay.width;
    const H = this.overlay.height;
    ctx.clearRect(0, 0, W, H);

    // 天氣前景粒子（雨／雪）
    if (simState.weather === "rain") {
      ctx.strokeStyle = "rgba(186, 230, 253, 0.45)";
      ctx.lineWidth = 1.5;
      const t = performance.now() * 0.012;
      ctx.beginPath();
      for (let i = 0; i < 85; i++) {
        const x = ((i * 97 + t * 18) % (W + 80)) - 40;
        const y = ((i * 53 + t * 55) % (H + 80)) - 40;
        ctx.moveTo(x, y);
        ctx.lineTo(x - 6, y + 18);
      }
      ctx.stroke();
    } else if (simState.weather === "snow") {
      ctx.fillStyle = "rgba(255, 255, 255, 0.85)";
      const t = performance.now() * 0.003;
      for (let i = 0; i < 90; i++) {
        const x = ((i * 113 + Math.sin(t + i) * 35) % W + W) % W;
        const y = ((i * 71 + t * 26) % H);
        ctx.beginPath();
        ctx.arc(x, y, 2.2, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    // 編輯模式下繪製路徑、漫遊區域、懸停格點與選取框
    if (!simState.visitorMode) {
      for (const p of simState.paths) {
        if (!p.waypoints || p.waypoints.length === 0) continue;
        ctx.strokeStyle = p.color || "#38bdf8";
        ctx.lineWidth = 2.5 * this.dpr;
        ctx.setLineDash([6 * this.dpr, 4 * this.dpr]);
        ctx.beginPath();
        let started = false;
        const pts = p.loop_mode === "loop" && p.waypoints.length > 2
          ? [...p.waypoints, p.waypoints[0]]
          : p.waypoints;
        for (const wp of pts) {
          const gz = this.getTerrainElevationAt(wp.x, wp.y) + (wp.z || 0) + 0.08;
          const sc = this.projectToScreen(wp.x, wp.y, gz);
          if (sc) {
            if (!started) { ctx.moveTo(sc[0], sc[1]); started = true; }
            else ctx.lineTo(sc[0], sc[1]);
          }
        }
        ctx.stroke();
        ctx.setLineDash([]);

        p.waypoints.forEach((wp, idx) => {
          const gz = this.getTerrainElevationAt(wp.x, wp.y) + (wp.z || 0) + 0.08;
          const sc = this.projectToScreen(wp.x, wp.y, gz);
          if (!sc) return;
          ctx.fillStyle = p.color || "#38bdf8";
          ctx.beginPath();
          ctx.arc(sc[0], sc[1], 5 * this.dpr, 0, Math.PI * 2);
          ctx.fill();
          ctx.fillStyle = "#0f172a";
          ctx.font = `bold ${9 * this.dpr}px sans-serif`;
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          ctx.fillText(String(idx + 1), sc[0], sc[1]);
        });
      }

      // 草稿繪製中路徑
      if (this.draftPathWaypoints.length > 0) {
        ctx.strokeStyle = "#fde047";
        ctx.lineWidth = 3 * this.dpr;
        ctx.beginPath();
        this.draftPathWaypoints.forEach((wp, idx) => {
          const gz = this.getTerrainElevationAt(wp.x, wp.y) + 0.12;
          const sc = this.projectToScreen(wp.x, wp.y, gz);
          if (!sc) return;
          if (idx === 0) ctx.moveTo(sc[0], sc[1]);
          else ctx.lineTo(sc[0], sc[1]);
        });
        ctx.stroke();
      }

      // 漫遊圓圈
      for (const z of simState.wanderZones) {
        ctx.strokeStyle = z.color || "#4ade80";
        ctx.lineWidth = 2 * this.dpr;
        ctx.beginPath();
        const segs = 28;
        for (let i = 0; i <= segs; i++) {
          const a = (i / segs) * Math.PI * 2;
          const wx = z.cx + Math.cos(a) * z.radius;
          const wy = z.cy + Math.sin(a) * z.radius;
          const wz = this.getTerrainElevationAt(wx, wy) + 0.06;
          const sc = this.projectToScreen(wx, wy, wz);
          if (!sc) continue;
          if (i === 0) ctx.moveTo(sc[0], sc[1]);
          else ctx.lineTo(sc[0], sc[1]);
        }
        ctx.stroke();
      }

      // 滑鼠懸停格點輪廓
      if (this.hoverTile && this.hoverTile.x >= 0 && this.hoverTile.x < this.worldWidth &&
          this.hoverTile.y >= 0 && this.hoverTile.y < this.worldHeight) {
        const tx = this.hoverTile.x;
        const ty = this.hoverTile.y;
        const tz = this.getTerrainElevationAt(tx + 0.5, ty + 0.5) + 0.03;
        const c0 = this.projectToScreen(tx, ty, tz);
        const c1 = this.projectToScreen(tx + 1, ty, tz);
        const c2 = this.projectToScreen(tx + 1, ty + 1, tz);
        const c3 = this.projectToScreen(tx, ty + 1, tz);
        if (c0 && c1 && c2 && c3) {
          ctx.strokeStyle = "rgba(56, 189, 248, 0.9)";
          ctx.lineWidth = 2 * this.dpr;
          ctx.beginPath();
          ctx.moveTo(c0[0], c0[1]);
          ctx.lineTo(c1[0], c1[1]);
          ctx.lineTo(c2[0], c2[1]);
          ctx.lineTo(c3[0], c3[1]);
          ctx.closePath();
          ctx.stroke();
        }
      }

      // 生態筆刷圓形預覽範圍
      if (this.brushPreview.active && this.hoverWorldPos) {
        const bx = this.hoverWorldPos[0];
        const by = this.hoverWorldPos[1];
        const rad = this.brushPreview.radius || 3.0;
        ctx.strokeStyle = "#4ade80";
        ctx.lineWidth = 2.5 * this.dpr;
        ctx.setLineDash([5 * this.dpr, 4 * this.dpr]);
        ctx.beginPath();
        for (let i = 0; i <= 28; i++) {
          const a = (i / 28) * Math.PI * 2;
          const px = bx + Math.cos(a) * rad;
          const py = by + Math.sin(a) * rad;
          const pz = this.getTerrainElevationAt(px, py) + 0.08;
          const sc = this.projectToScreen(px, py, pz);
          if (!sc) continue;
          if (i === 0) ctx.moveTo(sc[0], sc[1]);
          else ctx.lineTo(sc[0], sc[1]);
        }
        ctx.stroke();
        ctx.setLineDash([]);
      }

      // 選中實體光環
      if (this.selectedEntityId) {
        const sel = simState.entities.find((e) => e.id === this.selectedEntityId);
        if (sel) {
          const gz = this.getTerrainElevationAt(sel.x, sel.y) + sel.z + 0.05;
          const sc = this.projectToScreen(sel.x, sel.y, gz);
          if (sc) {
            ctx.strokeStyle = "#fde047";
            ctx.lineWidth = 3 * this.dpr;
            ctx.beginPath();
            ctx.arc(sc[0], sc[1], 18 * this.dpr, 0, Math.PI * 2);
            ctx.stroke();
          }
        }
      }
    }

    // 玩家角色（Avatar）腳底金色光環、面向箭頭與頭頂名牌（搭乘時依載具調整光環與名牌圖示）
    if (simState.player && simState.player.visible) {
      const pl = simState.player;
      const mountedVeh = pl.mountedEntityId
        ? simState.entities.find((e) => e.id === pl.mountedEntityId)
        : null;
      const pz = this.getTerrainElevationAt(pl.x, pl.y) + (pl.z || 0) + 0.04;
      const ringZ = mountedVeh
        ? this.getTerrainElevationAt(mountedVeh.x, mountedVeh.y) + (mountedVeh.z || 0) + 0.05
        : pz;
      const ringX = mountedVeh ? mountedVeh.x : pl.x;
      const ringY = mountedVeh ? mountedVeh.y : pl.y;
      const ringRadius = mountedVeh ? 0.56 : 0.32;
      const centerSc = this.projectToScreen(ringX, ringY, ringZ);
      if (centerSc) {
        // 繪製地面金色光環（透過多邊形投影貼合透視）
        ctx.strokeStyle = mountedVeh ? "#fbbf24" : "#facc15";
        ctx.lineWidth = (mountedVeh ? 3.0 : 2.6) * this.dpr;
        ctx.beginPath();
        for (let i = 0; i <= 24; i++) {
          const a = (i / 24) * Math.PI * 2;
          const sc = this.projectToScreen(ringX + Math.cos(a) * ringRadius, ringY + Math.sin(a) * ringRadius, ringZ);
          if (!sc) continue;
          if (i === 0) ctx.moveTo(sc[0], sc[1]);
          else ctx.lineTo(sc[0], sc[1]);
        }
        ctx.stroke();

        // 玩家名牌
        const tagOffset = pl.playerHiddenInVehicle
          ? 1.15
          : 1.55 * (pl.scale_z || pl.scale || 1.0);
        const tagSc = this.projectToScreen(pl.x, pl.y, pz + tagOffset);
        if (tagSc) {
          const tFn = window.__stackCityI18n?.tr;
          const mountIcon = mountedVeh && simState._getMountIcon
            ? simState._getMountIcon(mountedVeh, pl.mountConfig)
            : "⭐";
          const rawPlLabel = `${mountIcon} ${pl.name || "冒險玩家"}`;
          const plLabel = tFn ? tFn(rawPlLabel) : rawPlLabel;
          ctx.font = `bold ${11 * this.dpr}px "Noto Sans TC", sans-serif`;
          const pw = ctx.measureText(plLabel).width + 14 * this.dpr;
          const ph = 20 * this.dpr;
          ctx.fillStyle = mountedVeh ? "rgba(146, 64, 14, 0.9)" : "rgba(180, 83, 9, 0.88)";
          ctx.strokeStyle = "#fde047";
          ctx.lineWidth = 1.6 * this.dpr;
          ctx.beginPath();
          ctx.roundRect(tagSc[0] - pw * 0.5, tagSc[1] - ph, pw, ph, 6 * this.dpr);
          ctx.fill();
          ctx.stroke();
          ctx.fillStyle = "#fef9c3";
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          ctx.fillText(plLabel, tagSc[0], tagSc[1] - ph * 0.5);
        }
      }
    }

    // 任務 NPC／物件頭頂角色徽章（線索／考驗／劫匪／支援／寶藏／嫌疑人）
    const nearestId = simState.nearestToPlayer?.id || null;
    const nearestRideId = !simState.player?.mountedEntityId ? (simState.nearestRideable?.entity?.id || null) : null;
    const nearestPortalId = simState.nearestPortal?.entity?.id || null;
    const hoverId = (this.camera.walkMode || simState.visitorMode) && this.hoverEntity ? this.hoverEntity.id : null;

    // 建築物「門」與傳送門地面呼吸光圈及門口標記
    if (simState.getPortalWorldPos) {
      const pulse = 0.5 + 0.5 * Math.sin(performance.now() * 0.0042);
      for (const ent of simState.entities || []) {
        if (!ent.visible) continue;
        const pInfo = simState.getPortalWorldPos(ent);
        if (!pInfo) continue;
        const { x: dx, y: dy, portal } = pInfo;
        const dz = this.getTerrainElevationAt(dx, dy) + 0.06;
        const isExit = portal.kind === "exit";
        const ringCol = isExit ? "rgba(251, 191, 36, 0.9)" : "rgba(56, 189, 248, 0.92)";
        const rad = 0.42 + pulse * 0.1;

        ctx.strokeStyle = ringCol;
        ctx.lineWidth = 2.4 * this.dpr;
        ctx.beginPath();
        for (let i = 0; i <= 20; i++) {
          const a = (i / 20) * Math.PI * 2;
          const sc = this.projectToScreen(dx + Math.cos(a) * rad, dy + Math.sin(a) * rad, dz);
          if (!sc) continue;
          if (i === 0) ctx.moveTo(sc[0], sc[1]);
          else ctx.lineTo(sc[0], sc[1]);
        }
        ctx.stroke();

        const centerSc = this.projectToScreen(dx, dy, dz + 0.12);
        if (centerSc && ent.id !== nearestPortalId) {
          ctx.font = `bold ${11 * this.dpr}px "Noto Sans TC", sans-serif`;
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          ctx.fillText("🚪", centerSc[0], centerSc[1]);
        }
      }
    }

    if (simState.getQuestBadge) {
      for (const ent of simState.entities || []) {
        if (!ent.visible || ent.id === nearestId || ent.id === nearestRideId || ent.id === nearestPortalId || ent.id === hoverId) continue;
        const badge = simState.getQuestBadge(ent);
        if (!badge) continue;
        const ez = this.getTerrainElevationAt(ent.x, ent.y) + (ent.z || 0) + 1.45 * (ent.scale_z || ent.scale || 1.0);
        const sc = this.projectToScreen(ent.x, ent.y, ez);
        if (!sc) continue;
        ctx.font = `bold ${10 * this.dpr}px "Noto Sans TC", sans-serif`;
        const bText = window.__stackCityI18n?.tr ? window.__stackCityI18n.tr(badge.text) : badge.text;
        const bw = ctx.measureText(bText).width + 12 * this.dpr;
        const bh = 18 * this.dpr;
        ctx.fillStyle = badge.bg;
        ctx.beginPath();
        ctx.roundRect(sc[0] - bw * 0.5, sc[1] - bh, bw, bh, 5 * this.dpr);
        ctx.fill();
        ctx.fillStyle = badge.color || "#ffffff";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(bText, sc[0], sc[1] - bh * 0.5);
      }
    }

    const drawPromptPill = (scX, scY, text, bgColor) => {
      const label = window.__stackCityI18n?.tr ? window.__stackCityI18n.tr(text) : text;
      ctx.font = `bold ${11 * this.dpr}px "Noto Sans TC", sans-serif`;
      const tw = ctx.measureText(label).width + 16 * this.dpr;
      const th = 22 * this.dpr;
      ctx.fillStyle = bgColor;
      ctx.beginPath();
      ctx.roundRect(scX - tw * 0.5, scY - th, tw, th, 6 * this.dpr);
      ctx.fill();
      ctx.fillStyle = "#ffffff";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(label, scX, scY - th * 0.5);
      return th;
    };

    const getRideActionWord = (ent, cfg) => {
      if (!cfg) return "搭乘";
      if (cfg.mode === "passenger") return "搭乘巡航";
      if (cfg.mobility === "water") return "登船駕駛";
      if (cfg.mode === "both" && cfg.hasPath) return "搭乘／駕駛";
      const s = `${ent?.name || ""}`;
      if (/(馬|horse|pony)/i.test(s) && !/(馬車|carriage|wagon)/i.test(s)) return "騎乘";
      return "駕駛";
    };

    // 玩家身旁可互動物件 [F] 鍵、門 [F] 🚪 提示與可搭乘交通工具 [G] 鍵提示（互不遮擋）
    const nr = simState.player && simState.player.visible && !simState.player.mountedEntityId
      ? simState.nearestRideable
      : null;
    const np = simState.player && simState.player.visible ? simState.nearestPortal : null;
    const ne = simState.player && simState.player.visible ? simState.nearestToPlayer : null;
    const neWillDraw = Boolean(ne && (!np || np.entity.id !== ne.id) && (!this.hoverEntity || this.hoverEntity.id !== ne.id));
    const sameTarget = Boolean(nr && neWillDraw && nr.entity.id === ne.id);

    if (np) {
      const pe = np.entity;
      const pPos = np.doorPos || { x: pe.x, y: pe.y };
      const pz = this.getTerrainElevationAt(pPos.x, pPos.y) + (pe.z || 0) + 1.25;
      const sc = this.projectToScreen(pPos.x, pPos.y, pz);
      if (sc) {
        const lockInfo = simState.checkPortalLock ? simState.checkPortalLock(np.portal) : { locked: false };
        const pLabel = np.portal.label || (np.portal.kind === "exit" ? "離開建築返回原世界" : `進入「${pe.name}」`);
        const rawLbl = lockInfo?.locked ? `[F] 🔒 ${pLabel}` : `[F] 🚪 ${pLabel}`;
        const pillBg = lockInfo?.locked ? "rgba(185, 28, 28, 0.94)" : "rgba(37, 99, 235, 0.94)";
        drawPromptPill(sc[0], sc[1], rawLbl, pillBg);
      }
    }

    if (neWillDraw) {
      const nz = this.getTerrainElevationAt(ne.x, ne.y) + (ne.z || 0) + 1.15;
      const sc = this.projectToScreen(ne.x, ne.y, nz);
      if (sc) {
        const qb = simState.getQuestBadge ? simState.getQuestBadge(ne) : null;
        const actLbl = qb ? qb.actionLabel : "互動";
        const rawLbl = this.camera.walkMode ? `[F] 或點擊${actLbl}：${ne.name}` : `[F] ${actLbl}：${ne.name}`;
        const th = drawPromptPill(sc[0], sc[1], rawLbl, "rgba(16, 185, 129, 0.92)");
        if (sameTarget) {
          const rideLbl = getRideActionWord(nr.entity, nr.config);
          const icon = simState._getMountIcon ? simState._getMountIcon(nr.entity, nr.config) : "🚗";
          drawPromptPill(sc[0], sc[1] - th - 5 * this.dpr, `[G] ${icon} ${rideLbl}：${nr.entity.name}`, "rgba(217, 119, 6, 0.95)");
        }
      }
    }

    if (nr && !sameTarget) {
      const re = nr.entity;
      const rz = this.getTerrainElevationAt(re.x, re.y) + (re.z || 0) + 1.2;
      const sc = this.projectToScreen(re.x, re.y, rz);
      if (sc) {
        const rideLbl = getRideActionWord(re, nr.config);
        const icon = simState._getMountIcon ? simState._getMountIcon(re, nr.config) : "🚗";
        drawPromptPill(sc[0], sc[1], `[G] ${icon} ${rideLbl}：${re.name}`, "rgba(217, 119, 6, 0.95)");
      }
    }

    // 街道漫步或訪客模式時，懸停物件顯示互動提示標籤
    if ((this.camera.walkMode || simState.visitorMode) && this.hoverEntity) {
      const he = this.hoverEntity;
      const hz = this.getTerrainElevationAt(he.x, he.y) + (he.z || 0) + 1.4;
      const sc = this.projectToScreen(he.x, he.y, hz);
      if (sc) {
        const hb = simState.getQuestBadge ? simState.getQuestBadge(he) : null;
        const actLbl = hb ? hb.actionLabel : "互動";
        const rawLbl = `👆 點擊${actLbl}：${he.name}`;
        const label = window.__stackCityI18n?.tr ? window.__stackCityI18n.tr(rawLbl) : rawLbl;
        ctx.font = `bold ${11 * this.dpr}px "Noto Sans TC", sans-serif`;
        const tw = ctx.measureText(label).width + 16 * this.dpr;
        const th = 22 * this.dpr;
        ctx.fillStyle = "rgba(14, 165, 233, 0.9)";
        ctx.beginPath();
        ctx.roundRect(sc[0] - tw * 0.5, sc[1] - th, tw, th, 6 * this.dpr);
        ctx.fill();
        ctx.fillStyle = "#ffffff";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(label, sc[0], sc[1] - th * 0.5);
      }
    }

    // 3D 爆發粒子更新與繪製
    const alive = [];
    for (const pt of this.particles) {
      pt.life += dt;
      if (pt.life >= pt.maxLife) continue;
      pt.x += pt.vx * dt;
      pt.y += pt.vy * dt;
      pt.z += pt.vz * dt;
      if (pt.effect === "firework") pt.vz -= 4.2 * dt;
      else if (pt.effect === "sparkle") pt.vz -= 2.4 * dt;

      const sc = this.projectToScreen(pt.x, pt.y, pt.z);
      if (sc) {
        const alpha = 1.0 - pt.life / pt.maxLife;
        ctx.globalAlpha = alpha;
        ctx.fillStyle = pt.color;
        ctx.beginPath();
        ctx.arc(sc[0], sc[1], pt.size * this.dpr * (0.6 + 0.4 * alpha), 0, Math.PI * 2);
        ctx.fill();
        ctx.globalAlpha = 1.0;
      }
      alive.push(pt);
    }
    this.particles = alive;

    // 頭頂像素對話氣泡（同步追蹤一般物件與玩家化身）
    const nowMs = performance.now();
    this.bubbles = this.bubbles.filter((b) => b.expiresAt > nowMs);
    for (const b of this.bubbles) {
      const ent = b.entityId === "__player__"
        ? simState.player
        : simState.entities.find((e) => e.id === b.entityId);
      const bx = ent ? ent.x : b.x;
      const by = ent ? ent.y : b.y;
      const bz = (ent ? this.getTerrainElevationAt(ent.x, ent.y) + (ent.z || 0) : b.z) + 2.05;
      const sc = this.projectToScreen(bx, by, bz);
      if (!sc) continue;
      ctx.font = `bold ${12 * this.dpr}px "Noto Sans TC", sans-serif`;
      const tw = ctx.measureText(b.text).width;
      const pw = tw + 18 * this.dpr;
      const ph = 26 * this.dpr;
      const rx = sc[0] - pw * 0.5;
      const ry = sc[1] - ph;
      ctx.fillStyle = "rgba(15, 23, 42, 0.92)";
      ctx.strokeStyle = "#38bdf8";
      ctx.lineWidth = 2 * this.dpr;
      ctx.beginPath();
      ctx.roundRect(rx, ry, pw, ph, 8 * this.dpr);
      ctx.fill();
      ctx.stroke();
      ctx.fillStyle = "#f8fafc";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(b.text, sc[0], ry + ph * 0.5);
    }
  }
}

/// 產生單一精靈堆疊物件的 2D 等軸測預覽縮圖 Canvas（供資產庫卡片與轉盤用）
export function renderStackThumbnail(slicesImg, sliceCount, boxSize = 68, angleDeg = 35) {
  const c = document.createElement("canvas");
  c.width = boxSize;
  c.height = boxSize;
  const ctx = c.getContext("2d");
  ctx.imageSmoothingEnabled = false;
  const sw = slicesImg.width / Math.max(sliceCount, 1);
  const sh = slicesImg.height;
  const rad = (angleDeg * Math.PI) / 180;
  const maxDim = Math.max(sw, sh, sliceCount * 0.9);
  const scale = (boxSize * 0.65) / Math.max(maxDim, 1);
  const cx = boxSize * 0.5;
  const baseY = boxSize * 0.78;
  for (let z = 0; z < sliceCount; z++) {
    ctx.save();
    ctx.translate(cx, baseY - z * scale * 0.85);
    ctx.scale(scale, scale * 0.68);
    ctx.rotate(rad);
    ctx.drawImage(slicesImg, z * sw, 0, sw, sh, -sw * 0.5, -sh * 0.5, sw, sh);
    ctx.restore();
  }
  return c;
}

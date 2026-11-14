import {
  DENSITY,
  type Density,
  presetSettings,
  type Surface,
  type TrackBands,
  trackSurface,
  type WaveSettings,
} from './motion';

// The surface and camera maths from the reference page, unchanged.
const VERTEX = `#version 300 es
precision highp float;
layout(location = 0) in vec2 aGrid;
uniform float uFlow;
uniform float uRipple;
uniform float uRing;
uniform vec2 uRidge;
uniform vec4 uGain;
uniform vec3 uCamera;
uniform float uAspect;
uniform float uDpr;
out float vAlpha;

const float TAU = 6.28318530718;
const float CAMERA_DISTANCE = 2.5;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  float a = mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x);
  float b = mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x);
  return mix(a, b, u.y) * 2.0 - 1.0;
}

float height(vec2 g) {
  float ridge = exp(-pow((g.x - uRidge.x) / uRidge.y, 2.0));
  float wander = noise(g * vec2(1.6, 1.2) + uFlow * vec2(0.11, -0.07));
  float swell = sin(TAU * (1.30 * g.x + 0.30 * g.y) + uFlow + 0.5 * wander);
  float cross = sin(TAU * (2.4 * g.x - 1.1 * g.y) - uFlow * 1.6)
              * sin(TAU * 0.75 * g.y + uFlow * 0.6);
  float ripple = sin(TAU * (9.0 * g.x + 2.5 * g.y) + uRipple);
  float fromCrest = length((g - vec2(uRidge.x, 0.45)) * vec2(1.0, 0.62));
  float ring = exp(-pow((fromCrest - uRing) / 0.06, 2.0));

  return (0.11 + 0.24 * ridge) * swell * uGain.x
       + 0.075 * cross * uGain.y
       + 0.010 * ripple * uGain.z
       + 0.07 * ring * uGain.w;
}

void main() {
  float h = height(aGrid);
  vec3 p = vec3((aGrid.x - 0.5) * 3.5, h, (aGrid.y - 0.5) * 2.0);

  float c = cos(uCamera.x);
  float s = sin(uCamera.x);
  float screenY = p.y * c + p.z * s;
  float depth = CAMERA_DISTANCE + p.z * c - p.y * s;
  float fit = min(1.0, uAspect / 1.9);
  float scale = 2.0 * uCamera.y * fit / depth;

  gl_Position = vec4(p.x * scale / uAspect, screenY * scale * 1.15 - 0.06, 0.0, 1.0);

  float edge = smoothstep(0.0, 0.16, aGrid.x) * smoothstep(1.0, 0.84, aGrid.x)
             * smoothstep(0.0, 0.22, aGrid.y) * smoothstep(1.0, 0.72, aGrid.y);
  float crest = smoothstep(-0.22, 0.30, h);
  vAlpha = edge * (0.22 + 0.78 * crest);
  gl_PointSize = max(1.0, uDpr * uCamera.z * mix(0.9, 1.7, crest) * scale);
}`;

const FRAGMENT = `#version 300 es
precision mediump float;
uniform vec3 uColor;
in float vAlpha;
out vec4 outColor;
void main() { outColor = vec4(uColor, vAlpha); }`;

const UNIFORMS = [
  'uFlow',
  'uRipple',
  'uRing',
  'uRidge',
  'uGain',
  'uCamera',
  'uAspect',
  'uDpr',
  'uColor',
] as const;

export type Rgb = [number, number, number];
type Locations = Record<(typeof UNIFORMS)[number], WebGLUniformLocation | null>;

/** Draws the surface as points in one draw call. */
export class WaveRenderer {
  readonly #gl: WebGL2RenderingContext;
  readonly #at: Locations;
  #points = 0;
  #density: Density | null = null;
  background: Rgb = [0, 0, 0];
  foreground: Rgb = [1, 1, 1];

  /** Null when the browser has no WebGL2, so the caller can leave the space empty. */
  static on(canvas: HTMLCanvasElement): WaveRenderer | null {
    const gl = canvas.getContext('webgl2', {
      antialias: false,
      alpha: false,
      powerPreference: 'low-power',
    });
    return gl ? new WaveRenderer(gl) : null;
  }

  private constructor(gl: WebGL2RenderingContext) {
    this.#gl = gl;
    const program = gl.createProgram();
    for (const [type, source] of [
      [gl.VERTEX_SHADER, VERTEX],
      [gl.FRAGMENT_SHADER, FRAGMENT],
    ] as const) {
      const shader = gl.createShader(type);
      if (!shader) throw new Error('The wave could not make a shader.');
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS))
        throw new Error(gl.getShaderInfoLog(shader) ?? 'The wave shader did not compile.');
      gl.attachShader(program, shader);
    }
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS))
      throw new Error(gl.getProgramInfoLog(program) ?? 'The wave shaders did not link.');
    gl.useProgram(program);
    this.#at = Object.fromEntries(
      UNIFORMS.map((name) => [name, gl.getUniformLocation(program, name)]),
    ) as Locations;

    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }

  /** Builds the grid of points once per density. */
  useDensity(density: Density) {
    if (density === this.#density) return;
    const [columns, rows] = DENSITY[density];
    const grid = new Float32Array(columns * rows * 2);
    for (let row = 0, i = 0; row < rows; row++) {
      for (let column = 0; column < columns; column++) {
        grid[i++] = column / (columns - 1);
        grid[i++] = row / (rows - 1);
      }
    }
    this.#gl.bufferData(this.#gl.ARRAY_BUFFER, grid, this.#gl.STATIC_DRAW);
    this.#points = columns * rows;
    this.#density = density;
  }

  resize(width: number, height: number, dpr: number) {
    const canvas = this.#gl.canvas;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    this.#gl.viewport(0, 0, width, height);
    this.#gl.uniform1f(this.#at.uAspect, width / height);
    this.#gl.uniform1f(this.#at.uDpr, dpr);
  }

  draw(surface: Surface, settings: WaveSettings) {
    const gl = this.#gl;
    const at = this.#at;
    const [r, g, b] = this.background;
    gl.clearColor(r, g, b, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.uniform3f(at.uColor, this.foreground[0], this.foreground[1], this.foreground[2]);
    gl.uniform1f(at.uFlow, surface.flow);
    gl.uniform1f(at.uRipple, surface.ripple);
    gl.uniform1f(at.uRing, surface.ring);
    gl.uniform2f(at.uRidge, surface.ridge, surface.width);
    gl.uniform4f(at.uGain, surface.gain[0], surface.gain[1], surface.gain[2], surface.gain[3]);
    gl.uniform3f(at.uCamera, settings.elevation, settings.zoom, settings.pointSize);
    gl.drawArrays(gl.POINTS, 0, this.#points);
  }

  release() {
    this.#gl.getExtension('WEBGL_lose_context')?.loseContext();
  }
}

export interface WaveThumbOptions {
  seed: number;
  bands?: TrackBands;
  width: number;
  height: number;
  dpr: number;
  background: Rgb;
  foreground: Rgb;
}

const MAX_CACHED_THUMBS = 256;
const thumbCache = new Map<string, HTMLCanvasElement>();
let sharedCanvas: HTMLCanvasElement | null = null;
let sharedRenderer: WaveRenderer | null | undefined;
const thumbSettings: WaveSettings = {
  ...presetSettings('still'),
  density: 'thumb',
  zoom: 1.05,
  pointSize: 0.7,
};

function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

function hash2d(x: number, y: number): number {
  const s = Math.sin(x * 127.1 + y * 311.7) * 43758.5453;
  return s - Math.floor(s);
}

function noise2d(x: number, y: number): number {
  const gridX = Math.floor(x);
  const gridY = Math.floor(y);
  const fx = x - gridX;
  const fy = y - gridY;
  const ux = fx * fx * (3 - 2 * fx);
  const uy = fy * fy * (3 - 2 * fy);
  const a = hash2d(gridX, gridY) * (1 - ux) + hash2d(gridX + 1, gridY) * ux;
  const b = hash2d(gridX, gridY + 1) * (1 - ux) + hash2d(gridX + 1, gridY + 1) * ux;
  return (a * (1 - uy) + b * uy) * 2 - 1;
}

/** 2D canvas projection when WebGL2 is unavailable. */
function drawCanvas2dFallback(
  canvas: HTMLCanvasElement,
  surface: Surface,
  settings: WaveSettings,
  dpr: number,
  background: Rgb,
  foreground: Rgb,
) {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const width = canvas.width;
  const height = canvas.height;
  const aspect = width / Math.max(1, height);
  const bgR = Math.round(background[0] * 255);
  const bgG = Math.round(background[1] * 255);
  const bgB = Math.round(background[2] * 255);
  const fgR = Math.round(foreground[0] * 255);
  const fgG = Math.round(foreground[1] * 255);
  const fgB = Math.round(foreground[2] * 255);

  ctx.fillStyle = `rgb(${bgR} ${bgG} ${bgB})`;
  ctx.fillRect(0, 0, width, height);

  const columns = 38;
  const rows = 16;
  const c = Math.cos(settings.elevation);
  const s = Math.sin(settings.elevation);
  const fit = Math.min(1, aspect / 1.9);
  const tau = Math.PI * 2;

  for (let row = 0; row < rows; row++) {
    const gy = row / (rows - 1);
    const edgeY = smoothstep(0, 0.22, gy) * smoothstep(1, 0.72, gy);
    for (let col = 0; col < columns; col++) {
      const gx = col / (columns - 1);
      const edge = edgeY * smoothstep(0, 0.16, gx) * smoothstep(1, 0.84, gx);
      if (edge <= 0.01) continue;

      const ridge = Math.exp(-(((gx - surface.ridge) / surface.width) ** 2));
      const wander = noise2d(gx * 1.6 + surface.flow * 0.11, gy * 1.2 - surface.flow * 0.07);
      const swell = Math.sin(tau * (1.3 * gx + 0.3 * gy) + surface.flow + 0.5 * wander);
      const cross =
        Math.sin(tau * (2.4 * gx - 1.1 * gy) - surface.flow * 1.6) *
        Math.sin(tau * 0.75 * gy + surface.flow * 0.6);
      const ripple = Math.sin(tau * (9 * gx + 2.5 * gy) + surface.ripple);
      const dx = gx - surface.ridge;
      const dy = (gy - 0.45) * 0.62;
      const fromCrest = Math.hypot(dx, dy);
      const ring = Math.exp(-(((fromCrest - surface.ring) / 0.06) ** 2));

      const h =
        (0.11 + 0.24 * ridge) * swell * surface.gain[0] +
        0.075 * cross * surface.gain[1] +
        0.01 * ripple * surface.gain[2] +
        0.07 * ring * surface.gain[3];

      const px = (gx - 0.5) * 3.5;
      const pz = (gy - 0.5) * 2.0;
      const screenY = h * c + pz * s;
      const depth = 2.5 + pz * c - h * s;
      const scale = (2 * settings.zoom * fit) / depth;

      const ndcX = (px * scale) / aspect;
      const ndcY = screenY * scale * 1.15 - 0.06;
      const sx = (ndcX * 0.5 + 0.5) * width;
      const sy = (0.5 - ndcY * 0.5) * height;
      const crest = smoothstep(-0.22, 0.3, h);
      const alpha = edge * (0.22 + 0.78 * crest);
      const size = Math.max(1, dpr * settings.pointSize * (0.9 + 0.8 * crest) * scale);

      ctx.fillStyle = `rgb(${fgR} ${fgG} ${fgB} / ${alpha.toFixed(3)})`;
      ctx.fillRect(sx - size * 0.5, sy - size * 0.5, size, size);
    }
  }
}

function getSharedRenderer(): { canvas: HTMLCanvasElement; renderer: WaveRenderer } | null {
  if (sharedRenderer === undefined) {
    sharedCanvas = document.createElement('canvas');
    sharedRenderer = WaveRenderer.on(sharedCanvas);
  }
  return sharedRenderer && sharedCanvas ? { canvas: sharedCanvas, renderer: sharedRenderer } : null;
}

export function thumbCacheKey(options: WaveThumbOptions): string {
  const bandsKey = options.bands
    ? [
        options.bands.low?.toFixed(2) ?? '',
        options.bands.mid?.toFixed(2) ?? '',
        options.bands.high?.toFixed(2) ?? '',
        options.bands.tone?.toFixed(2) ?? '',
        options.bands.spread?.toFixed(2) ?? '',
      ].join(',')
    : '';
  const bg = options.background.map((c) => Math.round(c * 255)).join(',');
  const fg = options.foreground.map((c) => Math.round(c * 255)).join(',');
  return `${options.seed}|${bandsKey}|${options.width}x${options.height}@${options.dpr}|${bg}|${fg}`;
}

/** Draws a track's dot-wave thumbnail through one shared renderer and a bitmap cache. */
export function drawWaveThumb(target: HTMLCanvasElement, options: WaveThumbOptions): string {
  const width = Math.max(1, Math.round(options.width));
  const height = Math.max(1, Math.round(options.height));
  if (target.width !== width || target.height !== height) {
    target.width = width;
    target.height = height;
  }

  const key = thumbCacheKey({ ...options, width, height });
  let cached = thumbCache.get(key);
  if (!cached) {
    cached = document.createElement('canvas');
    cached.width = width;
    cached.height = height;
    const surface = trackSurface(options.seed, options.bands);
    const shared = getSharedRenderer();
    const cachedCtx = cached.getContext('2d');
    if (shared && cachedCtx) {
      shared.renderer.background = options.background;
      shared.renderer.foreground = options.foreground;
      shared.renderer.resize(width, height, options.dpr);
      shared.renderer.useDensity(thumbSettings.density);
      shared.renderer.draw(surface, thumbSettings);
      cachedCtx.drawImage(shared.canvas, 0, 0);
    } else {
      drawCanvas2dFallback(
        cached,
        surface,
        thumbSettings,
        options.dpr,
        options.background,
        options.foreground,
      );
    }
    if (thumbCache.size >= MAX_CACHED_THUMBS) {
      const oldest = thumbCache.keys().next().value;
      if (oldest !== undefined) thumbCache.delete(oldest);
    }
    thumbCache.set(key, cached);
  }

  const ctx = target.getContext('2d');
  if (ctx) {
    ctx.clearRect(0, 0, width, height);
    ctx.drawImage(cached, 0, 0);
  }
  return key;
}

export function hasCachedThumb(key: string): boolean {
  return thumbCache.has(key);
}

export function thumbCacheSize(): number {
  return thumbCache.size;
}

export function clearThumbCache(): void {
  thumbCache.clear();
}

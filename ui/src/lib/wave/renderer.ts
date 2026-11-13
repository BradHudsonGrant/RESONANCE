import { DENSITY, type Density, type Surface, type WaveSettings } from './motion';

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

type Rgb = [number, number, number];
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

import { BIN_BYTES, type WaveformFrame } from './frames';

/** Seconds of music across a deck's lane. */
export const SECONDS_IN_VIEW = 10;

/** Bins in one row of a waveform texture. Rows keep long tracks inside the texture size limit. */
const ROW = 4096;

/** The most bins one column reads. Past this the lane is zoomed out further than it ever is. */
const MOST_BINS_PER_COLUMN = 32;

const VERTEX = `#version 300 es
void main() {
  // One triangle that covers the viewport.
  vec2 corner = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0);
  gl_Position = vec4(corner, 0.0, 1.0);
}`;

// Each fragment finds the bins under its column and compares its distance from the centre line
// with their peak and their high, mid and low band levels. The peak is a faint outline and each
// band inside it is brighter the lower it is, so kicks and bass read as the solid core and hats as
// a fringe, with no colour. Columns are whole device pixels and nothing is blended, so the bars
// are one pixel wide with hard edges. Played bars use Text; upcoming bars use Text 2.
const FRAGMENT = `#version 300 es
precision highp float;
precision highp int;
uniform sampler2D uBins;
uniform vec4 uLane;
uniform float uBinCount;
uniform float uFirstColumn;
uniform float uBinsPerColumn;
uniform float uPlayheadColumn;
uniform float uPlayheadCapHalf;
uniform float uPlayheadCapHeight;
uniform vec3 uBackground;
uniform vec3 uUpcoming;
uniform vec3 uPlayed;
uniform vec3 uPlayhead;
out vec4 outColor;

const int ROW = ${ROW};
const int MOST_BINS = ${MOST_BINS_PER_COLUMN};

void main() {
  vec2 p = gl_FragCoord.xy - uLane.xy;
  float column = floor(p.x);
  if (
    column == uPlayheadColumn ||
    (abs(column - uPlayheadColumn) <= uPlayheadCapHalf && p.y >= uLane.w - uPlayheadCapHeight)
  ) {
    outColor = vec4(uPlayhead, 1.0);
    return;
  }
  float from = (uFirstColumn + column) * uBinsPerColumn;
  int first = int(floor(from));
  int last = max(first, int(ceil(from + uBinsPerColumn)) - 1);
  vec4 level = vec4(0.0);
  for (int i = 0; i < MOST_BINS; i++) {
    int bin = first + i;
    if (bin > last) break;
    if (bin < 0 || float(bin >= 0 ? bin : 0) >= uBinCount) continue;
    level = max(level, texelFetch(uBins, ivec2(bin % ROW, bin / ROW), 0));
  }
  float halfHeight = uLane.w * 0.5;
  float fromCentre = abs(floor(p.y) + 0.5 - halfHeight) / halfHeight;
  vec3 tone = column < uPlayheadColumn ? uPlayed : uUpcoming;
  // Texels are peak, low, mid, high. A brighter layer wins where two cover the same pixel.
  float brightness = 0.0;
  if (fromCentre < level.r) brightness = 0.3;
  if (fromCentre < level.a) brightness = max(brightness, 0.5);
  if (fromCentre < level.b) brightness = max(brightness, 0.72);
  if (fromCentre < level.g) brightness = 1.0;
  outColor = vec4(mix(uBackground, tone, brightness), 1.0);
}`;

const UNIFORMS = [
  'uBins',
  'uLane',
  'uBinCount',
  'uFirstColumn',
  'uBinsPerColumn',
  'uPlayheadColumn',
  'uPlayheadCapHalf',
  'uPlayheadCapHeight',
  'uBackground',
  'uUpcoming',
  'uPlayed',
  'uPlayhead',
] as const;

type Rgb = [number, number, number];
type Locations = Record<(typeof UNIFORMS)[number], WebGLUniformLocation | null>;

export interface Palette {
  background: Rgb;
  divider: Rgb;
  /** Bars still to play. */
  upcoming: Rgb;
  played: Rgb;
  playhead: Rgb;
  meterTrack: Rgb;
  meter: Rgb;
  meterHot: Rgb;
}

/** What one deck lane shows in a frame. */
export interface LaneState {
  positionSeconds: number;
  meterLeft: number;
  meterRight: number;
}

interface Lane {
  texture: WebGLTexture;
  binCount: number;
  binsPerSecond: number;
}

/** The meter turns brighter above this point on its scale, which is -6 dB on a 48 dB scale. */
const HOT = 42 / 48;

/** Draws the decks' scrolling waveforms, playheads and meters into one canvas, two lanes high. */
export class MixRenderer {
  readonly #gl: WebGL2RenderingContext;
  readonly #at: Locations;
  readonly #lanes: [Lane, Lane];
  #width = 1;
  #height = 1;
  #dpr = 1;
  palette: Palette = {
    background: [0, 0, 0],
    divider: [0.2, 0.2, 0.2],
    upcoming: [0.7, 0.7, 0.7],
    played: [1, 1, 1],
    playhead: [1, 0, 0],
    meterTrack: [0.2, 0.2, 0.2],
    meter: [0.7, 0.7, 0.7],
    meterHot: [1, 1, 1],
  };

  /** Null when the browser has no WebGL2. */
  static on(canvas: HTMLCanvasElement): MixRenderer | null {
    const gl = canvas.getContext('webgl2', {
      antialias: false,
      alpha: false,
      depth: false,
      stencil: false,
      powerPreference: 'high-performance',
    });
    return gl ? new MixRenderer(gl) : null;
  }

  private constructor(gl: WebGL2RenderingContext) {
    this.#gl = gl;
    const program = gl.createProgram();
    for (const [type, source] of [
      [gl.VERTEX_SHADER, VERTEX],
      [gl.FRAGMENT_SHADER, FRAGMENT],
    ] as const) {
      const shader = gl.createShader(type);
      if (!shader) throw new Error('The mix view could not make a shader.');
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS))
        throw new Error(gl.getShaderInfoLog(shader) ?? 'A mix view shader did not compile.');
      gl.attachShader(program, shader);
    }
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS))
      throw new Error(gl.getProgramInfoLog(program) ?? 'The mix view shaders did not link.');
    gl.useProgram(program);
    this.#at = Object.fromEntries(
      UNIFORMS.map((name) => [name, gl.getUniformLocation(program, name)]),
    ) as Locations;
    gl.uniform1i(this.#at.uBins, 0);
    gl.bindVertexArray(gl.createVertexArray());
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    this.#lanes = [this.#emptyLane(), this.#emptyLane()];
  }

  #emptyLane(): Lane {
    const gl = this.#gl;
    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA8,
      1,
      1,
      0,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      new Uint8Array(4),
    );
    return { texture, binCount: 0, binsPerSecond: 1 };
  }

  /** Puts a deck's waveform on the GPU. Called once per load, never per frame. */
  setWaveform(frame: WaveformFrame): void {
    const gl = this.#gl;
    const lane = this.#lanes[frame.deck];
    const count = frame.bins.length / BIN_BYTES;
    const rows = Math.max(1, Math.ceil(count / ROW));
    const padded = new Uint8Array(ROW * rows * BIN_BYTES);
    padded.set(frame.bins);
    gl.bindTexture(gl.TEXTURE_2D, lane.texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, ROW, rows, 0, gl.RGBA, gl.UNSIGNED_BYTE, padded);
    lane.binCount = count;
    lane.binsPerSecond = frame.binsPerSecond;
  }

  clearWaveform(deck: 0 | 1): void {
    this.#lanes[deck].binCount = 0;
  }

  resize(width: number, height: number, dpr: number): void {
    const canvas = this.#gl.canvas;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    this.#width = width;
    this.#height = height;
    this.#dpr = dpr;
  }

  draw(a: LaneState, b: LaneState): void {
    const gl = this.#gl;
    const divider = Math.max(1, Math.round(this.#dpr));
    const meterPad = Math.round(8 * this.#dpr);
    const meterBar = Math.round(4 * this.#dpr);
    const meterGap = Math.round(4 * this.#dpr);
    const meterWidth = 2 * meterBar + meterGap + 2 * meterPad;
    const laneWidth = Math.max(1, this.#width - meterWidth);
    const laneHeight = Math.max(2, Math.floor((this.#height - divider) / 2));
    const [r, g, bl] = this.palette.background;
    gl.disable(gl.SCISSOR_TEST);
    gl.viewport(0, 0, this.#width, this.#height);
    gl.clearColor(r, g, bl, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    // Deck A sits on top. GL counts rows from the bottom.
    this.#drawLane(0, a, laneHeight + divider, laneWidth, laneHeight);
    this.#drawLane(1, b, 0, laneWidth, laneHeight);
    gl.enable(gl.SCISSOR_TEST);
    this.#fill(0, laneHeight, this.#width, divider, this.palette.divider);
    const meterLeft = laneWidth + meterPad;
    this.#drawMeter(meterLeft, laneHeight + divider, meterBar, laneHeight, a.meterLeft);
    this.#drawMeter(
      meterLeft + meterBar + meterGap,
      laneHeight + divider,
      meterBar,
      laneHeight,
      a.meterRight,
    );
    this.#drawMeter(meterLeft, 0, meterBar, laneHeight, b.meterLeft);
    this.#drawMeter(meterLeft + meterBar + meterGap, 0, meterBar, laneHeight, b.meterRight);
  }

  #drawLane(deck: 0 | 1, state: LaneState, bottom: number, width: number, height: number): void {
    const gl = this.#gl;
    const at = this.#at;
    const lane = this.#lanes[deck];
    const binsPerColumn = (lane.binsPerSecond * SECONDS_IN_VIEW) / width;
    const playheadColumn = Math.floor(width / 2);
    const capSize = Math.max(3, Math.round(5 * this.#dpr));
    const capHalf = Math.floor((capSize - 1) / 2);
    // Columns are counted from the start of the track, so a column always shows the same bins
    // and the picture moves in whole pixels.
    const playheadAt = Math.floor((state.positionSeconds * lane.binsPerSecond) / binsPerColumn);
    gl.viewport(0, bottom, width, height);
    gl.bindTexture(gl.TEXTURE_2D, lane.texture);
    gl.uniform4f(at.uLane, 0, bottom, width, height);
    gl.uniform1f(at.uBinCount, lane.binCount);
    gl.uniform1f(at.uFirstColumn, playheadAt - playheadColumn);
    gl.uniform1f(at.uBinsPerColumn, binsPerColumn);
    gl.uniform1f(at.uPlayheadColumn, playheadColumn);
    gl.uniform1f(at.uPlayheadCapHalf, capHalf);
    gl.uniform1f(at.uPlayheadCapHeight, capSize);
    const p = this.palette;
    gl.uniform3f(at.uBackground, p.background[0], p.background[1], p.background[2]);
    gl.uniform3f(at.uUpcoming, p.upcoming[0], p.upcoming[1], p.upcoming[2]);
    gl.uniform3f(at.uPlayed, p.played[0], p.played[1], p.played[2]);
    gl.uniform3f(at.uPlayhead, p.playhead[0], p.playhead[1], p.playhead[2]);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  #drawMeter(left: number, bottom: number, width: number, height: number, level: number): void {
    const p = this.palette;
    this.#fill(left, bottom, width, height, p.meterTrack);
    const lit = Math.round(Math.min(level, HOT) * height);
    if (lit > 0) this.#fill(left, bottom, width, lit, p.meter);
    const hot = Math.round(level * height) - lit;
    if (hot > 0) this.#fill(left, bottom + lit, width, hot, p.meterHot);
  }

  #fill(left: number, bottom: number, width: number, height: number, colour: Rgb): void {
    const gl = this.#gl;
    gl.scissor(left, bottom, width, height);
    gl.clearColor(colour[0], colour[1], colour[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
  }

  release(): void {
    this.#gl.getExtension('WEBGL_lose_context')?.loseContext();
  }
}

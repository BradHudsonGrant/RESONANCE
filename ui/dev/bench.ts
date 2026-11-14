import { type ChildProcess, spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as wait } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

import { type Browser, chromium, type Page } from '@playwright/test';
import { createServer, type ViteDevServer } from 'vite';

import type { ScrollMeasurement } from '../src/lib/library/measure-scroll.ts';
import type { MixMeasurement } from '../src/lib/mix/measure-mix.ts';
import type { WaveMeasurement } from '../src/lib/wave/measure-wave.ts';

/**
 * The page half of `resonance bench`: everything that needs a window. It runs the release desktop
 * app over the WebView2 debugging port and the Mix view in Edge against its own core, and writes
 * the results to target/bench/page.json for `resonance bench` to put in the record. `just bench`
 * builds what it needs first.
 */

const root = fileURLToPath(new URL('../../', import.meta.url));
const bench = join(root, 'target', 'bench');
const appBinary = join(root, 'target', 'release', 'resonance-app.exe');
const coreBinary = join(root, 'target', 'release', 'resonance.exe');
const output = join(bench, 'page.json');
const synthetic = join(bench, 'synthetic');
const mixData = join(bench, 'mix');
const connectionFile = join(bench, 'connection.json');
const startupTrace = join(bench, 'startup-trace.json');
const fixturesLocal = join(root, 'fixtures-local');
const syntheticTracks = 100_000;
const debugPort = 9351;
const vitePort = 5181;
const timedLaunches = 5;
const runs = 5;
// Long enough for the Mix runs here and the audio run in `resonance bench` after them.
const shortestTrackSeconds = 240;

interface Launch {
  /** From starting the process to the page's navigation start. */
  navigationStartMs: number;
  /** From starting the process to the first filled rows in the table. */
  filledMs: number;
}

interface Memory {
  processes: number;
  workingSetBytes: number;
  privateWorkingSetBytes: number;
}

interface Viewport {
  width: number;
  height: number;
  devicePixelRatio: number;
}

interface TraceEvent {
  ph: string;
  name: string;
  pid: number;
  tid: number;
  dur?: number;
  args?: { name?: string };
}

// Desktop app processes still running, so a failed run never leaves a window behind.
const started: ChildProcess[] = [];
let core: ChildProcess | undefined;
let vite: ViteDevServer | undefined;
let edge: Browser | undefined;

function run(binary: string, ...args: string[]) {
  const done = spawnSync(binary, args, { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] });
  if (done.status !== 0) throw new Error(`${binary} ${args.join(' ')} failed`);
}

function now(): number {
  return performance.timeOrigin + performance.now();
}

function launchApp(browserArguments: string): { app: ChildProcess; launchedAt: number } {
  const launchedAt = now();
  const app = spawn(appBinary, ['--data-folder', synthetic], {
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: browserArguments },
    stdio: 'ignore',
  });
  started.push(app);
  return { app, launchedAt };
}

async function attach(): Promise<{ browser: Browser; page: Page }> {
  for (let tries = 0; ; tries++) {
    try {
      const browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
      const context = browser.contexts()[0];
      const page = context?.pages()[0] ?? (await context?.waitForEvent('page'));
      if (!page) throw new Error('The desktop app has no page.');
      return { browser, page };
    } catch (error) {
      if (tries > 200) throw error;
      await wait(50);
    }
  }
}

/** Asks the window to close, as its close button would, so the webview shuts down cleanly. */
async function quit(child: ChildProcess) {
  if (child.exitCode === null && child.pid !== undefined) {
    const exited = new Promise((resolve) => child.once('exit', resolve));
    spawnSync('taskkill', ['/PID', String(child.pid)], { stdio: 'ignore' });
    await Promise.race([exited, wait(10_000)]);
    if (child.exitCode === null) {
      spawnSync('taskkill', ['/F', '/T', '/PID', String(child.pid)], { stdio: 'ignore' });
    }
  }
  started.splice(started.indexOf(child), 1);
}

/** The process and every process under it, such as the webview's browser and renderers. */
function memoryOf(pid: number): Memory {
  const query =
    'Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | ' +
    'Select-Object IDProcess, CreatingProcessID, WorkingSet, WorkingSetPrivate | ConvertTo-Json -Compress';
  const done = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', query], {
    encoding: 'utf8',
  });
  const rows = JSON.parse(done.stdout) as {
    IDProcess: number;
    CreatingProcessID: number;
    WorkingSet: number;
    WorkingSetPrivate: number;
  }[];
  const tree = new Set([pid]);
  for (let grew = true; grew;) {
    grew = false;
    for (const row of rows) {
      if (row.IDProcess !== 0 && !tree.has(row.IDProcess) && tree.has(row.CreatingProcessID)) {
        tree.add(row.IDProcess);
        grew = true;
      }
    }
  }
  const inTree = rows.filter((row) => tree.has(row.IDProcess));
  return {
    processes: inTree.length,
    workingSetBytes: inTree.reduce((sum, row) => sum + row.WorkingSet, 0),
    privateWorkingSetBytes: inTree.reduce((sum, row) => sum + row.WorkingSetPrivate, 0),
  };
}

/** The longest task on the page's main thread, which is the UI thread the budget means. */
function longestMainTaskMs(events: TraceEvent[]): number {
  const mainThreads = new Set(
    events
      .filter((e) => e.ph === 'M' && e.name === 'thread_name' && e.args?.name === 'CrRendererMain')
      .map((e) => `${e.pid}:${e.tid}`),
  );
  let longest = 0;
  for (const e of events) {
    if (e.ph !== 'X' || !mainThreads.has(`${e.pid}:${e.tid}`)) continue;
    if (e.name !== 'ThreadControllerImpl::RunTask' && e.name !== 'RunTask') continue;
    longest = Math.max(longest, (e.dur ?? 0) / 1000);
  }
  return longest;
}

async function traced(browser: Browser, page: Page, work: () => Promise<unknown>) {
  await browser.startTracing(page, { categories: ['toplevel'] });
  await work();
  await wait(2_000);
  const trace = JSON.parse((await browser.stopTracing()).toString()) as {
    traceEvents: TraceEvent[];
  };
  return longestMainTaskMs(trace.traceEvents);
}

function viewportOf(page: Page): Promise<Viewport> {
  return page.evaluate(() => ({
    width: innerWidth,
    height: innerHeight,
    devicePixelRatio,
  }));
}

function gpuOf(page: Page): Promise<string> {
  return page.evaluate(() => {
    const gl = document.createElement('canvas').getContext('webgl2');
    const info = gl?.getExtension('WEBGL_debug_renderer_info');
    return gl && info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : 'unknown';
  });
}

async function startupLongestTaskMs(): Promise<number> {
  rmSync(startupTrace, { force: true });
  const { app } = launchApp(
    `--trace-startup=toplevel --trace-startup-format=json --trace-startup-duration=5 --trace-startup-file=${startupTrace}`,
  );
  let events: TraceEvent[] | undefined;
  for (let tries = 0; tries < 300 && !events; tries++) {
    await wait(100);
    try {
      events = (JSON.parse(readFileSync(startupTrace, 'utf8')) as { traceEvents: TraceEvent[] })
        .traceEvents;
    } catch {
      // The file appears before the trace in it is whole.
    }
  }
  await quit(app);
  rmSync(startupTrace, { force: true });
  if (!events) throw new Error('The webview wrote no startup trace.');
  return longestMainTaskMs(events);
}

async function measureDesktop() {
  if (!existsSync(join(synthetic, 'library.db'))) {
    const fixtures = ['run', '--quiet', '--release', '--package', 'resonance-fixtures'];
    run('cargo', ...fixtures, '--bin', 'synthetic-library', '--', synthetic, `${syntheticTracks}`);
  }
  // The first launch makes the webview's profile, as the first launch after an install does.
  rmSync(join(synthetic, 'EBWebView'), { recursive: true, force: true });
  const debugging = `--remote-debugging-port=${debugPort}`;

  const launches: Launch[] = [];
  let open: { app: ChildProcess; browser: Browser; page: Page } | undefined;
  for (let launch = 0; launch <= timedLaunches; launch++) {
    const { app, launchedAt } = launchApp(debugging);
    // Attaching while the page starts would slow it, so the timings are read afterwards.
    await wait(3_000);
    const { browser, page } = await attach();
    await page.waitForFunction(() => performance.getEntriesByName('tracks-filled').length > 0);
    const marks = await page.evaluate(() => ({
      origin: performance.timeOrigin,
      filled:
        performance.timeOrigin + (performance.getEntriesByName('tracks-filled')[0]?.startTime ?? 0),
    }));
    launches.push({
      navigationStartMs: marks.origin - launchedAt,
      filledMs: marks.filled - launchedAt,
    });
    if (launch < timedLaunches) {
      await browser.close();
      await quit(app);
    } else {
      open = { app, browser, page };
    }
  }
  if (!open) throw new Error('The desktop app did not stay open.');
  const { app, browser, page } = open;

  // Ten seconds after launch, with nothing touched since the table filled.
  await wait(7_000);
  const idleMemory = memoryOf(app.pid ?? 0);

  // The benchmark build's manual controls would sit over the table.
  await page.evaluate(() =>
    document.querySelector('[aria-label="Library scroll benchmark"]')?.remove(),
  );
  const scroll: ScrollMeasurement[] = [];
  for (let pass = 0; pass < runs; pass++) {
    await wait(1_000);
    scroll.push(await page.evaluate(() => window.measureTableScroll()));
  }
  const wave: WaveMeasurement[] = [];
  for (let pass = 0; pass < runs; pass++) {
    wave.push(await page.evaluate(() => window.measureWave()));
  }
  const tracedLongestTaskMs = await traced(browser, page, () =>
    page.evaluate(async () => {
      await window.measureTableScroll();
      await window.measureWave();
    }),
  );
  const afterRunsMemory = memoryOf(app.pid ?? 0);
  const result = {
    webview: browser.version(),
    gpu: await gpuOf(page),
    viewport: await viewportOf(page),
    tracks: syntheticTracks,
    firstLaunch: launches[0],
    launches: launches.slice(1),
    idleMemory,
    scroll,
    wave,
    tracedLongestTaskMs,
    afterRunsMemory,
  };
  await browser.close();
  await quit(app);
  return { ...result, startupLongestTaskMs: await startupLongestTaskMs() };
}

function deck(page: Page, name: string) {
  return page.locator(`[aria-label="${name}"]`);
}

/** The deck shows "0:00 / 5:23"; the length is zero until the core has read it. */
async function durationSeconds(page: Page, name: string): Promise<number> {
  for (let tries = 0; tries < 150; tries++) {
    const shown = (await deck(page, name).locator('.time').textContent()) ?? '';
    const [minutes = 0, seconds = 0] = (shown.split('/')[1] ?? '').trim().split(':').map(Number);
    if (minutes * 60 + seconds > 0) return minutes * 60 + seconds;
    await wait(100);
  }
  return 0;
}

/** Loads the first MP3 not already used that is long enough for every run, and returns it. */
async function loadLongTrack(page: Page, name: string, skip: string[]): Promise<string> {
  const choice = page.getByRole('combobox', { name: `Track on ${name}` });
  const tracks = await choice
    .locator('option')
    .evaluateAll((options) => options.map((option) => (option as HTMLOptionElement).value));
  for (const track of tracks.filter((t) => t.toLowerCase().endsWith('.mp3'))) {
    if (skip.includes(track)) continue;
    await choice.selectOption(track);
    if ((await durationSeconds(page, name)) >= shortestTrackSeconds) return track;
  }
  throw new Error(`fixtures-local has no two MP3s of ${shortestTrackSeconds} s or more.`);
}

async function measureMix() {
  if (!existsSync(fixturesLocal)) throw new Error('Copy real tracks first: just fixtures-local');
  run(coreBinary, '--data-folder', mixData, 'scan', fixturesLocal);
  rmSync(connectionFile, { force: true });
  core = spawn(
    coreBinary,
    ['--data-folder', mixData, 'serve', '--connection-file', connectionFile],
    {
      stdio: 'ignore',
    },
  );
  for (let tries = 0; !existsSync(connectionFile); tries++) {
    if (tries > 200 || core.exitCode !== null) throw new Error('The core did not start.');
    await wait(50);
  }
  // The dev server's proxy reads this, so the page reaches the core started here and no other.
  process.env.RESONANCE_CONNECTION_FILE = connectionFile;
  vite = await createServer({
    configFile: join(root, 'ui', 'vite.config.ts'),
    root: join(root, 'ui'),
    server: { port: vitePort, strictPort: true },
    logLevel: 'warn',
  });
  await vite.listen();

  // The window's real size and pixel ratio, as the desktop app has, rather than an emulated one.
  edge = await chromium.launch({
    channel: 'msedge',
    headless: false,
    args: ['--window-size=1440,900'],
  });
  const page = await edge.newPage({ viewport: null });
  await page.goto(`http://localhost:${vitePort}/`);
  await page.getByRole('combobox', { name: 'Commands' }).fill('mix');
  await page.getByRole('option', { name: 'Go to Mix' }).click();

  const first = await loadLongTrack(page, 'Deck A', []);
  const second = await loadLongTrack(page, 'Deck B', [first]);
  for (const name of ['Deck A', 'Deck B']) {
    await deck(page, name).getByRole('button', { name: 'Play' }).click();
  }
  await wait(2_000);

  const measured: MixMeasurement[] = [];
  for (let pass = 0; pass < runs; pass++) {
    measured.push(await page.evaluate(() => window.measureMix()));
    await wait(1_000);
  }
  const tracedLongestTaskMs = await traced(edge, page, () =>
    page.evaluate(() => window.measureMix()),
  );
  const canvas = await page
    .locator('canvas.lanes')
    .evaluate((element) => [
      (element as HTMLCanvasElement).width,
      (element as HTMLCanvasElement).height,
    ]);
  for (const name of ['Deck A', 'Deck B']) {
    await deck(page, name).getByRole('button', { name: 'Stop' }).click();
  }
  return {
    browser: edge.version(),
    gpu: await gpuOf(page),
    viewport: await viewportOf(page),
    canvas: { width: canvas[0] ?? 0, height: canvas[1] ?? 0 },
    tracks: [first, second],
    runs: measured,
    tracedLongestTaskMs,
  };
}

async function stopEverything() {
  await edge?.close();
  await vite?.close();
  core?.kill();
  for (const app of [...started]) await quit(app);
  rmSync(connectionFile, { force: true });
}

declare global {
  interface Window {
    measureTableScroll: () => Promise<ScrollMeasurement>;
    measureWave: () => Promise<WaveMeasurement>;
    measureMix: () => Promise<MixMeasurement>;
  }
}

mkdirSync(bench, { recursive: true });
try {
  const date = new Date().toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'long',
    year: 'numeric',
  });
  console.log('Measuring the desktop app. Its window opens and closes several times.');
  const desktop = await measureDesktop();
  console.log('Measuring the Mix view in Edge. Both decks play for about a minute.');
  const mix = await measureMix();
  writeFileSync(output, JSON.stringify({ date, node: process.version, desktop, mix }, null, 2));
  console.log(`Wrote ${output}`);
} finally {
  await stopEverything();
}

import { type ChildProcess, spawn, spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, rmSync, statSync, watch } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const suffix = process.platform === 'win32' ? '.exe' : '';
const built = join(root, 'target', 'debug', `resonance${suffix}`);
const devFolder = join(root, 'target', 'dev');
// The core runs from a copy, so cargo can replace the built file while the old core keeps serving.
const serving = join(devFolder, `resonance${suffix}`);
const connectionFile = join(devFolder, 'connection.json');

let core: ChildProcess | null = null;
let servingBuiltAt = 0;

function build(): boolean {
  const cargo = spawnSync('cargo', ['build', '--package', 'resonance-cli'], {
    cwd: root,
    stdio: 'inherit',
  });
  return cargo.status === 0;
}

function stopped(): Promise<void> {
  const running = core;
  if (!running) return Promise.resolve();
  return new Promise((resolve) => {
    running.once('exit', () => resolve());
    running.kill();
  });
}

/** Builds the core and swaps it in. A failed build leaves the running core alone. */
async function refresh() {
  if (!build()) return;
  const builtAt = statSync(built).mtimeMs;
  if (core && builtAt === servingBuiltAt) return;
  await stopped();
  mkdirSync(devFolder, { recursive: true });
  copyFileSync(built, serving);
  servingBuiltAt = builtAt;
  const started = spawn(serving, ['serve', '--connection-file', connectionFile], {
    stdio: 'inherit',
  });
  started.once('exit', () => {
    if (core === started) core = null;
  });
  core = started;
}

let queue = Promise.resolve();
let settling: NodeJS.Timeout | undefined;

function changed() {
  clearTimeout(settling);
  settling = setTimeout(() => {
    queue = queue.then(refresh);
  }, 200);
}

watch(join(root, 'crates'), { recursive: true }, changed);
watch(join(root, 'Cargo.toml'), changed);
watch(join(root, 'Cargo.lock'), changed);

process.on('exit', () => {
  core?.kill();
  // A killed core cannot remove its own file, and a stale one would send the token to whatever
  // listens on that port next.
  rmSync(connectionFile, { force: true });
});
process.on('SIGINT', () => process.exit());
process.on('SIGTERM', () => process.exit());

queue = queue.then(refresh);

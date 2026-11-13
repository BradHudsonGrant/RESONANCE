import { type ChildProcess, spawn, spawnSync } from 'node:child_process';
import { existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as wait } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const e2e = join(root, 'target', 'e2e');
// The test dev server's proxy reads this file, so the page reaches the core started here and no
// other.
export const connectionFile = join(e2e, 'connection.json');

export const musicFolder = join(e2e, 'music');

function cargo(...args: string[]) {
  const done = spawnSync('cargo', args, { cwd: root, stdio: 'inherit' });
  if (done.status !== 0) throw new Error(`cargo ${args.join(' ')} failed`);
}

/** Writes the music folder, then starts a core on an empty library and waits until it listens. */
export default async function startCore(): Promise<() => void> {
  rmSync(e2e, { recursive: true, force: true });
  cargo(
    'run',
    '--quiet',
    '--package',
    'resonance-fixtures',
    '--bin',
    'fixture-folder',
    '--',
    musicFolder,
  );
  cargo('build', '--quiet', '--package', 'resonance-cli');

  const binary = join(
    root,
    'target',
    'debug',
    process.platform === 'win32' ? 'resonance.exe' : 'resonance',
  );
  const core: ChildProcess = spawn(
    binary,
    ['--data-folder', join(e2e, 'data'), 'serve', '--connection-file', connectionFile],
    { stdio: 'inherit' },
  );
  for (let tries = 0; !existsSync(connectionFile); tries++) {
    if (tries > 200 || core.exitCode !== null) throw new Error('The core did not start.');
    await wait(50);
  }

  return () => {
    core.kill();
    rmSync(connectionFile, { force: true });
  };
}

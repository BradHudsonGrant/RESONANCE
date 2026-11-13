import { spawnSync } from 'node:child_process';
import { rmSync, utimesSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
// Its own folder, so measuring never throws away the build the developer is working from.
const targetFolder = join(root, 'target', 'build-times');
const changedFile = join(root, 'crates', 'core', 'src', 'lib.rs');
const runs = Number(process.argv[2] ?? 3);

function cargo(...args: string[]) {
  const done = spawnSync('cargo', args, {
    cwd: root,
    stdio: ['ignore', 'ignore', 'inherit'],
    env: { ...process.env, CARGO_TARGET_DIR: targetFolder },
  });
  if (done.status !== 0) process.exit(done.status ?? 1);
}

function secondsToBuild(): number {
  const start = performance.now();
  cargo('build', '--workspace', '--quiet');
  return (performance.now() - start) / 1000;
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  const upper = sorted[middle] ?? 0;
  return sorted.length % 2 ? upper : ((sorted[middle - 1] ?? upper) + upper) / 2;
}

function line(label: string, values: number[]): string {
  const each = values.map((value) => value.toFixed(1)).join(', ');
  return `${label.padEnd(28)}${median(values).toFixed(1).padStart(6)} s   (${each})`;
}

cargo('fetch', '--quiet');

const clean: number[] = [];
const afterCoreChange: number[] = [];
for (let run = 0; run < runs; run++) {
  rmSync(targetFolder, { recursive: true, force: true });
  clean.push(secondsToBuild());
  const now = new Date();
  utimesSync(changedFile, now, now);
  afterCoreChange.push(secondsToBuild());
}
rmSync(targetFolder, { recursive: true, force: true });

console.log(line('Clean dev build', clean));
console.log(line('After a change in core', afterCoreChange));

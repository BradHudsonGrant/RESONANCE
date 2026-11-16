import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { foldersIn, machineFolders } from './machine-paths.ts';

const forbiddenPathPattern =
  /(?:^|\/)\.env(?:\.[^/]+)?$|\.(?:mp3|flac|wav|aif|aiff|m4a|aac|alac|ogg|opus|wma|mp4|mkv|mov|avi|db|db-wal|db-shm|sqlite|sqlite3|onnx|safetensors|pt|pth|ckpt|gguf|engine|exe|msi|dll|so|dylib|pem|key|pfx|p12)$/i;

const binaryAssetPattern = /\.(?:png|ico|woff2)$/i;

/** Tracked paths whose extension or name must never sit in git. */
export function forbiddenTrackedFiles(paths: string[]): string[] {
  return paths.filter((path) => forbiddenPathPattern.test(path));
}

/** Machine-specific folders (checkout root and user home) found in tracked file bytes. */
export function machinePathsInTracked(
  files: Iterable<[string, Buffer]>,
  folders: string[],
): Map<string, string[]> {
  const hits = new Map<string, string[]>();
  for (const [path, bytes] of files) {
    if (binaryAssetPattern.test(path)) continue;
    const found = [...foldersIn(bytes, folders).keys()];
    if (found.length > 0) hits.set(path, found);
  }
  return hits;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const repo = resolve(fileURLToPath(new URL('../../', import.meta.url)));
  const commonGitDir = execFileSync('git', ['rev-parse', '--git-common-dir'], {
    cwd: repo,
    encoding: 'utf8',
  }).trim();
  const commonRoot = dirname(resolve(repo, commonGitDir));
  const folders = [
    ...new Set([...machineFolders(repo, homedir()), ...machineFolders(commonRoot, homedir())]),
  ];

  const tracked = execFileSync('git', ['ls-files', '-z'], {
    cwd: repo,
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  })
    .split('\0')
    .filter(Boolean);

  const forbidden = forbiddenTrackedFiles(tracked);
  for (const path of forbidden) {
    console.error(`Tracked file is not allowed in the public repository: ${path}`);
  }

  const entries = function* (): Iterable<[string, Buffer]> {
    for (const path of tracked) {
      yield [path, readFileSync(join(repo, path))];
    }
  };
  const leaked = machinePathsInTracked(entries(), folders);
  for (const [path, matched] of leaked) {
    console.error(`${path} names machine path(s): ${matched.join(', ')}`);
  }

  if (forbidden.length > 0 || leaked.size > 0) {
    process.exitCode = 1;
  } else {
    console.log('Repository safety check passed: no forbidden files or machine paths tracked.');
  }
}

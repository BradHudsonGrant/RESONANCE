import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** A file that carries the app's version, and how to read and write it there. */
export interface VersionedFile {
  path: string;
  read: (text: string) => string | null;
  write: (text: string, version: string) => string;
}

const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

export function isVersion(text: string): boolean {
  return SEMVER.test(text);
}

const workspaceVersion = /(\[workspace\.package\][^[]*?\nversion = ")([^"]+)(")/;

function jsonVersion(text: string): string | null {
  const version: unknown = JSON.parse(text).version;
  return typeof version === 'string' ? version : null;
}

/** Replaces the first top-level `"version"` in a JSON file, keeping its layout as it was. */
function withJsonVersion(text: string, version: string): string {
  return text.replace(/^(\s{2}"version": ")[^"]+(")/m, `$1${version}$2`);
}

export const FILES: VersionedFile[] = [
  {
    path: 'Cargo.toml',
    read: (text) => workspaceVersion.exec(text)?.[2] ?? null,
    write: (text, version) => text.replace(workspaceVersion, `$1${version}$3`),
  },
  { path: 'app/tauri.conf.json', read: jsonVersion, write: withJsonVersion },
  { path: 'ui/package.json', read: jsonVersion, write: withJsonVersion },
  {
    path: 'ui/package-lock.json',
    read: (text) => {
      const lock = JSON.parse(text);
      return lock.version === lock.packages?.['']?.version ? jsonVersion(text) : null;
    },
    // The lock file states the version twice: at the top and for the root package.
    write: (text, version) =>
      text.replace(
        /^(\s{2}"version": ")[^"]+("[\s\S]*?\n\s{4}"": \{[\s\S]*?\n\s{6}"version": ")[^"]+(")/m,
        `$1${version}$2${version}$3`,
      ),
  },
];

/** The version each file states, or the reason the files disagree. */
export function agreedVersion(texts: Map<string, string>): { version: string } | { error: string } {
  const found = FILES.map((file) => ({
    path: file.path,
    version: file.read(texts.get(file.path) ?? ''),
  }));
  const missing = found.find((one) => !one.version);
  if (missing) return { error: `No version could be read from ${missing.path}.` };
  const versions = new Set(found.map((one) => one.version));
  if (versions.size > 1) {
    const listed = found.map((one) => `  ${one.path}: ${one.version}`).join('\n');
    return {
      error: `The version is out of step:\n${listed}\nSet it with \`just version <version>\`.`,
    };
  }
  return { version: found[0]!.version! };
}

/** A release tag is `v` and the version, and may add a pre-release such as `-rc.1`. */
export function tagMatches(tag: string, version: string): boolean {
  return tag === `v${version}` || (tag.startsWith(`v${version}-`) && isVersion(tag.slice(1)));
}

const root = fileURLToPath(new URL('../../', import.meta.url));

function readAll(): Map<string, string> {
  return new Map(FILES.map((file) => [file.path, readFileSync(join(root, file.path), 'utf8')]));
}

function main(args: string[]): number {
  const texts = readAll();
  const [first, second] = args;

  if (first === '--tag') {
    const agreed = agreedVersion(texts);
    if ('error' in agreed) return fail(agreed.error);
    if (!second || !tagMatches(second, agreed.version))
      return fail(`The tag ${second ?? '(none)'} does not match version ${agreed.version}.`);
    console.log(agreed.version);
    return 0;
  }

  if (first) {
    if (!isVersion(first)) return fail(`${first} is not a version. Use one such as 0.2.0.`);
    for (const file of FILES) {
      const before = texts.get(file.path) ?? '';
      const after = file.write(before, first);
      if (file.read(after) !== first) return fail(`The version in ${file.path} could not be set.`);
      writeFileSync(join(root, file.path), after);
    }
    // Cargo.lock repeats the version of every workspace crate.
    const cargo = spawnSync('cargo', ['update', '--workspace', '--quiet'], {
      cwd: root,
      stdio: 'inherit',
    });
    if (cargo.status !== 0) return fail('cargo could not bring Cargo.lock up to date.');
    console.log(`Version set to ${first}.`);
    return 0;
  }

  const agreed = agreedVersion(texts);
  if ('error' in agreed) return fail(agreed.error);
  console.log(`Version ${agreed.version} in every file.`);
  return 0;
}

function fail(message: string): number {
  console.error(message);
  return 1;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main(process.argv.slice(2));
}

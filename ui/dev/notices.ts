import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

interface CargoPackage {
  id: string;
  name: string;
  version: string;
  source: string | null;
  license: string | null;
  license_file: string | null;
  manifest_path: string;
}

interface CargoNode {
  id: string;
  deps: { pkg: string; dep_kinds: { kind: string | null }[] }[];
}

interface CargoMetadata {
  packages: CargoPackage[];
  resolve: { nodes: CargoNode[] } | null;
}

interface NpmPackage {
  name: string;
  version: string;
  license?: string;
}

interface BundledPackage {
  name: string;
  version: string;
  folder: string;
}

interface Notice {
  ecosystem: 'cargo' | 'npm';
  name: string;
  version: string;
  licence: string | null;
  source: string;
  folder: string;
  licenceFile?: string | null;
}

interface LicenceSource {
  ecosystem: Notice['ecosystem'];
  name: string;
  version: string;
  files: { path: string; url: string; sha256: string }[];
}

const root = fileURLToPath(new URL('../../', import.meta.url));
const ui = join(root, 'ui');
const output = join(root, 'app', 'THIRD-PARTY-NOTICES.txt');
const licences = join(ui, 'dev', 'licences');
const licenceName = /^(licen[cs]e|copying|copyright|notice|authors)(?:[._-]|$)/i;
const codeExtension = /\.(?:rs|[cm]?[jt]sx?|json|toml|py|[ch]|cpp)$/i;
const omittedFolders = new Set(['.git', 'node_modules', 'target']);

function runtimeCrates(): Notice[] {
  const text = execFileSync(
    'cargo',
    [
      'metadata',
      '--format-version',
      '1',
      '--locked',
      '--filter-platform',
      'x86_64-pc-windows-msvc',
      '--features',
      'resonance-app/custom-protocol',
    ],
    {
      cwd: root,
      encoding: 'utf8',
      maxBuffer: 32 * 1024 * 1024,
      stdio: ['ignore', 'pipe', 'inherit'],
      env: { ...process.env, CARGO_HOME: join(root, 'target', 'cargo-home') },
    },
  );
  const metadata = JSON.parse(text) as CargoMetadata;
  const app = metadata.packages.find((item) => item.name === 'resonance-app');
  if (!app || !metadata.resolve) throw new Error('Cargo did not resolve resonance-app.');
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const packages = new Map(metadata.packages.map((item) => [item.id, item]));
  const seen = new Set<string>();
  const pending = [app.id];
  const notices: Notice[] = [];
  while (pending.length) {
    const id = pending.pop();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    const node = nodes.get(id);
    const item = packages.get(id);
    if (!node || !item) throw new Error(`Cargo did not describe ${id}.`);
    for (const dependency of node.deps) {
      if (dependency.dep_kinds.some((kind) => kind.kind === null)) pending.push(dependency.pkg);
    }
    if (item.source) {
      notices.push({
        ecosystem: 'cargo',
        name: item.name,
        version: item.version,
        licence: item.license,
        source: `https://crates.io/crates/${item.name}/${item.version}`,
        folder: dirname(item.manifest_path),
        licenceFile: item.license_file,
      });
    }
  }
  return notices;
}

function frontendPackages(): Notice[] {
  const bundleFile = join(root, 'target', 'frontend-packages.json');
  if (!existsSync(bundleFile)) {
    throw new Error('Build the UI before collecting its notices: npm --prefix ui run build.');
  }
  const bundled = JSON.parse(readFileSync(bundleFile, 'utf8')) as BundledPackage[];
  const lock = JSON.parse(readFileSync(join(ui, 'package-lock.json'), 'utf8')) as {
    packages: Record<string, { version?: string }>;
  };
  return bundled.map((entry) => {
    const folder = resolve(root, entry.folder);
    const path = relative(ui, folder).replaceAll('\\', '/');
    if (!path.startsWith('node_modules/') || path.split('/').includes('..')) {
      throw new Error(`${entry.name}: a bundled package must be inside ui/node_modules.`);
    }
    const item = JSON.parse(readFileSync(join(folder, 'package.json'), 'utf8')) as NpmPackage;
    if (item.name !== entry.name || item.version !== entry.version) {
      throw new Error(`${entry.name}: the installed package changed. Build the UI again.`);
    }
    if (lock.packages[path]?.version !== entry.version) {
      throw new Error(`${item.name} does not match package-lock.json. Run npm ci --prefix ui.`);
    }
    return {
      ecosystem: 'npm',
      name: item.name,
      version: item.version,
      licence: item.license ?? null,
      source: `https://www.npmjs.com/package/${item.name}/v/${item.version}`,
      folder,
    };
  });
}

function licenceFiles(folder: string): string[] {
  const found: string[] = [];
  for (const entry of readdirSync(folder, { withFileTypes: true })) {
    const path = join(folder, entry.name);
    if (entry.isDirectory() && !omittedFolders.has(entry.name)) {
      found.push(...licenceFiles(path));
    } else if (entry.isFile() && licenceName.test(entry.name) && !codeExtension.test(entry.name)) {
      found.push(path);
    }
  }
  return found;
}

function upstreamTexts(item: Notice, sources: LicenceSource[]): string[] {
  const source = sources.find(
    (entry) =>
      entry.ecosystem === item.ecosystem &&
      entry.name === item.name &&
      entry.version === item.version,
  );
  if (!source) return [];
  return source.files.map((file) => {
    const path = resolve(licences, file.path);
    const within = relative(licences, path);
    if (within.startsWith(`..${sep}`) || isAbsolute(within)) {
      throw new Error(`${item.name}: the upstream licence must stay inside ui/dev/licences.`);
    }
    const bytes = readFileSync(path);
    if (createHash('sha256').update(bytes).digest('hex') !== file.sha256) {
      throw new Error(`${item.name}: the upstream licence checksum changed: ${file.path}.`);
    }
    return `${file.path}\nLicence source: ${file.url}\n\n${bytes.toString('utf8').replaceAll('\r\n', '\n').trimEnd()}`;
  });
}

function packageNotice(item: Notice, sources: LicenceSource[]): string {
  const files = new Set(licenceFiles(item.folder));
  if (item.licenceFile) files.add(join(item.folder, item.licenceFile));
  const texts = [...files].sort().map((path) => {
    const label = relative(item.folder, path).replaceAll('\\', '/');
    const text = readFileSync(path, 'utf8').replaceAll('\r\n', '\n').trimEnd();
    if (!text.trim()) throw new Error(`${item.name} ${item.version}: ${label} is empty.`);
    return `${label}\n\n${text}`;
  });
  texts.push(...upstreamTexts(item, sources));
  if (!texts.length)
    throw new Error(`${item.name} ${item.version}: no licence text in the package.`);
  return [
    `${item.name} ${item.version}`,
    `Source: ${item.source}`,
    `Licence: ${item.licence ?? 'See the licence text below.'}`,
    '',
    ...texts,
  ].join('\n');
}

function notices(): string {
  const sourceFile = join(licences, 'sources.json');
  const sources = existsSync(sourceFile)
    ? (JSON.parse(readFileSync(sourceFile, 'utf8')) as LicenceSource[])
    : [];
  const packages = [...runtimeCrates(), ...frontendPackages()].sort((left, right) => {
    const a = `${left.name}@${left.version}`;
    const b = `${right.name}@${right.version}`;
    return a < b ? -1 : a > b ? 1 : 0;
  });
  const failures: string[] = [];
  const sections: string[] = [];
  for (const item of packages) {
    try {
      sections.push(packageNotice(item, sources));
    } catch (error) {
      failures.push(error instanceof Error ? error.message : String(error));
    }
  }
  if (failures.length) throw new Error(failures.join('\n'));
  return [
    'Third-party notices for Resonance',
    '',
    'The Windows app includes the following third-party packages and fonts.',
    'Their licence and attribution texts follow.',
    '',
    sections.join('\n\n' + '='.repeat(72) + '\n\n'),
    '',
  ].join('\n');
}

try {
  const args = process.argv.slice(2);
  if (args.length > 1 || (args.length === 1 && args[0] !== '--check')) {
    throw new Error('Use node ui/dev/notices.ts [--check].');
  }
  const text = notices();
  if (args[0] === '--check') {
    if (!existsSync(output) || readFileSync(output, 'utf8') !== text) {
      throw new Error('Third-party notices are stale. Run node ui/dev/notices.ts.');
    }
    console.log('Third-party notices match the installed dependencies.');
  } else {
    writeFileSync(output, text);
    console.log('Wrote app/THIRD-PARTY-NOTICES.txt.');
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}

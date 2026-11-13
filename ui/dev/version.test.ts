import { describe, expect, it } from 'vitest';

import { FILES, agreedVersion, isVersion, tagMatches } from './version.ts';

const cargo = `[workspace]
members = ["crates/core"]

[workspace.package]
version = "0.1.0"
edition = "2024"

[workspace.dependencies]
clap = { version = "4.6.7" }
`;

const tauri = `{
  "productName": "Resonance",
  "version": "0.1.0",
  "bundle": { "version": "unrelated" }
}
`;

const pkg = `{
  "name": "resonance-ui",
  "version": "0.1.0",
  "devDependencies": { "vite": "^8.3.8" }
}
`;

const lock = `{
  "name": "resonance-ui",
  "version": "0.1.0",
  "lockfileVersion": 3,
  "packages": {
    "": {
      "name": "resonance-ui",
      "version": "0.1.0"
    },
    "node_modules/vite": {
      "version": "8.3.8"
    }
  }
}
`;

function texts(): Map<string, string> {
  return new Map([
    ['Cargo.toml', cargo],
    ['app/tauri.conf.json', tauri],
    ['ui/package.json', pkg],
    ['ui/package-lock.json', lock],
  ]);
}

function setAll(version: string): Map<string, string> {
  const all = texts();
  for (const file of FILES) all.set(file.path, file.write(all.get(file.path)!, version));
  return all;
}

describe('the version files', () => {
  it('agree when each states the same version', () => {
    expect(agreedVersion(texts())).toEqual({ version: '0.1.0' });
  });

  it('are all moved to a new version, and nothing else in them changes', () => {
    const moved = setAll('0.2.0');

    expect(agreedVersion(moved)).toEqual({ version: '0.2.0' });
    expect(moved.get('Cargo.toml')).toBe(cargo.replace('version = "0.1.0"', 'version = "0.2.0"'));
    expect(moved.get('Cargo.toml')).toContain('clap = { version = "4.6.7" }');
    expect(moved.get('app/tauri.conf.json')).toContain('"version": "unrelated"');
    expect(moved.get('ui/package-lock.json')).toContain('"version": "8.3.8"');
    expect(JSON.parse(moved.get('ui/package-lock.json')!).packages[''].version).toBe('0.2.0');
  });

  it('are reported out of step, naming each file and its version', () => {
    const one = texts();
    one.set('ui/package.json', pkg.replace('0.1.0', '0.1.1'));

    const result = agreedVersion(one);

    expect(result).toHaveProperty('error');
    expect('error' in result && result.error).toContain('ui/package.json: 0.1.1');
  });

  it('are out of step when the lock file disagrees with itself', () => {
    const one = texts();
    one.set(
      'ui/package-lock.json',
      lock.replace('"version": "0.1.0"\n    }', '"version": "0.0.9"\n    }'),
    );

    expect(agreedVersion(one)).toEqual({
      error: 'No version could be read from ui/package-lock.json.',
    });
  });
});

describe('versions and tags', () => {
  it('accepts SemVer and refuses anything else', () => {
    expect(isVersion('0.2.0')).toBe(true);
    expect(isVersion('1.0.0-rc.1')).toBe(true);
    expect(isVersion('v0.2.0')).toBe(false);
    expect(isVersion('0.2')).toBe(false);
  });

  it('matches a tag of the version, with or without a pre-release', () => {
    expect(tagMatches('v0.1.0', '0.1.0')).toBe(true);
    expect(tagMatches('v0.1.0-rc.1', '0.1.0')).toBe(true);
    expect(tagMatches('v0.1.1', '0.1.0')).toBe(false);
    expect(tagMatches('0.1.0', '0.1.0')).toBe(false);
    expect(tagMatches('v0.1.0x', '0.1.0')).toBe(false);
  });
});

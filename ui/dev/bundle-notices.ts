import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import type { Plugin } from 'vite';

/** Records what ships, so compiler and test dependencies do not enter the app's notices. */
export function bundleNotices(): Plugin {
  const root = fileURLToPath(new URL('../../', import.meta.url));
  return {
    name: 'bundled-package-notices',
    apply: 'build',
    generateBundle(_options, bundle) {
      const packages = new Map<string, { name: string; version: string; folder: string }>();
      const add = (folder: string) => {
        const { name, version } = JSON.parse(
          readFileSync(join(folder, 'package.json'), 'utf8'),
        ) as {
          name: string;
          version: string;
        };
        packages.set(`${name}@${version}`, { name, version, folder });
      };
      for (const output of Object.values(bundle)) {
        if (output.type !== 'chunk') continue;
        for (const id of Object.keys(output.modules)) {
          if (!id.replaceAll('\\', '/').includes('/node_modules/')) continue;
          let folder = dirname(id.split('?')[0] ?? id);
          while (!existsSync(join(folder, 'package.json'))) {
            const parent = dirname(folder);
            if (parent === folder) throw new Error(`No package owns the bundled module ${id}.`);
            folder = parent;
          }
          add(folder);
        }
      }
      // CSS emits the font files as assets, outside the JavaScript module graph.
      for (const font of ['inter', 'space-grotesk', 'jetbrains-mono']) {
        add(join(root, 'ui', 'node_modules', '@fontsource-variable', font));
      }
      mkdirSync(join(root, 'target'), { recursive: true });
      writeFileSync(
        join(root, 'target', 'frontend-packages.json'),
        JSON.stringify(
          [...packages.values()].sort((a, b) => a.name.localeCompare(b.name)),
          null,
          2,
        ) + '\n',
      );
    },
  };
}

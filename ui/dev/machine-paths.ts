import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';

/** The folders of this machine that must not appear in anything we ship. */
export function machineFolders(repo: string, home: string): string[] {
  const forms = (path: string) => {
    const trimmed = path.replace(/[\\/]+$/, '');
    return [trimmed.replaceAll('/', '\\'), trimmed.replaceAll('\\', '/')];
  };
  return [...new Set([...forms(repo), ...forms(home)])];
}

/** Each machine folder found in the bytes, as text, with how often it appears. */
export function foldersIn(bytes: Buffer, folders: string[]): Map<string, number> {
  const text = bytes.toString('latin1').toLowerCase();
  const found = new Map<string, number>();
  for (const folder of folders) {
    const count = text.split(folder.toLowerCase()).length - 1;
    if (count > 0) found.set(folder, count);
  }
  return found;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const repo = fileURLToPath(new URL('../../', import.meta.url));
  const folders = machineFolders(repo, homedir());
  let clean = true;
  for (const file of process.argv.slice(2)) {
    for (const [folder, count] of foldersIn(readFileSync(file), folders)) {
      console.error(`${file} names ${folder} ${count} times.`);
      clean = false;
    }
  }
  if (!clean) {
    console.error('A shipped file must not name the folders of the machine that built it.');
    process.exitCode = 1;
  } else {
    console.log('No machine folders in the shipped files.');
  }
}

import { describe, expect, it } from 'vitest';

import { machineFolders } from './machine-paths.ts';
import { forbiddenTrackedFiles, machinePathsInTracked } from './repo-safety.ts';

describe('repo safety', () => {
  it('flags audio, databases, model weights, binaries and secrets while allowing source and docs', () => {
    const tracked = [
      'README.md',
      'crates/core/src/lib.rs',
      'crates/db/migrations/0001_library.sql',
      'docs/images/library.png',
      'fixtures/club-track.mp3',
      'sandbox/master.db',
      'models/beat-this.onnx',
      'app/.env.local',
      'certs/signing.pfx',
    ];

    expect(forbiddenTrackedFiles(tracked)).toEqual([
      'fixtures/club-track.mp3',
      'sandbox/master.db',
      'models/beat-this.onnx',
      'app/.env.local',
      'certs/signing.pfx',
    ]);
  });

  it('flags machine user and checkout folders in tracked text files and skips binary images', () => {
    const folders = machineFolders('D:\\Work\\RESONANCE', 'C:\\Users\\someone');
    const files: [string, Buffer][] = [
      ['docs/clean.md', Buffer.from('Measured against fixtures-local.\n', 'utf8')],
      ['docs/leaked.md', Buffer.from('Scanned C:\\Users\\someone\\Music\\Set.\n', 'utf8')],
      ['docs/images/shot.png', Buffer.from('C:\\Users\\someone', 'utf8')],
    ];

    expect(machinePathsInTracked(files, folders)).toEqual(
      new Map([['docs/leaked.md', ['C:\\Users\\someone']]]),
    );
  });
});

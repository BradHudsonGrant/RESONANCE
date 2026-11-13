import { describe, expect, it } from 'vitest';

import { foldersIn, machineFolders } from './machine-paths.ts';

describe('machine folders', () => {
  const folders = machineFolders('D:\\Work\\RESONANCE\\', 'C:\\Users\\someone');

  it('are looked for with either slash', () => {
    expect(folders).toEqual([
      'D:\\Work\\RESONANCE',
      'D:/Work/RESONANCE',
      'C:\\Users\\someone',
      'C:/Users/someone',
    ]);
  });

  it('are found and counted in a binary, whatever their case', () => {
    const bytes = Buffer.from(
      '\0panic at d:\\work\\resonance\\target\\cargo-home\\registry\0C:/Users/someone/x\0' +
        'D:\\Work\\RESONANCE\\crates\0',
      'latin1',
    );

    expect(foldersIn(bytes, folders)).toEqual(
      new Map([
        ['D:\\Work\\RESONANCE', 2],
        ['C:/Users/someone', 1],
      ]),
    );
  });

  it('are not reported in a binary whose paths were remapped', () => {
    const bytes = Buffer.from('\0cargo\\registry\\src\\tokio\0resonance\\crates\\api\0', 'latin1');

    expect(foldersIn(bytes, folders).size).toBe(0);
  });
});

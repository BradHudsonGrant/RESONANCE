// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { ScanReport } from '../ipc/bindings/ScanReport';
import { CoreError } from '../ipc/client';
import ScanFolder from './ScanFolder.svelte';

const report: ScanReport = {
  source_folder: 'D:\\Music\\RESONANCE\\fixtures-local',
  found: 50,
  added: 42,
  changed: 3,
  unchanged: 5,
  unreadable_count: 0,
  unreadable: [],
};

describe('ScanFolder', () => {
  it('requires a folder before offering to scan', async () => {
    const scan = vi.fn<(folder: string) => Promise<ScanReport>>();
    render(ScanFolder, { open: true, scan, onscanned: vi.fn() });
    const submit = screen.getByRole('button', { name: 'Scan folder' }) as HTMLButtonElement;

    expect(submit.disabled).toBe(true);
    await fireEvent.input(screen.getByRole('textbox', { name: 'Music folder' }), {
      target: { value: '   ' },
    });

    expect(submit.disabled).toBe(true);
    expect(scan).not.toHaveBeenCalled();
  });

  it('preserves the folder path and submits it once while a scan is pending', async () => {
    let finish: (report: ScanReport) => void = () => {};
    const scan = vi.fn<(folder: string) => Promise<ScanReport>>(
      () =>
        new Promise<ScanReport>((resolve) => {
          finish = resolve;
        }),
    );
    const onscanned = vi.fn();
    render(ScanFolder, { open: true, scan, onscanned });
    const folder = screen.getByRole('textbox', { name: 'Music folder' }) as HTMLInputElement;
    await fireEvent.input(folder, { target: { value: `  ${report.source_folder}  ` } });

    await fireEvent.submit(folder.form!);
    await fireEvent.submit(folder.form!);

    expect(scan).toHaveBeenCalledExactlyOnceWith(`  ${report.source_folder}  `);
    expect(folder.disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Scanning…' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(screen.getByRole('status').textContent).toContain('Reading the folder');
    expect(onscanned).not.toHaveBeenCalled();

    finish(report);

    await waitFor(() => expect(onscanned).toHaveBeenCalledOnce());
    expect(screen.getByRole('status').textContent).toMatch(
      /50 files: 42 added, 3 changed,\s+5 unchanged/,
    );
    expect(folder.disabled).toBe(false);
    expect(
      (screen.getByRole('button', { name: 'Scan folder' }) as HTMLButtonElement).disabled,
    ).toBe(false);
  });

  it('keeps a failed path available to correct and clears the error on retry', async () => {
    const message = 'The folder could not be read. Check its path and try again.';
    const scan = vi
      .fn<(folder: string) => Promise<ScanReport>>()
      .mockRejectedValueOnce(new CoreError('scan_failed', message))
      .mockResolvedValueOnce(report);
    const onscanned = vi.fn();
    render(ScanFolder, { open: true, scan, onscanned });
    const folder = screen.getByRole('textbox', { name: 'Music folder' }) as HTMLInputElement;
    await fireEvent.input(folder, { target: { value: 'missing' } });

    await fireEvent.submit(folder.form!);

    expect((await screen.findByRole('alert')).textContent).toBe(message);
    expect(folder.value).toBe('missing');
    expect(folder.disabled).toBe(false);
    expect(onscanned).not.toHaveBeenCalled();

    await fireEvent.input(folder, { target: { value: report.source_folder } });
    await fireEvent.submit(folder.form!);

    await waitFor(() => expect(onscanned).toHaveBeenCalledOnce());
    expect(scan).toHaveBeenLastCalledWith(report.source_folder);
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('reports unreadable files alongside the tracks added and identifies a shortened problem list', async () => {
    const scan = vi.fn<(folder: string) => Promise<ScanReport>>().mockResolvedValue({
      ...report,
      unreadable_count: 103,
      unreadable: [{ path: 'broken.flac', reason: 'The audio header could not be read.' }],
    });
    const onscanned = vi.fn();
    render(ScanFolder, { open: true, scan, onscanned });
    const folder = screen.getByRole('textbox', { name: 'Music folder' }) as HTMLInputElement;
    await fireEvent.input(folder, { target: { value: report.source_folder } });

    await fireEvent.submit(folder.form!);

    await waitFor(() => expect(onscanned).toHaveBeenCalledOnce());
    const status = screen.getByRole('status');
    expect(status.textContent).toContain('42 added');
    expect(status.textContent).toContain('103 files or folders could not be read');
    expect(screen.getByRole('listitem').textContent).toBe(
      'broken.flac: The audio header could not be read.',
    );
    expect(status.textContent).toContain('Showing the first 1 problems.');
  });
});

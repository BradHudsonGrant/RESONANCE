// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import { fakeTrack } from '../library/fake-library';
import type { PrepItem } from '../ipc/bindings/PrepItem';
import type { StagedField } from '../ipc/bindings/StagedField';
import type { TagName } from '../ipc/bindings/TagName';
import Prep, { type PrepClient } from './Prep.svelte';

HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

const STAGED_ORDER: TagName[] = [
  'title',
  'artist',
  'mix',
  'album',
  'genre',
  'subgenre',
  'label',
  'catno',
  'year',
  'bpm',
  'key',
  'track_number',
  'comment',
];

function makeFields(overrides: Partial<Record<TagName, Partial<StagedField>>>): StagedField[] {
  return STAGED_ORDER.map((field) => {
    const custom = overrides[field] ?? {};
    return {
      field,
      current_value: custom.current_value ?? null,
      current_provenance: custom.current_provenance ?? null,
      proposed_value: custom.proposed_value ?? null,
      provider: custom.provider ?? null,
      confidence: custom.confidence ?? null,
      audio_matched: custom.audio_matched ?? false,
      status: custom.status ?? null,
      locked_by_user: custom.locked_by_user ?? false,
      changes: custom.changes ?? false,
    };
  });
}

function makePrepItem(index: number, confidence = 0.96, audioMatched = true): PrepItem {
  const track = fakeTrack(index);
  const fields = makeFields({
    title: {
      current_value: null,
      proposed_value: `Track ${index + 1}`,
      provider: 'musicbrainz',
      confidence,
      audio_matched: audioMatched,
      status: 'pending',
      changes: true,
    },
    artist: {
      current_value: null,
      proposed_value: 'Kaden',
      provider: 'musicbrainz',
      confidence,
      audio_matched: audioMatched,
      status: 'pending',
      changes: true,
    },
    label: {
      current_value: null,
      proposed_value: 'Vector Recordings',
      provider: 'musicbrainz',
      confidence,
      audio_matched: audioMatched,
      status: 'pending',
      changes: true,
    },
  });
  return {
    track,
    fields,
    artwork: { track_id: track.id, artwork: null, proposed: null },
    pending_fields: 3,
    provider: 'musicbrainz',
    confidence,
    audio_matched: audioMatched,
    needs_url: false,
    changeable_confidences: [confidence, confidence, confidence],
    release: {
      key: 'release:vec001:horizon ep',
      title: 'Horizon EP',
      subtitle: 'Vector Recordings · VEC001 · 2024',
    },
  };
}

function makeClient(initialItems: PrepItem[]): PrepClient {
  let items = [...initialItems];
  return {
    queue: vi.fn(async () => ({
      items,
      unwritten_tracks: 2,
      last_operation: {
        id: 7,
        kind: 'write_tags' as const,
        state: 'applied' as const,
        summary: 'Wrote tags to 2 files',
        can_undo: true,
        created_ms: 1000,
        applied_ms: 1010,
        undone_ms: null,
      },
    })),
    diff: vi.fn(async (trackId: number) => ({
      track_id: trackId,
      fields: items.find((i) => i.track.id === trackId)?.fields ?? makeFields({}),
    })),
    artwork: vi.fn(async (trackId: number) => ({
      track_id: trackId,
      artwork: null,
      proposed: null,
    })),
    acceptItems: vi.fn(async (trackIds: number[]) => {
      items = items.filter((item) => !trackIds.includes(item.track.id));
      return { tracks: trackIds.length, fields: trackIds.length * 3 };
    }),
    skipItems: vi.fn(async (trackIds: number[]) => {
      items = items.filter((item) => !trackIds.includes(item.track.id));
      return { tracks: trackIds.length, fields: trackIds.length * 3 };
    }),
    acceptAboveThreshold: vi.fn(async (minConfidence: number) => {
      const accepted = items.filter((item) => item.confidence >= minConfidence);
      items = items.filter((item) => item.confidence < minConfidence);
      return { tracks: accepted.length, fields: accepted.length * 3 };
    }),
    setUserTag: vi.fn(async (trackIds: number[]) => ({ tracks: trackIds.length })),
    acceptArtwork: vi.fn(async (trackId: number) => ({
      track_id: trackId,
      artwork: 'abc',
      proposed: null,
    })),
    skipArtwork: vi.fn(async (trackId: number) => ({
      track_id: trackId,
      artwork: null,
      proposed: null,
    })),
    identifyOnline: vi.fn(async () => ({
      active: [],
      recent: [],
      mix_backoff_active: false,
    })),
    identifyWithUrl: vi.fn(async (trackId: number) => ({
      track_id: trackId,
      best: null,
      needs_url: false,
      steps: [],
    })),
    previewTagWrites: vi.fn(async () => [
      {
        track_id: 1,
        path: 'Music/01 - Kaden - Track 1.wav',
        fields: [{ field: 'title' as const, from: null, to: 'Track 1' }],
        cover: false,
        blocked: null,
      },
    ]),
    applyTagWrites: vi.fn(async () => ({
      operation_id: 8,
      written: 1,
      failed: [],
    })),
    previewBatchTag: vi.fn(async () => ({ tracks: 2, fields: 2 })),
    applyBatchTag: vi.fn(async () => ({ tracks: 2, fields: 2 })),
    undoOperation: vi.fn(async () => ({
      operation: {
        id: 7,
        kind: 'write_tags' as const,
        state: 'undone' as const,
        summary: 'Wrote tags to 2 files',
        can_undo: false,
        created_ms: 1000,
        applied_ms: 1010,
        undone_ms: 1020,
      },
      restored: 2,
    })),
  };
}

describe('Prep space', () => {
  it('shows the queue, diff, audio match badge and threshold counts, and runs the keyboard loop', async () => {
    const client = makeClient([
      makePrepItem(0, 0.96, true),
      makePrepItem(1, 0.94, false),
      makePrepItem(2, 0.78, false),
    ]);
    render(Prep, { client });

    await screen.findByRole('listbox', { name: 'Prep queue' });
    expect(screen.getByText('Audio matches')).toBeTruthy();
    expect(
      screen.getByRole('button', { name: /Accept above 90% \(2 tracks · 6 fields\)/ }),
    ).toBeTruthy();

    // Edit inline with E, type a value, and save with Enter
    await fireEvent.keyDown(window, { key: 'e' });
    const editInput = screen.getByRole('textbox', { name: 'Edit Title' }) as HTMLInputElement;
    await fireEvent.input(editInput, { target: { value: 'Echo Chamber VIP' } });
    await fireEvent.submit(editInput.closest('form')!);
    expect(client.setUserTag).toHaveBeenCalledWith(
      [1],
      [{ field: 'title', value: 'Echo Chamber VIP' }],
    );

    // Press Enter to accept the first item and move to the second
    await fireEvent.keyDown(window, { key: 'Enter' });
    expect(client.acceptItems).toHaveBeenCalledWith([1], []);

    // Press S to skip the current item
    await fireEvent.keyDown(window, { key: 's' });
    expect(client.skipItems).toHaveBeenCalled();

    // Undo with Ctrl+Z
    await fireEvent.keyDown(window, { key: 'z', ctrlKey: true });
    expect(client.undoOperation).toHaveBeenCalledWith(7);
  });

  it('groups by release with G and accepts the whole release on Enter', async () => {
    const client = makeClient([makePrepItem(0, 0.96, true), makePrepItem(1, 0.94, true)]);
    render(Prep, { client });

    await screen.findByRole('listbox', { name: 'Prep queue' });
    await fireEvent.keyDown(window, { key: 'g' });

    expect(screen.getByRole('heading', { name: 'Horizon EP' })).toBeTruthy();
    await fireEvent.keyDown(window, { key: 'Enter' });
    expect(client.acceptItems).toHaveBeenCalledWith([1, 2], []);
  });

  it('opens Batch tools with B and Write tags with W from the keyboard', async () => {
    const client = makeClient([makePrepItem(0, 0.96, true)]);
    render(Prep, { client });

    await screen.findByRole('listbox', { name: 'Prep queue' });

    await fireEvent.keyDown(window, { key: 'b' });
    const batchDialog = await screen.findByRole('dialog', { name: 'Batch tag tools' });
    expect(within(batchDialog).getByText(/Will update 2 tracks/)).toBeTruthy();
    await fireEvent.click(within(batchDialog).getByRole('button', { name: 'Cancel' }));

    await fireEvent.keyDown(window, { key: 'w' });
    const writeDialog = await screen.findByRole('dialog', { name: 'Write tags to files' });
    const writeBtn = await within(writeDialog).findByRole('button', {
      name: 'Write tags to 1 file',
    });
    const writeForm = writeDialog.querySelector('form');
    if (!writeForm) throw new Error('write-tags-form not found');
    expect(writeBtn).toBeTruthy();
    await fireEvent.submit(writeForm);
    expect(client.applyTagWrites).toHaveBeenCalled();
  });
});

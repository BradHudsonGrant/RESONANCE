// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import { fakeTrack } from '../library/fake-library';
import type { ActivityJob } from '../ipc/bindings/ActivityJob';
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
      dirty_on_disk: custom.dirty_on_disk ?? false,
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
    review_token: 'review-' + track.id,
    fields,
    artwork: { track_id: track.id, artwork: null, proposed: null, dirty_on_disk: false },
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
    evidence: {
      basis: audioMatched ? 'audio_fingerprint' : 'tags_and_duration',
      providers: ['musicbrainz'],
      primary_provider: 'musicbrainz',
      duration_delta_ms: 400,
      preview_similarity: null,
      catno_matched: false,
      isrc_matched: false,
      url: 'https://musicbrainz.org/release/rel-1',
    },
    candidates: [],
    selected_candidate: 0,
    candidates_expanded: false,
    steps: [],
    has_unwritten: false,
    online_checked: true,
  };
}

function makeClient(items: PrepItem[]): PrepClient {
  return {
    trackIds: vi.fn(async () => items.map((item) => item.track.id)),
    prepPage: vi.fn(async (_query, ids, offset, limit) => {
      const scoped = ids ? items.filter((item) => ids.includes(item.track.id)) : items;
      return {
        total: scoped.length,
        offset,
        items: scoped.slice(offset, offset + limit),
        unwritten_tracks: 0,
        last_operation: null,
      };
    }),
    selectCandidate: vi.fn(async (id) => items.find((item) => item.track.id === id)!),
    useCandidateArtwork: vi.fn(async (id) => items.find((item) => item.track.id === id)!),
    acceptAndWrite: vi.fn(async (ids) => ({
      written: ids.length,
      library_fields: 0,
      reverted_fields: 0,
      operation_id: 45,
      failed: [],
    })),
    identifyOnline: vi.fn(async () => ({ active: [], recent: [], mix_backoff_active: false })),
    identifyWithUrl: vi.fn(async (id) => ({
      track_id: id,
      steps: [],
      best: null,
      needs_url: false,
    })),
    previewBatchTag: vi.fn(async () => ({ tracks: 0, fields: 0 })),
    applyBatchTag: vi.fn(async () => ({ tracks: 0, fields: 0 })),
    previewTagWrites: vi.fn(async () => []),
    applyTagWrites: vi.fn(async () => ({ written: 0, operation_id: null, failed: [] })),
    undoOperation: vi.fn(async (id) => ({
      restored: 1,
      operation: {
        id,
        kind: 'write_tags' as const,
        state: 'undone' as const,
        summary: 'Restored file',
        can_undo: false,
        created_ms: 0,
        applied_ms: 1,
        undone_ms: 2,
      },
    })),
  };
}

describe('Prep', () => {
  it('lists tracks before a search and limits search to checked tracks', async () => {
    const items = [makePrepItem(0), makePrepItem(1)];
    const client = makeClient(items);
    render(Prep, { client });
    await screen.findByRole('button', { name: 'Accept all' });
    const search = screen.getByRole('button', { name: 'Search online' }) as HTMLButtonElement;
    expect(search.disabled).toBe(true);
    expect(screen.queryByRole('slider')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Open release' })).toBeNull();
    const id = items[1]!.track.id;
    await fireEvent.click(
      screen.getByRole('checkbox', { name: 'Select ' + items[1]!.track.title }),
    );
    await fireEvent.click(search);
    expect(client.identifyOnline).toHaveBeenCalledWith([id], true);
  });

  it('retains Library filters when filtering for missing artwork and clears old selection', async () => {
    const client = makeClient([makePrepItem(0)]);
    const linked = { text: 'crate' };
    render(Prep, { client, batchQuery: linked, batchTrackIds: [1, 2] });
    await fireEvent.click(await screen.findByRole('checkbox', { name: 'Select all tracks' }));
    await fireEvent.change(screen.getByRole('combobox', { name: 'Filter tracks' }), {
      target: { value: 'artwork' },
    });
    await waitFor(() =>
      expect(client.prepPage).toHaveBeenCalledWith(
        {
          text: 'crate',
          filters: [{ any_of: [{ column: 'artwork', condition: { op: 'missing' } }] }],
        },
        [1, 2],
        0,
        50,
      ),
    );
    expect(
      (screen.getByRole('button', { name: 'Search online' }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it('selects every filtered track across pages and respects individual exclusions', async () => {
    const items = Array.from({ length: 505 }, (_, i) => makePrepItem(i));
    const ids = items.map((item) => item.track.id);
    const client = makeClient(items);
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('checkbox', { name: 'Select all tracks' }));
    await waitFor(() => expect(client.trackIds).toHaveBeenCalledOnce());
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    await fireEvent.click(
      await screen.findByRole('checkbox', { name: 'Select ' + items[50]!.track.title }),
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Accept selected' }));
    await waitFor(() =>
      expect(client.acceptAndWrite).toHaveBeenCalledWith(
        ids.filter((id) => id !== items[50]!.track.id),
        null,
        true,
        undefined,
      ),
    );
    await fireEvent.click(await screen.findByRole('button', { name: 'Undo save' }));
    expect(client.undoOperation).toHaveBeenCalledWith(45);
  });

  it('saves just the accepted field and offers undo beside that field', async () => {
    const item = makePrepItem(0);
    const client = makeClient([item]);
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept Artist' }));
    expect(client.acceptAndWrite).toHaveBeenCalledWith(
      [item.track.id],
      ['artist'],
      false,
      item.review_token,
    );
    const undo = await screen.findByRole('button', { name: 'Undo save' });
    expect(undo.closest('tr')?.textContent).toContain('Artist');
    await fireEvent.click(undo);
    expect(client.undoOperation).toHaveBeenCalledWith(45);
  });

  it('exposes failed saves without offering an undo for an unwritten file', async () => {
    const item = makePrepItem(0);
    const client = makeClient([item]);
    vi.mocked(client.acceptAndWrite).mockResolvedValue({
      written: 0,
      library_fields: 0,
      reverted_fields: 0,
      operation_id: null,
      failed: [{ track_id: item.track.id, path: 'example.wav', reason: 'File changed since scan' }],
    });
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept all' }));
    expect(client.acceptAndWrite).toHaveBeenCalledWith(
      [item.track.id],
      ['title', 'artist', 'label'],
      false,
      item.review_token,
    );
    await screen.findByText('Save failed');
    expect(screen.queryByRole('button', { name: 'Undo save' })).toBeNull();
  });

  it('restores keyboard focus when the next track is on another page', async () => {
    const items = Array.from({ length: 51 }, (_, index) => makePrepItem(index));
    const client = makeClient(items);
    render(Prep, { client });
    const last = await screen.findByRole('button', { name: /^Track 49 Ada Marsh/ });
    await fireEvent.click(last);
    last.focus();
    await fireEvent.keyDown(last, { key: 'ArrowDown' });
    const next = await screen.findByRole('button', { name: /^Track 50 Ada Marsh/ });
    await waitFor(() => expect(document.activeElement).toBe(next));
  });

  it('accepts and undoes a library-only field without reporting a file write', async () => {
    const item = makePrepItem(0);
    item.fields = makeFields({ mix: { changes: true, proposed_value: 'Dub', status: 'pending' } });
    const client = makeClient([item]);
    vi.mocked(client.acceptAndWrite).mockResolvedValue({
      written: 0,
      library_fields: 1,
      reverted_fields: 0,
      operation_id: 68,
      failed: [],
    });
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept Mix' }));
    expect(client.acceptAndWrite).toHaveBeenCalledWith(
      [item.track.id],
      ['mix'],
      false,
      item.review_token,
    );
    await screen.findByText('1 field saved to library');
    await fireEvent.click(screen.getByRole('button', { name: 'Undo save' }));
    expect(client.undoOperation).toHaveBeenCalledWith(68);
  });

  it('keeps the selected batch fixed when another track arrives', async () => {
    const first = makePrepItem(0);
    const second = makePrepItem(1);
    const client = makeClient([first, second]);
    vi.mocked(client.prepPage)
      .mockResolvedValueOnce({
        total: 1,
        offset: 0,
        items: [first],
        unwritten_tracks: 0,
        last_operation: null,
      })
      .mockResolvedValue({
        total: 2,
        offset: 0,
        items: [first, second],
        unwritten_tracks: 0,
        last_operation: null,
      });
    const view = render(Prep, { client });
    await fireEvent.click(await screen.findByRole('checkbox', { name: 'Select all tracks' }));
    await view.rerender({ client, revision: 1 });
    await screen.findByRole('checkbox', { name: 'Select ' + second.track.title });
    await fireEvent.click(screen.getByRole('button', { name: 'Search online' }));
    expect(client.identifyOnline).toHaveBeenCalledWith([first.track.id], true);
  });

  it('refreshes the presented suggestions when a reviewed value has changed', async () => {
    const item = makePrepItem(0);
    const changed = {
      ...item,
      review_token: 'different-review',
      fields: makeFields({
        artist: { changes: true, proposed_value: 'Updated artist', status: 'pending' },
      }),
    };
    const client = makeClient([item]);
    vi.mocked(client.prepPage)
      .mockResolvedValueOnce({
        total: 1,
        offset: 0,
        items: [item],
        unwritten_tracks: 0,
        last_operation: null,
      })
      .mockResolvedValue({
        total: 1,
        offset: 0,
        items: [changed],
        unwritten_tracks: 0,
        last_operation: null,
      });
    vi.mocked(client.acceptAndWrite).mockRejectedValueOnce(new Error('Suggestions have changed'));
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept Artist' }));
    await screen.findByText('Updated artist');
    expect(screen.queryByRole('button', { name: 'Undo save' })).toBeNull();
    expect(client.acceptAndWrite).toHaveBeenCalledOnce();
    expect(client.acceptAndWrite).toHaveBeenCalledWith(
      [item.track.id],
      ['artist'],
      false,
      item.review_token,
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Accept Artist' }));
    expect(client.acceptAndWrite).toHaveBeenLastCalledWith(
      [item.track.id],
      ['artist'],
      false,
      changed.review_token,
    );
  });

  it('passes explicit artwork flag on single-track Accept all and null fields on bulk Accept selected', async () => {
    const item = makePrepItem(0);
    item.artwork = {
      track_id: item.track.id,
      artwork: 'existing-hash',
      proposed: { artwork: 'proposed-hash', provider: 'discogs' },
      dirty_on_disk: false,
    };
    const client = makeClient([item]);
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept all' }));
    expect(client.acceptAndWrite).toHaveBeenLastCalledWith(
      [item.track.id],
      ['title', 'artist', 'label'],
      true,
      item.review_token,
    );
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Select ' + item.track.title }));
    await fireEvent.click(screen.getByRole('button', { name: 'Accept selected' }));
    expect(client.acceptAndWrite).toHaveBeenLastCalledWith([item.track.id], null, true, undefined);
  });

  it('keeps Undo in the review header in compact detail view when the saved track leaves a filter', async () => {
    const first = makePrepItem(0);
    const second = makePrepItem(1);
    const client = makeClient([first, second]);
    const observers: ResizeObserverCallback[] = [];
    const origObserver = globalThis.ResizeObserver;
    const origWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
    globalThis.ResizeObserver = class {
      constructor(cb: ResizeObserverCallback) {
        observers.push(cb);
      }
      observe() {}
      disconnect() {}
      unobserve() {}
    } as unknown as typeof ResizeObserver;
    Object.defineProperty(HTMLElement.prototype, 'clientWidth', {
      configurable: true,
      get() {
        return 640;
      },
    });
    vi.mocked(client.prepPage)
      .mockResolvedValueOnce({
        total: 2,
        offset: 0,
        items: [first, second],
        unwritten_tracks: 0,
        last_operation: null,
      })
      .mockResolvedValue({
        total: 1,
        offset: 0,
        items: [second],
        unwritten_tracks: 0,
        last_operation: null,
      });

    try {
      const { container } = render(Prep, { client });
      for (const cb of observers) cb([], {} as ResizeObserver);
      await fireEvent.click(await screen.findByRole('button', { name: /^Track 0 Ada Marsh/ }));
      expect(container.querySelector('.workspace')?.classList.contains('detail-open')).toBe(true);
      await fireEvent.click(await screen.findByRole('button', { name: 'Accept Artist' }));
      await screen.findByRole('heading', { name: second.track.title });
      const undo = await screen.findByRole('button', { name: 'Undo save' });
      expect(undo.closest('.review-head')).not.toBeNull();
    } finally {
      globalThis.ResizeObserver = origObserver;
      if (origWidth) Object.defineProperty(HTMLElement.prototype, 'clientWidth', origWidth);
      else delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
    }
  });

  it('updates stale manual-edits notice after WriteTagsSheet writes and passes focused track scope', async () => {
    const item = makePrepItem(0);
    const client = makeClient([item]);
    const onrename = vi.fn();
    vi.mocked(client.previewTagWrites).mockResolvedValue([
      {
        track_id: item.track.id,
        path: 'Music/track0.flac',
        fields: [{ field: 'title', from: 'Old', to: 'New' }],
        cover: false,
        blocked: null,
      },
    ]);
    vi.mocked(client.applyTagWrites).mockResolvedValue({
      written: 1,
      operation_id: 91,
      failed: [],
    });
    render(Prep, { client, onrename });
    const summary = await screen.findByText('Track tools');
    await fireEvent.click(summary);
    await fireEvent.click(screen.getByRole('button', { name: 'Edit tags' }));
    await screen.findByText(`Focused track: ${item.track.title}`);
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    await fireEvent.click(screen.getByRole('button', { name: 'Write manual edits' }));
    await screen.findByText(`Focused track: ${item.track.title}`);
    await fireEvent.click(await screen.findByRole('button', { name: 'Write tags to 1 file' }));
    await screen.findByText('Wrote tags to 1 file');
    expect(screen.queryByText('Manual edits are ready to write')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Rename files' }));
    expect(onrename).toHaveBeenCalledWith(
      { kind: 'tracks', track_ids: [item.track.id] },
      `Focused track: ${item.track.title}`,
    );
  });

  it('keeps Track tools open and reports feedback when Find match finds no release', async () => {
    const item = makePrepItem(0);
    const client = makeClient([item]);
    let resolveLookup!: (value: Awaited<ReturnType<PrepClient['identifyWithUrl']>>) => void;
    vi.mocked(client.identifyWithUrl).mockReturnValue(
      new Promise((resolve) => {
        resolveLookup = resolve;
      }),
    );
    render(Prep, { client });
    await fireEvent.click(await screen.findByText('Track tools'));
    const input = screen.getByLabelText('Release link');
    await fireEvent.input(input, { target: { value: 'https://discogs.com/release/99' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Find match' }));
    await screen.findByRole('button', { name: 'Searching…' });
    resolveLookup({
      track_id: item.track.id,
      best: null,
      needs_url: true,
      steps: [],
    });
    await screen.findByText('No match found at that release link');
    expect((screen.getByLabelText('Release link') as HTMLInputElement).value).toBe(
      'https://discogs.com/release/99',
    );
  });

  it('displays cover provider when sources differ and keeps selected cover option', async () => {
    const item = makePrepItem(0);
    item.artwork = {
      track_id: item.track.id,
      artwork: null,
      proposed: { artwork: 'art-hash-2', provider: 'Discogs' },
      dirty_on_disk: false,
    };
    item.selected_candidate = 5;
    item.candidates = [
      {
        index: 2,
        provider: 'musicbrainz',
        providers: ['musicbrainz'],
        confidence: 0.96,
        audio_matched: true,
        artist: 'Kaden',
        title: 'Track 1',
        mix: null,
        album: 'Horizon EP',
        label: 'Vector',
        catno: null,
        year: 2024,
        genre: null,
        subgenre: null,
        track_number: null,
        duration_delta_ms: null,
        url: null,
        has_artwork: true,
        artwork_provider: 'MusicBrainz',
        artwork_hash: 'art-hash-1',
        evidence: item.evidence!,
      },
      {
        index: 5,
        provider: 'discogs',
        providers: ['discogs'],
        confidence: 0.91,
        audio_matched: false,
        artist: 'Kaden',
        title: 'Track 1',
        mix: null,
        album: 'Vinyl Edition',
        label: 'Vector',
        catno: null,
        year: 2024,
        genre: null,
        subgenre: null,
        track_number: null,
        duration_delta_ms: null,
        url: null,
        has_artwork: true,
        artwork_provider: 'Discogs',
        artwork_hash: 'art-hash-2',
        evidence: item.evidence!,
      },
    ];
    const client = makeClient([item]);
    render(Prep, { client });
    const coverSelect = (await screen.findByRole('combobox', {
      name: 'Choose suggested cover',
    })) as HTMLSelectElement;
    expect(coverSelect.value).toBe('5');
    const artworkRow = coverSelect.closest('tr')!;
    expect(artworkRow.textContent).toContain('Discogs');
    await fireEvent.change(coverSelect, { target: { value: '2' } });
    expect(client.useCandidateArtwork).toHaveBeenCalledWith(
      item.track.id,
      2,
      false,
      item.review_token,
    );
    await waitFor(() => expect(coverSelect.value).toBe('2'));
  });

  it('prefers exact artwork hash over same-provider fallback for active cover selection', async () => {
    const item = makePrepItem(0);
    item.artwork = {
      track_id: item.track.id,
      artwork: null,
      proposed: { artwork: 'art-hash-1', provider: 'Discogs' },
      dirty_on_disk: false,
    };
    item.selected_candidate = 5;
    item.candidates = [
      {
        index: 2,
        provider: 'discogs',
        providers: ['discogs'],
        confidence: 0.96,
        audio_matched: true,
        artist: 'Kaden',
        title: 'Track 1',
        mix: null,
        album: 'Pressing A',
        label: 'Vector',
        catno: null,
        year: 2024,
        genre: null,
        subgenre: null,
        track_number: null,
        duration_delta_ms: null,
        url: null,
        has_artwork: true,
        artwork_provider: 'Discogs',
        artwork_hash: 'art-hash-1',
        evidence: item.evidence!,
      },
      {
        index: 5,
        provider: 'discogs',
        providers: ['discogs'],
        confidence: 0.91,
        audio_matched: false,
        artist: 'Kaden',
        title: 'Track 1',
        mix: null,
        album: 'Pressing B',
        label: 'Vector',
        catno: null,
        year: 2024,
        genre: null,
        subgenre: null,
        track_number: null,
        duration_delta_ms: null,
        url: null,
        has_artwork: true,
        artwork_provider: 'Discogs',
        artwork_hash: 'art-hash-2',
        evidence: item.evidence!,
      },
    ];
    const client = makeClient([item]);
    render(Prep, { client });
    const coverSelect = (await screen.findByRole('combobox', {
      name: 'Choose suggested cover',
    })) as HTMLSelectElement;
    expect(coverSelect.value).toBe('2');
  });

  it('deduplicates raw and display provider names and hides cover picker after sole cover accept', async () => {
    const item = makePrepItem(0);
    item.artwork = {
      track_id: item.track.id,
      artwork: null,
      proposed: { artwork: 'art-hash-mb', provider: 'MusicBrainz' },
      dirty_on_disk: false,
    };
    item.selected_candidate = 0;
    item.candidates = [
      {
        index: 0,
        provider: 'musicbrainz',
        providers: ['musicbrainz'],
        confidence: 0.96,
        audio_matched: true,
        artist: 'Kaden',
        title: 'Track 1',
        mix: null,
        album: 'Horizon EP',
        label: 'Vector',
        catno: null,
        year: 2024,
        genre: null,
        subgenre: null,
        track_number: null,
        duration_delta_ms: null,
        url: null,
        has_artwork: true,
        artwork_provider: 'MusicBrainz',
        artwork_hash: 'art-hash-mb',
        evidence: item.evidence!,
      },
    ];
    const acceptedItem: PrepItem = {
      ...item,
      artwork: {
        track_id: item.track.id,
        artwork: 'art-hash-mb',
        proposed: null,
        dirty_on_disk: false,
      },
    };
    const client = makeClient([item]);
    vi.mocked(client.prepPage)
      .mockResolvedValueOnce({
        total: 1,
        offset: 0,
        items: [item],
        unwritten_tracks: 0,
        last_operation: null,
      })
      .mockResolvedValue({
        total: 1,
        offset: 0,
        items: [acceptedItem],
        unwritten_tracks: 0,
        last_operation: null,
      });

    render(Prep, { client });
    const acceptArt = await screen.findByRole('button', { name: 'Accept artwork' });
    const table = acceptArt.closest('table')!;
    expect(table.querySelector('thead')?.textContent).toContain('MusicBrainz');
    expect(table.querySelectorAll('tbody small')).toHaveLength(0);

    await fireEvent.click(acceptArt);
    await screen.findByRole('button', { name: 'Undo save' });
    expect(screen.queryByRole('combobox', { name: 'Choose suggested cover' })).toBeNull();
  });

  it('keeps single-track review and accept enabled while showing save notice during an online job', async () => {
    const item = makePrepItem(0);
    const client = makeClient([item]);
    const activeOnlineJob: ActivityJob = {
      id: 7,
      kind: 'identify_online',
      tier: 'on_demand',
      status: 'running',
      label: 'Searching online',
      done_items: 1,
      total_items: 10,
      paused_for_mix: false,
      paused: false,
      error_message: null,
      created_ms: 0,
      started_ms: 1,
      finished_ms: null,
    };
    render(Prep, { client, activeOnlineJob });
    const acceptBtn = (await screen.findByRole('button', {
      name: 'Accept Artist',
    })) as HTMLButtonElement;
    expect(acceptBtn.disabled).toBe(false);
    await fireEvent.click(acceptBtn);
    expect(client.acceptAndWrite).toHaveBeenCalledWith(
      [item.track.id],
      ['artist'],
      false,
      item.review_token,
    );
    await waitFor(() => {
      const status = screen.getByRole('status').textContent ?? '';
      expect(status).toContain('Searching online');
      expect(status).toContain('1 / 10');
      expect(status).toContain('Saved to 1 file');
    });
  });

  it('omits Accept all when compact fallback undo renders on a track with no suggestions', async () => {
    const first = makePrepItem(0);
    const second = makePrepItem(1);
    second.fields = makeFields({});
    second.pending_fields = 0;
    const client = makeClient([first, second]);
    const origObserver = globalThis.ResizeObserver;
    const origWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
    const observers: ResizeObserverCallback[] = [];
    globalThis.ResizeObserver = class {
      constructor(cb: ResizeObserverCallback) {
        observers.push(cb);
      }
      observe() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
    Object.defineProperty(HTMLElement.prototype, 'clientWidth', {
      configurable: true,
      get() {
        return 640;
      },
    });
    vi.mocked(client.prepPage)
      .mockResolvedValueOnce({
        total: 2,
        offset: 0,
        items: [first, second],
        unwritten_tracks: 0,
        last_operation: null,
      })
      .mockResolvedValue({
        total: 1,
        offset: 0,
        items: [second],
        unwritten_tracks: 0,
        last_operation: null,
      });

    try {
      render(Prep, { client });
      for (const cb of observers) cb([], {} as ResizeObserver);
      await fireEvent.click(await screen.findByRole('button', { name: /^Track 0 Ada Marsh/ }));
      await fireEvent.click(await screen.findByRole('button', { name: 'Accept Artist' }));
      await screen.findByRole('heading', { name: second.track.title });
      const undo = await screen.findByRole('button', { name: 'Undo save' });
      expect(undo.closest('.review-head')).not.toBeNull();
      expect(screen.queryByRole('button', { name: 'Accept all' })).toBeNull();
    } finally {
      globalThis.ResizeObserver = origObserver;
      if (origWidth) Object.defineProperty(HTMLElement.prototype, 'clientWidth', origWidth);
      else delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
    }
  });

  it('supports Prep keyboard selection from outside focus without hijacking outside buttons on Enter', async () => {
    const items = [makePrepItem(0), makePrepItem(1)];
    const client = makeClient(items);
    const outsideClick = vi.fn();
    const outer = document.createElement('button');
    outer.textContent = 'Sidebar nav';
    outer.addEventListener('click', outsideClick);
    document.body.appendChild(outer);
    try {
      render(Prep, { client });
      await screen.findByRole('button', { name: 'Accept all' });
      outer.focus();
      await fireEvent.keyDown(outer, { key: 'ArrowDown' });
      const secondRowBtn = screen.getByRole('button', { name: /^Track 1 Ada Marsh/ });
      await waitFor(() => expect(document.activeElement).toBe(secondRowBtn));
      outer.focus();
      await fireEvent.keyDown(outer, { key: 'x' });
      expect(
        (
          screen.getByRole('checkbox', {
            name: 'Select ' + items[1]!.track.title,
          }) as HTMLInputElement
        ).checked,
      ).toBe(true);
      await fireEvent.keyDown(outer, { key: 'Enter' });
      expect(client.acceptAndWrite).not.toHaveBeenCalled();
    } finally {
      outer.remove();
    }
  });
});

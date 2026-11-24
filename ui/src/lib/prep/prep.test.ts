// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import { fakeTrack } from '../library/fake-library';
import type { MatchCandidateSummary } from '../ipc/bindings/MatchCandidateSummary';
import type { PrepItem } from '../ipc/bindings/PrepItem';
import type { PrepStageName } from '../ipc/bindings/PrepStageName';
import type { StagedField } from '../ipc/bindings/StagedField';
import type { TagName } from '../ipc/bindings/TagName';
import type { TrackCandidateArtwork } from '../ipc/bindings/TrackCandidateArtwork';
import Prep, { type PrepClient } from './Prep.svelte';
import {
  basisSentence,
  formatSaveNotice,
  lengthDifference,
  missingNow,
  rowStatus,
  stageSummary,
  stillMissing,
} from './prep-view';

HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

const STAGED_ORDER: TagName[] = ['title', 'artist', 'mix', 'album', 'genre', 'label', 'year'];

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

function proposal(value: string, provider = 'musicbrainz'): Partial<StagedField> {
  return {
    proposed_value: value,
    provider,
    confidence: 0.94,
    audio_matched: true,
    status: 'pending',
    changes: true,
  };
}

function candidate(index: number, over: Partial<MatchCandidateSummary> = {}) {
  const evidence = {
    basis: 'audio_fingerprint' as const,
    providers: ['acoustid', 'musicbrainz'],
    primary_provider: 'musicbrainz',
    duration_delta_ms: 400,
    preview_similarity: null,
    catno_matched: false,
    isrc_matched: false,
    url: null,
  };
  return {
    index,
    provider: 'musicbrainz',
    providers: ['musicbrainz'],
    confidence: 0.94,
    audio_matched: true,
    artist: 'Kaden',
    title: 'Horizon',
    mix: null,
    album: 'Horizon EP',
    label: 'Vector Recordings',
    catno: 'VEC001',
    year: 2024,
    genre: null,
    subgenre: null,
    track_number: null,
    duration_delta_ms: 400,
    url: null,
    has_artwork: false,
    artwork_provider: null,
    artwork_hash: null,
    evidence,
    ...over,
  } satisfies MatchCandidateSummary;
}

function makeItem(index: number, stage: PrepStageName = 'review'): PrepItem {
  const track = fakeTrack(index);
  const review = stage === 'review';
  return {
    track,
    review_token: 'review-' + track.id,
    fields: makeFields(
      review
        ? {
            title: proposal(`Track ${index + 1}`),
            artist: proposal('Kaden'),
            label: proposal('Vector Recordings'),
          }
        : {},
    ),
    artwork: { track_id: track.id, artwork: null, proposed: null, dirty_on_disk: false },
    pending_fields: review ? 3 : 0,
    provider: review ? 'musicbrainz' : null,
    confidence: review ? 0.94 : 0,
    audio_matched: review,
    needs_url: false,
    changeable_confidences: [],
    release: null,
    evidence: review ? candidate(0).evidence : null,
    candidates: review ? [candidate(0), candidate(1, { confidence: 0.7, album: 'Remixes' })] : [],
    selected_candidate: 0,
    candidates_expanded: false,
    steps: [],
    has_unwritten: false,
    online_checked: stage !== 'not_searched',
    stage,
  };
}

function makeClient(items: PrepItem[]) {
  const inStage = (stage: PrepStageName | null) =>
    items.filter((item) => stage === null || item.stage === stage);
  const count = (stage: PrepStageName) => items.filter((item) => item.stage === stage).length;
  const client = {
    trackIds: vi.fn(async (_query, _ids, stage: PrepStageName | null) =>
      inStage(stage).map((item) => item.track.id),
    ),
    prepPage: vi.fn(
      async (_query, ids: number[] | null, stage: PrepStageName | null, offset, limit) => {
        const shown = ids
          ? ids.flatMap((id) => items.filter((item) => item.track.id === id))
          : inStage(stage);
        return {
          total: shown.length,
          offset,
          stages: {
            review: count('review'),
            not_searched: count('not_searched'),
            no_match: count('no_match'),
            done: count('done'),
          },
          items: shown.slice(offset, offset + limit),
          unwritten_tracks: 0,
          last_operation: null,
        };
      },
    ),
    acceptAndWrite: vi.fn(async (ids: number[]) => ({
      written: ids.length,
      library_fields: 0,
      reverted_fields: 0,
      operation_id: 45 as number | null,
      failed: [] as { track_id: number; path: string; reason: string }[],
    })),
    skipItems: vi.fn(async (ids: number[]) => ({ tracks: ids.length, fields: 1 })),
    selectCandidate: vi.fn(async (id: number) => items.find((item) => item.track.id === id)!),
    useCandidateArtwork: vi.fn(async (id: number) => items.find((item) => item.track.id === id)!),
    candidateArtwork: vi.fn(async (id: number): Promise<TrackCandidateArtwork> => ({
      track_id: id,
      candidates: [],
    })),
    skipArtwork: vi.fn(async (id: number) => items.find((item) => item.track.id === id)!),
    identifyOnline: vi.fn(async () => ({ active: [], recent: [], mix_backoff_active: false })),
    identifyWithUrl: vi.fn(async (id: number) => ({
      track_id: id,
      steps: [],
      best: null,
      needs_url: false,
    })),
    previewBatchTag: vi.fn(async () => ({ tracks: 0, fields: 0 })),
    applyBatchTag: vi.fn(async () => ({ tracks: 0, fields: 0 })),
    previewTagWrites: vi.fn(async () => []),
    applyTagWrites: vi.fn(async () => ({ written: 0, operation_id: null, failed: [] })),
    undoOperation: vi.fn(async (id: number) => ({
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
  return client satisfies PrepClient;
}

describe('Prep', () => {
  it('lists every track in one list with a summary of where they stand', async () => {
    const items = [makeItem(0, 'not_searched'), makeItem(1), makeItem(2, 'no_match')];
    const client = makeClient(items);
    render(Prep, { client });
    expect(await screen.findByText('1 to review · 1 not searched · 1 with no match')).toBeTruthy();
    expect(screen.queryByRole('tab')).toBeNull();
    expect(document.querySelectorAll('.row')).toHaveLength(3);
    expect(screen.getByText('Not searched online yet')).toBeTruthy();
  });

  it('keeps every track in its place when a save changes its status', async () => {
    const items = [makeItem(0, 'not_searched'), makeItem(1), makeItem(2, 'not_searched')];
    const client = makeClient(items);
    render(Prep, { client });
    const titles = () =>
      [...document.querySelectorAll('.row .title')].map((node) => node.textContent);
    await screen.findByText('Not searched online yet');
    const before = titles();
    await fireEvent.click(document.querySelectorAll<HTMLButtonElement>('.track-name')[1]!);
    items[1] = { ...items[1]!, stage: 'done', fields: makeFields({}), pending_fields: 0 };
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept all' }));
    await waitFor(() => expect(client.acceptAndWrite).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Accept all' })).toBeNull());
    expect(titles()).toEqual(before);
  });

  it('sorts by status with work first, then the other way', async () => {
    const items = [makeItem(0, 'done'), makeItem(1, 'not_searched'), makeItem(2)];
    const client = makeClient(items);
    render(Prep, { client });
    await screen.findByText(/1 to review/);
    await fireEvent.click(screen.getByRole('button', { name: 'Sort by status' }));
    await waitFor(() =>
      expect(document.querySelector('.row .status')?.getAttribute('data-stage')).toBe('review'),
    );
    await fireEvent.click(screen.getByRole('button', { name: /Sort by status/ }));
    await waitFor(() =>
      expect(document.querySelector('.row .status')?.getAttribute('data-stage')).toBe('done'),
    );
  });

  it('finds matches for every track in the tab without ticking any', async () => {
    const items = [makeItem(0, 'not_searched'), makeItem(1, 'not_searched'), makeItem(2, 'done')];
    const client = makeClient(items);
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('button', { name: 'Find matches for 2' }));
    await waitFor(() =>
      expect(client.identifyOnline).toHaveBeenCalledWith(
        [items[0]!.track.id, items[1]!.track.id],
        true,
      ),
    );
    expect(client.trackIds).toHaveBeenCalledWith(null, null, 'not_searched');
  });

  it('finds matches for the ticked tracks only', async () => {
    const items = [makeItem(0, 'not_searched'), makeItem(1, 'not_searched')];
    const client = makeClient(items);
    render(Prep, { client });
    await fireEvent.click(
      await screen.findByRole('checkbox', { name: 'Tick ' + items[1]!.track.title }),
    );
    expect(screen.getByText('1 ticked')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Find matches' }));
    await waitFor(() =>
      expect(client.identifyOnline).toHaveBeenCalledWith([items[1]!.track.id], true),
    );
  });

  it('accepts one suggested tag with its own button', async () => {
    const items = [makeItem(0), makeItem(1)];
    const client = makeClient(items);
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept Label' }));
    await waitFor(() =>
      expect(client.acceptAndWrite).toHaveBeenCalledWith(
        [items[0]!.track.id],
        ['label'],
        false,
        'review-' + items[0]!.track.id,
      ),
    );
    expect(client.skipItems).not.toHaveBeenCalled();
    expect(await screen.findByText('Saved to 1 file')).toBeTruthy();
  });

  it('shows the covers of the matches and keeps the file cover when chosen', async () => {
    const item = makeItem(0);
    item.artwork = { ...item.artwork, artwork: 'a'.repeat(64) };
    item.candidates = [
      candidate(0, { has_artwork: true, artwork_provider: 'Discogs' }),
      candidate(1, { has_artwork: true, artwork_provider: 'Bandcamp' }),
    ];
    const client = makeClient([item]);
    client.candidateArtwork.mockImplementation(async (id: number) => ({
      track_id: id,
      candidates: [0, 1].map((index) => ({
        index,
        has_artwork: true,
        artwork_provider: 'Discogs',
        artwork_hash: String(index).repeat(64),
      })),
    }));
    render(Prep, { client });
    await waitFor(() => expect(client.candidateArtwork).toHaveBeenCalledWith(item.track.id, 0));
    expect(screen.getByText('3 to choose from')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: /Use cover from Bandcamp/ }));
    await waitFor(() =>
      expect(client.useCandidateArtwork).toHaveBeenCalledWith(
        item.track.id,
        1,
        false,
        'review-' + item.track.id,
      ),
    );
    await fireEvent.click(screen.getByRole('button', { name: /In the file/ }));
    await waitFor(() => expect(client.skipArtwork).toHaveBeenCalledWith(item.track.id));
  });

  it('shows every tag, marking the missing ones', async () => {
    const client = makeClient([makeItem(0)]);
    render(Prep, { client });
    const table = await screen.findByRole('table');
    expect(table.textContent).toContain('Genre');
    expect(table.textContent).toContain('Missing');
    expect(screen.getByText('3 suggested')).toBeTruthy();
  });

  it('shows search progress in Prep and what the search found', async () => {
    const items = [makeItem(0, 'not_searched'), makeItem(1, 'not_searched')];
    const client = makeClient(items);
    client.identifyOnline.mockResolvedValueOnce({
      active: [],
      recent: [{ id: 9, kind: 'identify_online', status: 'done' }],
      mix_backoff_active: false,
    } as never);
    const view = render(Prep, { client });
    await fireEvent.click(await screen.findByRole('button', { name: 'Find matches for 2' }));
    expect(await screen.findByText(/Search finished for/)).toBeTruthy();
    expect(client.trackIds).toHaveBeenCalledWith(
      null,
      [items[0]!.track.id, items[1]!.track.id],
      'review',
    );
    await view.rerender({
      client,
      activeOnlineJob: {
        id: 10,
        kind: 'identify_online',
        done_items: 3,
        total_items: 8,
        paused: false,
      } as never,
    });
    expect(await screen.findByText('3 of 8 tracks')).toBeTruthy();
  });
  it('saves every change with Enter and leaves nothing to skip', async () => {
    const items = [makeItem(0)];
    const client = makeClient(items);
    render(Prep, { client });
    await screen.findByRole('button', { name: 'Accept 3 changes' });
    await fireEvent.keyDown(window, { key: 'Enter' });
    await waitFor(() =>
      expect(client.acceptAndWrite).toHaveBeenCalledWith(
        [items[0]!.track.id],
        ['title', 'artist', 'label'],
        false,
        'review-' + items[0]!.track.id,
      ),
    );
    expect(client.skipItems).not.toHaveBeenCalled();
  });

  it('keeps the rest of a failed save waiting and lists the file', async () => {
    const items = [makeItem(0)];
    const client = makeClient(items);
    client.acceptAndWrite.mockResolvedValueOnce({
      written: 0,
      library_fields: 0,
      reverted_fields: 0,
      operation_id: null,
      failed: [{ track_id: items[0]!.track.id, path: 'a.mp3', reason: 'File is read-only' }],
    });
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept Title' }));
    expect(await screen.findByText('1 file needs attention')).toBeTruthy();
    expect(client.skipItems).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Undo save' })).toBeNull();
  });

  it('skips a track with S and undoes a save with Ctrl+Z', async () => {
    const items = [makeItem(0), makeItem(1)];
    const client = makeClient(items);
    render(Prep, { client });
    await screen.findByRole('button', { name: 'Accept 3 changes' });
    await fireEvent.keyDown(window, { key: 's' });
    await waitFor(() => expect(client.skipItems).toHaveBeenCalledWith([items[0]!.track.id], []));
    await fireEvent.click(await screen.findByRole('button', { name: 'Accept 3 changes' }));
    await screen.findByText('Saved to 1 file');
    await fireEvent.keyDown(window, { key: 'z', ctrlKey: true });
    await waitFor(() => expect(client.undoOperation).toHaveBeenCalledWith(45));
    expect(await screen.findByText('Restored 1 file')).toBeTruthy();
  });

  it('chooses another match with its number key', async () => {
    const items = [makeItem(0)];
    const client = makeClient(items);
    render(Prep, { client });
    await screen.findByRole('button', { name: 'Accept 3 changes' });
    await fireEvent.keyDown(window, { key: '2' });
    await waitFor(() =>
      expect(client.selectCandidate).toHaveBeenCalledWith(
        items[0]!.track.id,
        1,
        'review-' + items[0]!.track.id,
      ),
    );
  });

  it('searches the focused track with Enter when it has not been searched', async () => {
    const items = [makeItem(0, 'not_searched')];
    const client = makeClient(items);
    render(Prep, { client });
    await screen.findByText('Not searched online yet');
    await fireEvent.keyDown(window, { key: 'Enter' });
    await waitFor(() =>
      expect(client.identifyOnline).toHaveBeenCalledWith([items[0]!.track.id], true),
    );
  });

  it('ticks every track, loaded or not', async () => {
    const items = Array.from({ length: 120 }, (_, i) => makeItem(i));
    const client = makeClient(items);
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('checkbox', { name: 'Select all tracks' }));
    await waitFor(() => expect(client.trackIds).toHaveBeenCalledWith(null, null, null));
    expect(await screen.findByText('120 ticked')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Save ticked' }));
    await waitFor(() =>
      expect(client.acceptAndWrite).toHaveBeenCalledWith(
        items.map((item) => item.track.id),
        null,
        true,
      ),
    );
  });

  it('keeps Library filters when filtering for a missing tag', async () => {
    const client = makeClient([makeItem(0)]);
    render(Prep, { client, batchQuery: { text: 'crate' }, batchTrackIds: [1, 2] });
    await fireEvent.change(await screen.findByRole('combobox', { name: 'Filter tracks' }), {
      target: { value: 'artwork' },
    });
    await waitFor(() =>
      expect(client.prepPage).toHaveBeenCalledWith(
        {
          text: 'crate',
          filters: [{ any_of: [{ column: 'artwork', condition: { op: 'missing' } }] }],
        },
        [1, 2],
        null,
        0,
        1,
      ),
    );
  });
});

describe('prep view', () => {
  it('sums up where the tracks stand, leaving out empty stages', () => {
    expect(stageSummary({ review: 36, not_searched: 1395, no_match: 0, done: 2 })).toBe(
      '36 to review · 1,395 not searched · 2 done',
    );
    expect(stageSummary({ review: 0, not_searched: 0, no_match: 0, done: 0 })).toBe('');
  });

  it('describes each row by its stage', () => {
    expect(rowStatus(makeItem(0))).toEqual({
      primary: 'Audio match',
      secondary: '3 changes',
      confidence: '94%',
    });
    const base = makeItem(1);
    const local = {
      ...base,
      candidates: [],
      evidence: null,
      fields: base.fields.map((field) => ({ ...field, provider: field.provider && 'filename' })),
    };
    expect(rowStatus(local).primary).toBe('From filename');
    expect(rowStatus({ ...makeItem(2, 'no_match'), needs_url: true })).toEqual({
      primary: 'No match',
      secondary: 'Needs a release link',
      confidence: null,
    });
  });

  it('words the evidence for a DJ', () => {
    expect(basisSentence('tags_only')).toBe('Only the names match. Listen before saving');
    expect(lengthDifference(400)).toBe('Same length');
    expect(lengthDifference(-2500)).toBe('2.5 s shorter');
    expect(lengthDifference(null)).toBe('');
  });

  it('lists only the core tags still missing after the suggestions', () => {
    expect(stillMissing(makeItem(0))).toEqual(['Artwork', 'Genre', 'Year']);
    expect(missingNow(makeItem(0))).toEqual(['artist', 'title', 'cover', 'genre', 'label', 'year']);
  });

  it('formats save notices with digits', () => {
    expect(
      formatSaveNotice({ written: 2, library_fields: 1, reverted_fields: 0, failed: [] }),
    ).toBe('Saved to 2 files · 1 field saved to library');
  });
});

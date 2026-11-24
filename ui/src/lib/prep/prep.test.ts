// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import { fakeTrack } from '../library/fake-library';
import type { MatchCandidateSummary } from '../ipc/bindings/MatchCandidateSummary';
import type { PrepItem } from '../ipc/bindings/PrepItem';
import type { PrepStageName } from '../ipc/bindings/PrepStageName';
import type { StagedField } from '../ipc/bindings/StagedField';
import type { TagName } from '../ipc/bindings/TagName';
import Prep, { type PrepClient } from './Prep.svelte';
import {
  basisSentence,
  firstWorkingTab,
  formatSaveNotice,
  lengthDifference,
  rowStatus,
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
    prepPage: vi.fn(async (_query, _ids, stage: PrepStageName | null, offset, limit) => {
      const shown = inStage(stage);
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
    }),
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
  it('opens on the first tab with work in it and shows a count on every tab', async () => {
    const items = [makeItem(0, 'not_searched'), makeItem(1), makeItem(2, 'no_match')];
    const client = makeClient(items);
    render(Prep, { client });
    const review = await screen.findByRole('tab', { name: /To review/ });
    await waitFor(() => expect(review.getAttribute('aria-selected')).toBe('true'));
    expect(client.prepPage).toHaveBeenLastCalledWith(null, null, 'review', 0, 50);
    expect(screen.getByRole('tab', { name: /Not searched/ }).textContent).toContain('1');
    expect(screen.getByRole('tab', { name: /All/ }).textContent).toContain('3');
    expect(await screen.findByText('94%', { selector: '.confidence' })).toBeTruthy();
    expect(screen.getByText(/The audio fingerprint matches/)).toBeTruthy();
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

  it('saves only the ticked changes and sets the rest aside', async () => {
    const items = [makeItem(0), makeItem(1)];
    const client = makeClient(items);
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('checkbox', { name: 'Label' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save 2 changes' }));
    await waitFor(() =>
      expect(client.acceptAndWrite).toHaveBeenCalledWith(
        [items[0]!.track.id],
        ['title', 'artist'],
        false,
        'review-' + items[0]!.track.id,
      ),
    );
    await waitFor(() => expect(client.skipItems).toHaveBeenCalledWith([items[0]!.track.id], []));
    expect(await screen.findByText('Saved to 1 file')).toBeTruthy();
  });

  it('saves every change with Enter and leaves nothing to skip', async () => {
    const items = [makeItem(0)];
    const client = makeClient(items);
    render(Prep, { client });
    await screen.findByRole('button', { name: 'Save 3 changes' });
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
    await fireEvent.click(await screen.findByRole('checkbox', { name: 'Title' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save 2 changes' }));
    expect(await screen.findByText('1 file needs attention')).toBeTruthy();
    expect(client.skipItems).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Undo save' })).toBeNull();
  });

  it('skips a track with S and undoes a save with Ctrl+Z', async () => {
    const items = [makeItem(0), makeItem(1)];
    const client = makeClient(items);
    render(Prep, { client });
    await screen.findByRole('button', { name: 'Save 3 changes' });
    await fireEvent.keyDown(window, { key: 's' });
    await waitFor(() => expect(client.skipItems).toHaveBeenCalledWith([items[0]!.track.id], []));
    await fireEvent.click(await screen.findByRole('button', { name: 'Save 3 changes' }));
    await screen.findByText('Saved to 1 file');
    await fireEvent.keyDown(window, { key: 'z', ctrlKey: true });
    await waitFor(() => expect(client.undoOperation).toHaveBeenCalledWith(45));
    expect(await screen.findByText('Restored 1 file')).toBeTruthy();
  });

  it('chooses another match with its number key', async () => {
    const items = [makeItem(0)];
    const client = makeClient(items);
    render(Prep, { client });
    await screen.findByRole('button', { name: 'Save 3 changes' });
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

  it('ticks every track in the tab across pages', async () => {
    const items = Array.from({ length: 120 }, (_, i) => makeItem(i));
    const client = makeClient(items);
    render(Prep, { client });
    await fireEvent.click(await screen.findByRole('checkbox', { name: 'Select all tracks' }));
    await waitFor(() => expect(client.trackIds).toHaveBeenCalledWith(null, null, 'review'));
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
        expect.anything(),
        0,
        50,
      ),
    );
  });
});

describe('prep view', () => {
  it('opens on review, then unsearched, then no match', () => {
    const none = { review: 0, not_searched: 0, no_match: 0, done: 4 };
    expect(firstWorkingTab({ ...none, review: 1, not_searched: 2 })).toBe('review');
    expect(firstWorkingTab({ ...none, not_searched: 2, no_match: 1 })).toBe('not_searched');
    expect(firstWorkingTab({ ...none, no_match: 1 })).toBe('no_match');
    expect(firstWorkingTab(none)).toBe('all');
  });

  it('describes each row by its stage', () => {
    expect(rowStatus(makeItem(0))).toEqual({
      primary: 'Audio match',
      secondary: '3 changes',
      confidence: '94%',
    });
    const local = { ...makeItem(1), candidates: [], evidence: null };
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
  });

  it('formats save notices with digits', () => {
    expect(
      formatSaveNotice({ written: 2, library_fields: 1, reverted_fields: 0, failed: [] }),
    ).toBe('Saved to 2 files · 1 field saved to library');
  });
});

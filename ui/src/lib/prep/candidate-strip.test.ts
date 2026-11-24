// @vitest-environment happy-dom
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { MatchCandidateSummary } from '../ipc/bindings/MatchCandidateSummary';
import CandidateStrip from './CandidateStrip.svelte';
import MatchEvidenceHeader from './MatchEvidenceHeader.svelte';

function makeCandidate(index: number): MatchCandidateSummary {
  const isFirst = index === 0;
  return {
    index,
    provider: isFirst ? 'deezer' : 'discogs',
    providers: isFirst ? ['deezer', 'musicbrainz'] : ['discogs'],
    confidence: isFirst ? 0.96 : 0.89,
    audio_matched: false,
    artist: 'Some Artist',
    title: 'Night Drive',
    mix: isFirst ? 'Extended Mix' : 'Dub Mix',
    album: isFirst ? 'Night Drive EP' : 'Club Dubs',
    label: 'Defected',
    catno: isFirst ? 'DFTD01' : 'DFTD01D',
    year: 2022,
    genre: 'Electronic',
    subgenre: isFirst ? 'House' : 'Deep House',
    track_number: isFirst ? '1' : 'A1',
    duration_delta_ms: isFirst ? 400 : 1200,
    url: isFirst ? 'https://musicbrainz.org/release/rel-1' : 'https://www.discogs.com/release/2',
    has_artwork: !isFirst,
    artwork_provider: isFirst ? null : 'Discogs',
    artwork_hash: null,
    evidence: {
      basis: isFirst ? 'tags_and_duration' : 'catalog_and_duration',
      providers: isFirst ? ['deezer', 'musicbrainz'] : ['discogs'],
      primary_provider: isFirst ? 'deezer' : 'discogs',
      duration_delta_ms: isFirst ? 400 : 1200,
      preview_similarity: null,
      catno_matched: !isFirst,
      isrc_matched: false,
      url: isFirst ? 'https://musicbrainz.org/release/rel-1' : 'https://www.discogs.com/release/2',
    },
  };
}

describe('Match review', () => {
  it('shows mix and source in the compact match picker and uses the stored candidate index', async () => {
    const onselect = vi.fn();
    render(CandidateStrip, {
      candidates: [makeCandidate(0), makeCandidate(4)],
      selected: 0,
      busy: false,
      onselect,
    });
    const select = screen.getByRole('combobox', { name: 'Choose match' });
    expect(screen.getByRole('option', { name: /Dub Mix.*Discogs/ })).toBeTruthy();
    await fireEvent.change(select, { target: { value: '4' } });
    expect(onselect).toHaveBeenCalledWith(4);
  });

  it('keeps missing keys and failed requests distinct in the source details', () => {
    render(MatchEvidenceHeader, {
      evidence: null,
      steps: [
        {
          step: 3,
          provider: 'acoustid',
          outcome: { kind: 'skipped_missing_key', key: 'acoustid' },
        },
        { step: 4, provider: 'discogs', outcome: { kind: 'failed', reason: 'Timed out' } },
      ],
    });
    expect(screen.getByText('Key needed').closest('details')?.open).toBe(false);
    expect(screen.getByText('Timed out')).toBeTruthy();
  });
});

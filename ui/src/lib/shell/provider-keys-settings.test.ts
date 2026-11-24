// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { ProviderKeyKind } from '../ipc/bindings/ProviderKeyKind';
import type { ProviderKeyStatus } from '../ipc/bindings/ProviderKeyStatus';
import ProviderKeysSettings, { type ProviderKeysClient } from './ProviderKeysSettings.svelte';

function fakeClient(initial: Partial<Record<ProviderKeyKind, boolean>> = {}, initialVerify = true) {
  const stored: Record<ProviderKeyKind, boolean> = {
    acoustid: false,
    discogs: false,
    audd: false,
    ...initial,
  };
  let verifyPreviews = initialVerify;
  const status = (provider: ProviderKeyKind): ProviderKeyStatus => ({
    provider,
    configured: stored[provider],
    paid: provider === 'audd',
  });
  const client: ProviderKeysClient = {
    status: vi.fn(async () => (['acoustid', 'discogs', 'audd'] as const).map(status)),
    save: vi.fn(async (provider: ProviderKeyKind) => {
      stored[provider] = true;
      return status(provider);
    }),
    clear: vi.fn(async (provider: ProviderKeyKind) => {
      stored[provider] = false;
      return status(provider);
    }),
    identificationSettings: vi.fn(async () => ({ verify_audio_previews: verifyPreviews })),
    saveIdentificationSettings: vi.fn(async (settings) => {
      verifyPreviews = settings.verify_audio_previews;
      return { verify_audio_previews: verifyPreviews };
    }),
  };
  return client;
}

function row(name: string): HTMLElement {
  return screen.getByRole('group', { name });
}

describe('ProviderKeysSettings', () => {
  it('shows each free key as not set and keeps paid recognisers folded away', async () => {
    const client = fakeClient();
    render(ProviderKeysSettings, { client });
    await waitFor(() => expect(client.status).toHaveBeenCalled());

    for (const name of ['AcoustID application key', 'Discogs personal access token']) {
      expect(within(row(name)).getByText('Not set')).toBeTruthy();
    }
    const disclosure = screen.getByRole('button', { name: 'Paid recognisers' });
    expect(disclosure.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByRole('group', { name: 'AudD API token' })).toBeNull();

    await fireEvent.click(disclosure);
    expect(within(row('AudD API token')).getByText('Not set')).toBeTruthy();
  });

  it('saves a key through a masked field, then clears it', async () => {
    const client = fakeClient();
    render(ProviderKeysSettings, { client });
    const acoustid = row('AcoustID application key');
    const input = within(acoustid).getByLabelText('AcoustID application key') as HTMLInputElement;
    expect(input.type).toBe('password');

    await fireEvent.input(input, { target: { value: 'test-key' } });
    await fireEvent.click(within(acoustid).getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(within(acoustid).getByText('Configured')).toBeTruthy());
    expect(client.save).toHaveBeenCalledWith('acoustid', 'test-key');
    expect(input.value).toBe('');
    expect(screen.getByRole('status').textContent).toBe('AcoustID key saved.');

    await fireEvent.click(within(acoustid).getByRole('button', { name: 'Clear AcoustID key' }));
    await waitFor(() => expect(within(acoustid).getByText('Not set')).toBeTruthy());
    expect(client.clear).toHaveBeenCalledWith('acoustid');
  });

  it('opens paid recognisers when one already has a key', async () => {
    render(ProviderKeysSettings, { client: fakeClient({ audd: true }) });
    await waitFor(() => expect(within(row('AudD API token')).getByText('Configured')).toBeTruthy());
  });

  it('shows the core error when a save fails', async () => {
    const client = fakeClient();
    client.save = vi.fn(async () => {
      throw new Error('No credential store is available on this computer.');
    });
    render(ProviderKeysSettings, { client });
    const discogs = row('Discogs personal access token');
    await fireEvent.input(within(discogs).getByLabelText('Discogs personal access token'), {
      target: { value: 'token' },
    });
    await fireEvent.click(within(discogs).getByRole('button', { name: 'Save' }));

    await waitFor(() =>
      expect(screen.getByRole('alert').textContent).toBe(
        'No credential store is available on this computer.',
      ),
    );
    expect(within(discogs).getByText('Not set')).toBeTruthy();
  });

  it('persists the 30 s audio preview verification setting and clarifies the AcoustID key role', async () => {
    const client = fakeClient({}, true);
    render(ProviderKeysSettings, { client });
    await waitFor(() => expect(client.identificationSettings).toHaveBeenCalled());

    expect(
      screen.getByText(
        'Use the Application API key from your AcoustID app registration, not a User API key',
      ),
    ).toBeTruthy();
    const toggle = screen.getByRole('checkbox', {
      name: 'Verify uncertain matches against 30 s audio previews',
    }) as HTMLInputElement;
    expect(toggle.checked).toBe(true);

    await fireEvent.click(toggle);
    await waitFor(() =>
      expect(client.saveIdentificationSettings).toHaveBeenCalledWith({
        verify_audio_previews: false,
      }),
    );
    expect(toggle.checked).toBe(false);
    expect(screen.getByRole('status').textContent).toBe('Preview verification turned off.');
  });
});

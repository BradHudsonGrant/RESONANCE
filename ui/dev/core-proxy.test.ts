import { describe, expect, it } from 'vitest';

import { isOwnPage } from './core-proxy.ts';

describe('isOwnPage', () => {
  it('accepts a page served by this dev server on a loopback name', () => {
    expect(isOwnPage('http://localhost:5173', 'localhost:5173')).toBe(true);
    expect(isOwnPage('http://127.0.0.1:5173', '127.0.0.1:5173')).toBe(true);
    expect(isOwnPage('http://[::1]:5173', '[::1]:5173')).toBe(true);
  });

  it('refuses a page from another site', () => {
    expect(isOwnPage('https://example.com', 'localhost:5173')).toBe(false);
  });

  it('refuses a page from another port on this machine', () => {
    expect(isOwnPage('http://localhost:8080', 'localhost:5173')).toBe(false);
  });

  it('refuses a site whose own hostname points at this machine', () => {
    expect(isOwnPage('http://example.com:5173', 'example.com:5173')).toBe(false);
  });

  it('refuses a request with no origin, an opaque origin or no host', () => {
    expect(isOwnPage(undefined, 'localhost:5173')).toBe(false);
    expect(isOwnPage('null', 'localhost:5173')).toBe(false);
    expect(isOwnPage('http://localhost:5173', undefined)).toBe(false);
  });
});

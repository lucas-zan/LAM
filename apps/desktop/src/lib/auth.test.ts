import { describe, expect, it } from 'vitest';
import { authModeLabel } from './auth';

describe('authModeLabel', () => {
  it('keeps user-facing auth labels consistent across account surfaces', () => {
    expect(authModeLabel('personal_token')).toBe('PAT');
    expect(authModeLabel('uploaded')).toBe('Uploaded');
    expect(authModeLabel('oauth')).toBe('Auth');
    expect(authModeLabel('api_key')).toBe('Auth');
    expect(authModeLabel('config')).toBe('Auth');
  });

  it('hides absent auth modes', () => {
    expect(authModeLabel(null)).toBeNull();
    expect(authModeLabel(undefined)).toBeNull();
    expect(authModeLabel('')).toBeNull();
  });
});

import { describe, expect, it, vi, beforeEach } from 'vitest';
import { uploadPatCredentials, getPatMetadata, checkProfileTokenExpiration } from './api';
import type { UploadedCredentials, AuthMetadata, TokenExpirationStatus } from './types';

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// Get reference to mocked invoke
import { invoke } from '@tauri-apps/api/core';
import { type Mock } from 'vitest';

const mockInvoke = invoke as Mock;

beforeEach(() => {
  vi.clearAllMocks();
});

describe('PAT API functions', () => {
  describe('uploadPatCredentials', () => {
    it('should call upload_pat_credentials command with correct parameters', async () => {
      const profileId = 'test-profile';
      const uploaded: UploadedCredentials = {
        accessToken: 'at-test',
        accountId: 'id',
        disabled: false,
        email: 'test@example.com',
        expired: '2030-12-31T10:00:00+08:00',
        lastRefresh: '2026-06-24T00:00:00+08:00',
        type: 'codex',
        websockets: true,
      };

      mockInvoke.mockResolvedValue(undefined);

      await uploadPatCredentials(profileId, uploaded);

      expect(mockInvoke).toHaveBeenCalledWith('upload_pat_credentials', {
        profileId,
        uploaded,
      });
    });
  });

  describe('getPatMetadata', () => {
    it('should return metadata when it exists', async () => {
      const profileId = 'test-profile';
      const metadata: AuthMetadata = {
        profileId,
        authType: 'personal_token',
        tokenExpiration: '2030-12-31T10:00:00+08:00',
        lastChecked: '2026-06-24T00:00:00+08:00',
      };

      mockInvoke.mockResolvedValue(metadata);

      const result = await getPatMetadata(profileId);

      expect(mockInvoke).toHaveBeenCalledWith('get_pat_metadata', { profileId });
      expect(result).toEqual(metadata);
    });

    it('should return null when metadata does not exist', async () => {
      mockInvoke.mockResolvedValue(null);

      const result = await getPatMetadata('nonexistent');

      expect(result).toBeNull();
    });
  });

  describe('checkProfileTokenExpiration', () => {
    it('should return expiration status', async () => {
      const profileId = 'test-profile';
      const status: TokenExpirationStatus = {
        profileId,
        isExpired: false,
        daysUntilExpiration: 100,
        expirationDate: '2030-12-31T10:00:00+08:00',
        warningLevel: 'ok',
      };

      mockInvoke.mockResolvedValue(status);

      const result = await checkProfileTokenExpiration(profileId);

      expect(mockInvoke).toHaveBeenCalledWith('check_profile_token_expiration', { profileId });
      expect(result).toEqual(status);
    });

    it('should handle expired tokens', async () => {
      const status: TokenExpirationStatus = {
        profileId: 'test',
        isExpired: true,
        daysUntilExpiration: -10,
        expirationDate: '2020-01-01T10:00:00+08:00',
        warningLevel: 'expired',
      };

      mockInvoke.mockResolvedValue(status);

      const result = await checkProfileTokenExpiration('test');

      expect(result.isExpired).toBe(true);
      expect(result.warningLevel).toBe('expired');
    });
  });
});

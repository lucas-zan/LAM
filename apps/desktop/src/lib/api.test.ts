import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  addPatAccount,
  uploadPatCredentials,
  getPatMetadata,
  checkProfileTokenExpiration,
  getApiAccountConnectionV2,
  updateApiAccountConnectionV2,
  getAntigravityPort,
  setAntigravityPort,
  listSessionsPage,
  querySessionsPage,
  getSessionStorageSummary,
  deleteSessions,
  queryDeletableSessionPaths,
  getCodexLaunchPermissionPreset,
  setCodexLaunchPermissionPreset,
} from './api';
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
  it('uses exact session query and delete command payloads', async () => {
    mockInvoke.mockResolvedValue({ items: [], nextCursor: null });
    const descriptor = Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__');
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: { invoke: mockInvoke },
    });

    try {
      await querySessionsPage('main', {
        limit: 20,
        sort: 'largest',
        age: 'olderThan30Days',
      });
      await getSessionStorageSummary('main');
      await deleteSessions({ profileId: 'main', paths: ['/tmp/session.jsonl'] });
      await queryDeletableSessionPaths('main', {
        age: 'olderThan30Days',
        query: 'cleanup',
      });

      expect(mockInvoke).toHaveBeenNthCalledWith(1, 'query_sessions_page', {
        accountId: 'main',
        req: { limit: 20, sort: 'largest', age: 'olderThan30Days' },
      });
      expect(mockInvoke).toHaveBeenNthCalledWith(2, 'get_session_storage_summary', {
        accountId: 'main',
      });
      expect(mockInvoke).toHaveBeenNthCalledWith(3, 'delete_sessions', {
        req: { profileId: 'main', paths: ['/tmp/session.jsonl'] },
      });
      expect(mockInvoke).toHaveBeenNthCalledWith(4, 'query_deletable_session_paths', {
        accountId: 'main',
        req: { age: 'olderThan30Days', query: 'cleanup' },
      });
    } finally {
      if (descriptor) Object.defineProperty(window, '__TAURI_INTERNALS__', descriptor);
      else delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it('requests a paged session list with the account and cursor payload', async () => {
    mockInvoke.mockResolvedValue({ items: [], nextCursor: null });
    const descriptor = Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__');
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: { invoke: mockInvoke },
    });

    try {
      await listSessionsPage('main', {
        limit: 5,
        cursor: { modifiedAt: 10, path: '/tmp/main/session.jsonl' },
      });

      expect(mockInvoke).toHaveBeenCalledWith('list_sessions_page', {
        accountId: 'main',
        req: {
          limit: 5,
          cursor: { modifiedAt: 10, path: '/tmp/main/session.jsonl' },
        },
      });
    } finally {
      if (descriptor) Object.defineProperty(window, '__TAURI_INTERNALS__', descriptor);
      else delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it('uses exact Codex launch permission setting payloads', async () => {
    mockInvoke.mockResolvedValue('askForApproval');
    const descriptor = Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__');
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: { invoke: mockInvoke },
    });

    try {
      await getCodexLaunchPermissionPreset();
      await setCodexLaunchPermissionPreset('approveForMe');
      expect(mockInvoke).toHaveBeenNthCalledWith(1, 'get_codex_launch_permission_preset');
      expect(mockInvoke).toHaveBeenNthCalledWith(2, 'set_codex_launch_permission_preset', {
        preset: 'approveForMe',
      });
    } finally {
      if (descriptor) Object.defineProperty(window, '__TAURI_INTERNALS__', descriptor);
      else delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it('uses exact API account detail and update command payloads', async () => {
    mockInvoke.mockResolvedValue({ profileId: 'work-api' });
    await getApiAccountConnectionV2('work-api');
    expect(mockInvoke).toHaveBeenLastCalledWith('get_api_account_connection_v2', {
      profileId: 'work-api',
    });
    const request = {
      profileId: 'work-api',
      expectedProviderStoreRevision: 7,
      baseUrl: 'https://new.example.test/v1',
      apiKey: 'sk-write-only',
    };
    await updateApiAccountConnectionV2(request);
    expect(mockInvoke).toHaveBeenLastCalledWith('update_api_account_connection_v2', {
      req: request,
    });
  });

  describe('addPatAccount', () => {
    it('passes the uploaded auth.json without transforming it', async () => {
      const authJson = {
        auth_mode: 'chatgpt',
        OPENAI_API_KEY: null,
        tokens: {
          access_token: 'at-test',
          refresh_token: 'rt-test',
        },
      };

      mockInvoke.mockResolvedValue({
        accountId: 'test-profile',
        email: '',
        expired: '',
      });

      await addPatAccount({
        accountId: 'test-profile',
        authJson,
        personalAccessToken: 'pat-test',
        tokenExpiration: '2030-12-31T23:59:59.000Z',
      });

      expect(mockInvoke).toHaveBeenCalledWith('add_pat_account', {
        req: {
          accountId: 'test-profile',
          authJson,
          personalAccessToken: 'pat-test',
          tokenExpiration: '2030-12-31T23:59:59.000Z',
        },
      });
    });
  });

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

describe('Antigravity settings API', () => {
  it('uses exact get and set commands and payloads', async () => {
    const descriptor = Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__');
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: { invoke: vi.fn() },
    });
    try {
      mockInvoke.mockResolvedValueOnce(62891).mockResolvedValue(undefined);

      await expect(getAntigravityPort()).resolves.toBe(62891);
      expect(mockInvoke).toHaveBeenLastCalledWith('get_antigravity_port');
      await setAntigravityPort(62891);
      expect(mockInvoke).toHaveBeenLastCalledWith('set_antigravity_port', { port: 62891 });
      await setAntigravityPort(null);
      expect(mockInvoke).toHaveBeenLastCalledWith('set_antigravity_port', { port: null });
    } finally {
      if (descriptor) Object.defineProperty(window, '__TAURI_INTERNALS__', descriptor);
      else delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it('uses browser fallbacks without a Tauri bridge', async () => {
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;

    await expect(getAntigravityPort()).resolves.toBeNull();
    await expect(setAntigravityPort(62891)).resolves.toBeUndefined();
    await expect(setAntigravityPort(null)).resolves.toBeUndefined();
    expect(mockInvoke).not.toHaveBeenCalled();
  });
});

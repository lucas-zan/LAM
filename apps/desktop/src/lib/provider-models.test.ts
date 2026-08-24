import { describe, expect, it } from 'vitest';
import { resolveAccountProvider, sameModelIdSet } from './provider-models';
import type {
  CodexAccount,
  ProfileProviderBindingViewV2,
  ProviderProfileViewV2,
} from './types';

const account = { id: 'opencode', providerId: null } as CodexAccount;
const provider = {
  id: 'account-opencode',
  models: [{ id: 'deepseek-v4-flash', label: 'DeepSeek V4 Flash' }],
} as ProviderProfileViewV2;
const binding = {
  profileId: 'opencode',
  providerId: 'account-opencode',
} as ProfileProviderBindingViewV2;

describe('resolveAccountProvider', () => {
  it('falls back to the durable profile binding when the parsed account provider is missing', () => {
    expect(resolveAccountProvider(account, [provider], [binding])).toBe(provider);
  });

  it('prefers the account provider id when it is available', () => {
    expect(
      resolveAccountProvider({ ...account, providerId: 'account-opencode' }, [provider], []),
    ).toBe(provider);
  });
});

describe('sameModelIdSet', () => {
  it('matches by id set while ignoring order and labels', () => {
    const left = [
      { id: 'a', label: 'A' },
      { id: 'b', label: 'B' },
    ];
    const right = [
      { id: 'b', label: 'Bee' },
      { id: 'a', label: 'Aye' },
    ];
    const shorter = [{ id: 'a', label: 'A' }];
    const longer = [
      { id: 'a', label: 'A' },
      { id: 'b', label: 'B' },
    ];
    expect(sameModelIdSet(left, right)).toBe(true);
    expect(sameModelIdSet(shorter, longer)).toBe(false);
  });
});

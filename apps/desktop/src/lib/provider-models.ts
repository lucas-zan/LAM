import type {
  CodexAccount,
  ProfileProviderBindingViewV2,
  ProviderProfileViewV2,
} from './types';

export function resolveAccountProvider(
  account: CodexAccount,
  providers: ProviderProfileViewV2[],
  bindings: ProfileProviderBindingViewV2[],
): ProviderProfileViewV2 | undefined {
  const providerId =
    account.providerId ?? bindings.find((binding) => binding.profileId === account.id)?.providerId;
  return providers.find((provider) => provider.id === providerId);
}

export function sameModelIdSet(
  left: ReadonlyArray<{ id: string }>,
  right: ReadonlyArray<{ id: string }>,
): boolean {
  const leftIds = new Set(left.map((model) => model.id));
  const rightIds = new Set(right.map((model) => model.id));
  if (leftIds.size !== left.length || rightIds.size !== right.length) return false;
  if (leftIds.size !== rightIds.size) return false;
  for (const id of leftIds) {
    if (!rightIds.has(id)) return false;
  }
  return true;
}

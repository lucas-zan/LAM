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

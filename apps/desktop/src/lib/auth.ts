export function authModeLabel(authMode?: string | null): string | null {
  if (!authMode) return null;
  if (authMode === 'personal_token') return 'PAT';
  if (authMode === 'uploaded') return 'Uploaded';
  return 'Auth';
}

import type { CodexSession } from './types';

export function formatError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object' && 'message' in err)
    return String((err as { message: unknown }).message);
  return String(err);
}

export function sessionDisplayName(session: CodexSession): string {
  return session.threadName?.trim() || session.summary?.trim() || session.id;
}

export function relaySessionLabel(session: CodexSession): string {
  const displayName = sessionDisplayName(session);
  return [
    session.id,
    displayName !== session.id ? displayName : null,
    session.cwd?.trim() || 'unknown cwd',
  ]
    .filter(Boolean)
    .join(' · ');
}

import { describe, expect, it } from 'vitest';
import { relaySessionLabel } from './format';
import type { CodexSession } from './types';

function session(overrides: Partial<CodexSession> = {}): CodexSession {
  return {
    id: '019fca57-20ce-73f3-89cc-6efac04c44cf',
    accountId: 'main',
    path: '/tmp/session.jsonl',
    modifiedAt: 1,
    sizeBytes: 10,
    cwd: '/repo/LAM',
    threadName: '排查 Clash Verge IP 显示异常',
    summary: null,
    firstUserMessage: null,
    model: null,
    originalProviderId: null,
    originalModel: null,
    currentProviderId: null,
    currentModel: null,
    providerMismatch: false,
    ...overrides,
  };
}

describe('relaySessionLabel', () => {
  it('puts the session ID before the descriptive text and cwd', () => {
    expect(relaySessionLabel(session())).toBe(
      '019fca57-20ce-73f3-89cc-6efac04c44cf · 排查 Clash Verge IP 显示异常 · /repo/LAM',
    );
  });

  it('does not repeat the ID when no separate descriptive text exists', () => {
    expect(relaySessionLabel(session({ threadName: null, summary: null, cwd: null }))).toBe(
      '019fca57-20ce-73f3-89cc-6efac04c44cf · unknown cwd',
    );
  });
});

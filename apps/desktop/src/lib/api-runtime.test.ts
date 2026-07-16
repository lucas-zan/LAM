import { afterEach, describe, expect, it } from 'vitest';
import { inTauri } from './api';

const original = Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__');

function setInternals(value: unknown) {
  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    configurable: true,
    writable: true,
    value,
  });
}

afterEach(() => {
  if (original) Object.defineProperty(window, '__TAURI_INTERNALS__', original);
  else delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe('Tauri runtime detection', () => {
  it.each([undefined, null, {}, { invoke: true }, { invoke: 'not-a-function' }])(
    'rejects an unusable bridge value: %j',
    (internals) => {
      setInternals(internals);
      expect(inTauri()).toBe(false);
    },
  );

  it('accepts only a bridge with an invoke function', () => {
    setInternals({ invoke: () => Promise.resolve() });
    expect(inTauri()).toBe(true);
  });
});

import assert from 'node:assert/strict';
import test from 'node:test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  resolveArtifactName,
  validateG5Manifest,
  verifyG5,
} from './verify-provider-gateway-g5.mjs';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

test('G5 manifest and product wiring are complete', async () => {
  assert.deepEqual(await verifyG5({ desktopRoot }), { scenarios: 10, sourceChecks: 7 });
});

test('G5 rejects an incomplete scenario set', () => {
  assert.throws(
    () =>
      validateG5Manifest({
        schemaVersion: 1,
        gate: 'provider-gateway-g5',
        productInvariant: 'one-account-one-profile-one-codex-home',
        scenarios: [],
        requiredRoutes: ['GET /v1/models', 'POST /v1/responses'],
      }),
    /missing G5 scenario/,
  );
});

test('G5 resolves versioned artifact templates from release metadata', () => {
  assert.equal(
    resolveArtifactName('LAM_{version}_{architecture}.dmg', {
      version: '0.3.0',
      architecture: 'aarch64',
    }),
    'LAM_0.3.0_aarch64.dmg',
  );
});

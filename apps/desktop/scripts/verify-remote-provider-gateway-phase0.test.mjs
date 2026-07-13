import assert from 'node:assert/strict';
import { cp, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { verifyPhaseZero } from './verify-remote-provider-gateway-phase0.mjs';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = path.resolve(desktopRoot, '../..');
const sourceFixtureRoot = path.join(desktopRoot, 'src-tauri/tests/fixtures/codex-gateway-contract');
const sourceApproval = path.join(
  desktopRoot,
  'src-tauri/tests/fixtures/remote-provider-gateway-phase0-approval.json',
);
const sourceReport = path.join(repoRoot, 'docs/remote-provider-gateway-contract-coverage.md');

async function withFixtureCopy(run) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'lam-phase0-gate-'));
  const fixtureRoot = path.join(root, 'contract');
  const approvalPath = path.join(root, 'approval.json');
  const reportPath = path.join(root, 'coverage.md');
  try {
    await cp(sourceFixtureRoot, fixtureRoot, { recursive: true });
    await cp(sourceApproval, approvalPath);
    await cp(sourceReport, reportPath);
    await run({ approvalPath, fixtureRoot, reportPath });
  } finally {
    await rm(root, { force: true, recursive: true });
  }
}

test('accepts the checked-in exact-tested Phase 0 contract', async () => {
  await withFixtureCopy(async (paths) => {
    const result = await verifyPhaseZero(paths);
    assert.equal(result.scenarioCount, 12);
    assert.deepEqual(result.requiredRoutes, ['GET /v1/models', 'POST /v1/responses']);
  });
});

test('rejects a corrupted manifest checksum', async () => {
  await withFixtureCopy(async (paths) => {
    const manifestPath = path.join(paths.fixtureRoot, 'manifest.json');
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    manifest.artifacts[0].fnv1a64 = '0000000000000000';
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

    await assert.rejects(verifyPhaseZero(paths), /checksum mismatch/);
  });
});

test('rejects a synthetic secret marker in an owned artifact', async () => {
  await withFixtureCopy(async (paths) => {
    const artifact = path.join(paths.fixtureRoot, 'observations/normal.json');
    await writeFile(artifact, 'LAM_TEST_SECRET_API_KEY_sk-rpg405-7f3a\n', { flag: 'a' });

    await assert.rejects(verifyPhaseZero(paths), /unsafe fixture content/);
  });
});

test('rejects removal of a mandatory contract case', async () => {
  await withFixtureCopy(async (paths) => {
    const manifestPath = path.join(paths.fixtureRoot, 'manifest.json');
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    manifest.scenarios = manifest.scenarios.filter((scenario) => scenario.id !== 'disconnect');
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

    await assert.rejects(verifyPhaseZero(paths), /missing mandatory scenario: disconnect/);
  });
});

test('rejects an approval that disagrees with the captured routes', async () => {
  await withFixtureCopy(async (paths) => {
    const approval = JSON.parse(await readFile(paths.approvalPath, 'utf8'));
    approval.routes.required = ['POST /v1/responses'];
    await writeFile(paths.approvalPath, `${JSON.stringify(approval, null, 2)}\n`);

    await assert.rejects(verifyPhaseZero(paths), /approval required routes mismatch/);
  });
});

test('rejects a stale generated coverage report', async () => {
  await withFixtureCopy(async (paths) => {
    await writeFile(paths.reportPath, '\nmanual edit\n', { flag: 'a' });

    await assert.rejects(verifyPhaseZero(paths), /coverage report is stale/);
  });
});

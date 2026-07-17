import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { architectureForTriple, readReleaseMetadata } from './release-metadata.mjs';

async function releaseFixture(version) {
  const root = await mkdtemp(path.join(tmpdir(), 'lam-release-metadata-'));
  await mkdir(root, { recursive: true });
  await writeFile(
    path.join(root, 'package.json'),
    `${JSON.stringify({ name: '@localagentmanager/desktop', version }, null, 2)}\n`,
  );
  return root;
}

test('builds the DMG name from the release version and Apple Silicon host', async () => {
  const desktopRoot = await releaseFixture('0.3.0');

  assert.deepEqual(await readReleaseMetadata(desktopRoot, 'aarch64-apple-darwin'), {
    version: '0.3.0',
    triple: 'aarch64-apple-darwin',
    architecture: 'aarch64',
    dmgName: 'LAM_0.3.0_aarch64.dmg',
  });
});

test('supports prerelease versions and Intel macOS artifact names', async () => {
  const desktopRoot = await releaseFixture('1.2.3-beta.1');

  assert.equal(architectureForTriple('x86_64-apple-darwin'), 'x64');
  assert.equal(
    (await readReleaseMetadata(desktopRoot, 'x86_64-apple-darwin')).dmgName,
    'LAM_1.2.3-beta.1_x64.dmg',
  );
});

test('rejects unsupported hosts and invalid package versions', async () => {
  assert.throws(() => architectureForTriple('aarch64-unknown-linux-gnu'), /unsupported/);
  const desktopRoot = await releaseFixture('not-semver');
  await assert.rejects(
    readReleaseMetadata(desktopRoot, 'aarch64-apple-darwin'),
    /invalid release version/,
  );
});

import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const scriptPath = fileURLToPath(new URL('./sync-release-version.mjs', import.meta.url));

async function createFixtureRoot() {
  const root = await mkdtemp(path.join(tmpdir(), 'lam-version-sync-'));
  const desktopDir = path.join(root, 'apps', 'desktop');
  const tauriDir = path.join(desktopDir, 'src-tauri');
  await mkdir(tauriDir, { recursive: true });
  await writeFile(
    path.join(desktopDir, 'package.json'),
    `${JSON.stringify({ name: '@localagentmanager/desktop', version: '0.2.0' }, null, 2)}\n`,
  );
  await writeFile(
    path.join(tauriDir, 'tauri.conf.json'),
    `${JSON.stringify({ productName: 'LAM', version: '0.2.0' }, null, 2)}\n`,
  );
  await writeFile(
    path.join(tauriDir, 'Cargo.toml'),
    [
      '[package]',
      'name = "localagentmanager-core"',
      'version = "0.2.0"',
      'edition = "2021"',
      '',
      '[dependencies]',
      'serde = "1"',
      '',
    ].join('\n'),
  );
  return root;
}

async function readJson(filePath) {
  return JSON.parse(await readFile(filePath, 'utf8'));
}

test('updates package, tauri, and cargo versions under a root', async () => {
  const root = await createFixtureRoot();
  const result = spawnSync(process.execPath, [scriptPath, '--root', root, '1.2.3-beta.1'], {
    encoding: 'utf8',
  });

  assert.equal(result.status, 0, result.stderr);
  assert.equal(
    (await readJson(path.join(root, 'apps', 'desktop', 'package.json'))).version,
    '1.2.3-beta.1',
  );
  assert.equal(
    (await readJson(path.join(root, 'apps', 'desktop', 'src-tauri', 'tauri.conf.json'))).version,
    '1.2.3-beta.1',
  );
  const cargoToml = await readFile(
    path.join(root, 'apps', 'desktop', 'src-tauri', 'Cargo.toml'),
    'utf8',
  );
  assert.match(cargoToml, /^version = "1\.2\.3-beta\.1"$/m);
});

test('rejects invalid version strings', async () => {
  const root = await createFixtureRoot();
  const result = spawnSync(process.execPath, [scriptPath, '--root', root, 'not-a-version'], {
    encoding: 'utf8',
  });

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Invalid version/);
});

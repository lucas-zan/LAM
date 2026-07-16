import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

test('tauri development prepares every Codex runtime component', async () => {
  const packageJson = JSON.parse(await readFile(path.join(desktopRoot, 'package.json'), 'utf8'));
  const script = await readFile(
    path.join(desktopRoot, 'scripts/package-gateway-components.mjs'),
    'utf8',
  );

  assert.match(packageJson.scripts['tauri:dev'], /package-gateway-components\.mjs dev/);
  assert.match(script, /async function prepareDev\(\)/);
  for (const binary of ['lam', 'lam-provider-gateway', 'lam-auth-helper']) {
    assert.match(script, new RegExp(`'--bin',\\s*'${binary}'`));
  }
});

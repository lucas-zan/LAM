#!/usr/bin/env node
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const VERSION_PATTERN = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

function usage() {
  return [
    'Usage: node scripts/sync-release-version.mjs [--root <repo-root>] <version>',
    '',
    'Example:',
    '  node scripts/sync-release-version.mjs 0.3.0',
  ].join('\n');
}

function parseArgs(argv) {
  let root = path.resolve(fileURLToPath(new URL('../../..', import.meta.url)));
  const args = [...argv];

  if (args[0] === '--root') {
    if (!args[1]) {
      throw new Error('Missing value for --root');
    }
    root = path.resolve(args[1]);
    args.splice(0, 2);
  }

  if (args.length !== 1) {
    throw new Error(usage());
  }

  const version = args[0];
  if (!VERSION_PATTERN.test(version)) {
    throw new Error(`Invalid version "${version}". Expected semver like 1.2.3 or 1.2.3-beta.1.`);
  }

  return { root, version };
}

async function updateJsonVersion(filePath, version) {
  const data = JSON.parse(await readFile(filePath, 'utf8'));
  data.version = version;
  await writeFile(filePath, `${JSON.stringify(data, null, 2)}\n`);
}

async function updatePackageLockVersion(filePath, version) {
  const data = JSON.parse(await readFile(filePath, 'utf8'));
  if (!data.packages?.['']) {
    throw new Error(`Could not find root package metadata in ${filePath}`);
  }
  data.version = version;
  data.packages[''].version = version;
  await writeFile(filePath, `${JSON.stringify(data, null, 2)}\n`);
}

async function updateCargoPackageVersion(filePath, version) {
  const original = await readFile(filePath, 'utf8');
  const pattern = /^(\[package\][\s\S]*?^version\s*=\s*)"([^"]+)"/m;
  const current = original.match(pattern);
  if (!current) {
    throw new Error(`Could not find package version in ${filePath}`);
  }
  if (current[2] === version) return;

  const next = original.replace(pattern, `$1"${version}"`);
  await writeFile(filePath, next);
}

export async function syncReleaseVersion({ root, version }) {
  const desktopDir = path.join(root, 'apps', 'desktop');
  await updateJsonVersion(path.join(desktopDir, 'package.json'), version);
  await updatePackageLockVersion(path.join(desktopDir, 'package-lock.json'), version);
  await updateJsonVersion(path.join(desktopDir, 'src-tauri', 'tauri.conf.json'), version);
  await updateCargoPackageVersion(path.join(desktopDir, 'src-tauri', 'Cargo.toml'), version);
}

try {
  const options = parseArgs(process.argv.slice(2));
  await syncReleaseVersion(options);
  console.log(`Synchronized release version to ${options.version}`);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}

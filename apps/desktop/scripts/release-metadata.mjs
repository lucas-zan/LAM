import { readFile } from 'node:fs/promises';
import path from 'node:path';

const VERSION_PATTERN = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

export function architectureForTriple(triple) {
  if (triple === 'aarch64-apple-darwin') return 'aarch64';
  if (triple === 'x86_64-apple-darwin') return 'x64';
  throw new Error(`unsupported Gateway packaging target: ${triple}`);
}

export async function readReleaseMetadata(desktopRoot, triple) {
  const packageJson = JSON.parse(await readFile(path.join(desktopRoot, 'package.json'), 'utf8'));
  const version = packageJson.version;
  if (typeof version !== 'string' || !VERSION_PATTERN.test(version)) {
    throw new Error(`invalid release version in package.json: ${String(version)}`);
  }
  const architecture = architectureForTriple(triple);
  return {
    version,
    triple,
    architecture,
    dmgName: `LAM_${version}_${architecture}.dmg`,
  };
}

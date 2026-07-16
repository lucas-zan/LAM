import { createHash } from 'node:crypto';
import { chmod, copyFile, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const tauriRoot = path.join(desktopRoot, 'src-tauri');
const binariesRoot = path.join(tauriRoot, 'binaries');
const generatedManifest = path.join(tauriRoot, 'generated/provider-gateway-install-manifest.json');
const componentNames = ['lam', 'lam-provider-gateway', 'lam-auth-helper'];

// Stable codesign identifiers per component. The macOS Keychain "Always Allow"
// grant is keyed on the designated requirement (identifier + signing cert), so
// pinning an explicit identifier here — instead of letting codesign derive it
// from the on-disk filename — keeps the grant valid across `make start`,
// `make build`, and `make dmg`, whose binaries live at different paths.
const componentIdentifiers = {
  lam: 'dev.localagentmanager.lam',
  'lam-provider-gateway': 'dev.localagentmanager.provider-gateway',
  'lam-auth-helper': 'dev.localagentmanager.auth-helper',
};

function signComponent(name, file) {
  const identifier = componentIdentifiers[name];
  if (!identifier) {
    throw new Error(`no stable codesign identifier for component: ${name}`);
  }
  run('/usr/bin/codesign', [
    '--force',
    '--identifier',
    identifier,
    '--sign',
    process.env.LAM_CODESIGN_IDENTITY || '-',
    file,
  ]);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? desktopRoot,
    encoding: 'utf8',
    stdio: options.capture ? 'pipe' : 'inherit',
  });
  if (result.status !== 0) {
    throw new Error(`${command} failed with exit ${result.status}`);
  }
  return result.stdout?.trim() ?? '';
}

function hostTriple() {
  const output = run('rustc', ['-vV'], { capture: true });
  const host = output
    .split('\n')
    .find((line) => line.startsWith('host: '))
    ?.slice(6);
  if (!host) throw new Error('rustc did not report a host triple');
  return host;
}

async function sha256(file) {
  return createHash('sha256')
    .update(await readFile(file))
    .digest('hex');
}

function packageIdentity() {
  return process.env.LAM_TEAM_ID || 'adhoc';
}

async function writeManifest(components) {
  await mkdir(path.dirname(generatedManifest), { recursive: true });
  await writeFile(
    generatedManifest,
    `${JSON.stringify({ schemaVersion: 1, components }, null, 2)}\n`,
    { mode: 0o600 },
  );
}

async function prepare() {
  const triple = hostTriple();
  if (triple !== 'aarch64-apple-darwin') {
    throw new Error(`unsupported Gateway packaging target: ${triple}`);
  }
  run('cargo', [
    'build',
    '--manifest-path',
    path.join(tauriRoot, 'Cargo.toml'),
    '--release',
    '--bin',
    'lam',
    '--bin',
    'lam-provider-gateway',
    '--bin',
    'lam-auth-helper',
  ]);
  await mkdir(binariesRoot, { recursive: true, mode: 0o700 });
  const components = [];
  for (const name of componentNames) {
    const source = path.join(tauriRoot, 'target/release', name);
    const staged = path.join(binariesRoot, `${name}-${triple}`);
    await copyFile(source, staged);
    await chmod(staged, 0o700);
    signComponent(name, staged);
    components.push({
      name:
        name === 'lam-provider-gateway'
          ? 'gateway'
          : name === 'lam-auth-helper'
            ? 'auth-helper'
            : 'launcher',
      relativePath: `MacOS/${name}`,
      version: '0.2.1',
      sha256: await sha256(staged),
      protocolVersion: 1,
      stateSchema: 1,
      platform: 'macos',
      architecture: 'aarch64',
      packageIdentity: packageIdentity(),
    });
  }
  await writeManifest(components);
}

async function prepareDev() {
  run('cargo', [
    'build',
    '--manifest-path',
    path.join(tauriRoot, 'Cargo.toml'),
    '--bin',
    'lam',
    '--bin',
    'lam-provider-gateway',
    '--bin',
    'lam-auth-helper',
  ]);
  const devRoot = path.join(tauriRoot, 'target/debug');
  const components = [];
  for (const name of componentNames) {
    const executable = path.join(devRoot, name);
    await chmod(executable, 0o700);
    // A stable signing identity (LAM_CODESIGN_IDENTITY) keeps the macOS Keychain
    // "Always Allow" grant valid across rebuilds. Falling back to ad-hoc ("-")
    // re-derives the code identity from the binary hash on every rebuild, which
    // invalidates the grant and re-prompts for the login password each time.
    signComponent(name, executable);
    components.push({
      name:
        name === 'lam-provider-gateway'
          ? 'gateway'
          : name === 'lam-auth-helper'
            ? 'auth-helper'
            : 'launcher',
      relativePath: name,
      version: '0.2.1',
      sha256: await sha256(executable),
      protocolVersion: 1,
      stateSchema: 1,
      platform: 'macos',
      architecture: 'aarch64',
      packageIdentity: 'adhoc',
    });
  }
  await writeFile(
    path.join(devRoot, 'provider-gateway-install-manifest.json'),
    `${JSON.stringify({ schemaVersion: 1, components }, null, 2)}\n`,
    { mode: 0o600 },
  );
}

async function finalize() {
  const app = path.join(tauriRoot, 'target/release/bundle/macos/LAM.app');
  const contents = path.join(app, 'Contents');
  const components = [];
  for (const name of componentNames) {
    const bundled = path.join(contents, 'MacOS', name);
    signComponent(name, bundled);
    components.push({
      name:
        name === 'lam-provider-gateway'
          ? 'gateway'
          : name === 'lam-auth-helper'
            ? 'auth-helper'
            : 'launcher',
      relativePath: `MacOS/${name}`,
      version: '0.2.1',
      sha256: await sha256(bundled),
      protocolVersion: 1,
      stateSchema: 1,
      platform: 'macos',
      architecture: 'aarch64',
      packageIdentity: packageIdentity(),
    });
  }
  const bundledManifest = path.join(contents, 'Resources/provider-gateway-install-manifest.json');
  await writeFile(
    bundledManifest,
    `${JSON.stringify({ schemaVersion: 1, components }, null, 2)}\n`,
    { mode: 0o600 },
  );
  run('/usr/bin/codesign', ['--force', '--sign', process.env.LAM_CODESIGN_IDENTITY || '-', app]);
  run('/usr/bin/codesign', ['--verify', '--deep', '--strict', app]);
}

async function verify() {
  const triple = hostTriple();
  const manifest = JSON.parse(await readFile(generatedManifest, 'utf8'));
  if (manifest.schemaVersion !== 1 || manifest.components.length !== 3) {
    throw new Error('generated Gateway manifest has an invalid component set');
  }
  for (const component of manifest.components) {
    const binaryName =
      component.name === 'gateway'
        ? 'lam-provider-gateway'
        : component.name === 'auth-helper'
          ? 'lam-auth-helper'
          : 'lam';
    const staged = path.join(binariesRoot, `${binaryName}-${triple}`);
    if ((await sha256(staged)) !== component.sha256) {
      throw new Error(`Gateway component hash mismatch: ${component.name}`);
    }
    run('/usr/bin/codesign', ['--verify', '--strict', staged]);
  }
}

async function verifyBundledApp(app) {
  run('/usr/bin/codesign', ['--verify', '--deep', '--strict', app]);
  const contents = path.join(app, 'Contents');
  const manifest = JSON.parse(
    await readFile(path.join(contents, 'Resources/provider-gateway-install-manifest.json'), 'utf8'),
  );
  if (manifest.schemaVersion !== 1 || manifest.components.length !== 3) {
    throw new Error('bundled Gateway manifest has an invalid component set');
  }
  for (const component of manifest.components) {
    const bundled = path.join(contents, component.relativePath);
    if ((await sha256(bundled)) !== component.sha256) {
      throw new Error(`bundled Gateway component hash mismatch: ${component.name}`);
    }
    run('/usr/bin/codesign', ['--verify', '--strict', bundled]);
  }
}

async function dmg() {
  const release = path.join(tauriRoot, 'target/release/bundle');
  const app = path.join(release, 'macos/LAM.app');
  run('/usr/bin/codesign', ['--verify', '--deep', '--strict', app]);
  const staging = path.join(release, 'gateway-dmg-staging');
  const output = path.join(release, 'dmg/LAM_0.2.1_aarch64.dmg');
  run('/bin/rm', ['-rf', staging]);
  await mkdir(staging, { recursive: true, mode: 0o700 });
  run('/bin/cp', ['-R', app, path.join(staging, 'LAM.app')]);
  run('/bin/ln', ['-s', '/Applications', path.join(staging, 'Applications')]);
  await mkdir(path.dirname(output), { recursive: true });
  run('/usr/bin/hdiutil', [
    'create',
    '-volname',
    'LAM',
    '-srcfolder',
    staging,
    '-ov',
    '-format',
    'UDZO',
    output,
  ]);
  run('/bin/rm', ['-rf', staging]);
  const checksum = await sha256(output);
  await writeFile(`${output}.sha256`, `${checksum}  ${path.basename(output)}\n`, {
    mode: 0o600,
  });

  const mount = path.join(release, `gateway-dmg-mount-${process.pid}`);
  await rm(mount, { recursive: true, force: true });
  await mkdir(mount, { recursive: true, mode: 0o700 });
  let attached = false;
  try {
    run('/usr/bin/hdiutil', ['attach', '-readonly', '-nobrowse', '-mountpoint', mount, output]);
    attached = true;
    await verifyBundledApp(path.join(mount, 'LAM.app'));
    if ((await sha256(output)) !== checksum) {
      throw new Error('DMG checksum changed during verification');
    }
  } finally {
    if (attached) run('/usr/bin/hdiutil', ['detach', mount]);
    await rm(mount, { recursive: true, force: true });
  }
}

const mode = process.argv[2];
if (mode === 'dev') await prepareDev();
else if (mode === 'prepare') await prepare();
else if (mode === 'finalize') await finalize();
else if (mode === 'verify') await verify();
else if (mode === 'dmg') await dmg();
else throw new Error('usage: package-gateway-components.mjs dev|prepare|finalize|verify|dmg');

import { access, readFile, readdir, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const requiredScenarios = [
  'add-exclusive-api-account',
  'explicit-provider-reuse',
  'same-home-model-switch',
  'create-crash-recovery',
  'delete-crash-recovery',
  'provider-aware-relay-no-write',
  'representation-loss-confirmation',
  'structured-redacted-metrics',
  'packaged-launcher-sidecar',
  'app-dmg-integrity',
];

const sourceRequirements = [
  ['src/App.tsx', ['ApiAccountFlow', 'planApiAccountModelSwitchV2', 'deleteApiAccountV2']],
  ['src/components/api-account-flow.tsx', ['planApiAccountV2', 'executeApiAccountV2']],
  ['src-tauri/src/main.rs', ['recover_provider_transactions_service_v2', 'plan_api_account_v2']],
  [
    'src-tauri/src/services/provider_api_v2.rs',
    ['api-account-journal.json', 'CrashAfterDeleteDetach', 'recover_api_account_transactions_at_root_service_v2'],
  ],
  [
    'src-tauri/src/services/provider_relay_compatibility.rs',
    ['CompatibleWithLoss', 'PreviousResponseState', 'classify_codex_relay_history'],
  ],
  [
    'src-tauri/src/services/relay.rs',
    ['RELAY_COMPATIBILITY_BLOCKED', 'confirm_compatibility_loss', 'compatibility_fingerprint'],
  ],
  [
    'src-tauri/src/services/gateway/server.rs',
    ['RequestUsageMetadata', 'retry_count', 'binding_hash'],
  ],
];

export function validateG5Manifest(manifest) {
  if (manifest.schemaVersion !== 1 || manifest.gate !== 'provider-gateway-g5')
    throw new Error('unsupported G5 manifest');
  if (manifest.productInvariant !== 'one-account-one-profile-one-codex-home')
    throw new Error('G5 product invariant mismatch');
  for (const scenario of requiredScenarios)
    if (!manifest.scenarios?.includes(scenario)) throw new Error(`missing G5 scenario: ${scenario}`);
  if (JSON.stringify(manifest.requiredRoutes) !== JSON.stringify(['GET /v1/models', 'POST /v1/responses']))
    throw new Error('G5 route contract mismatch');
}

export async function verifyG5({ desktopRoot, requireArtifacts = false }) {
  const manifestPath = path.join(
    desktopRoot,
    'src-tauri/tests/fixtures/provider-gateway-g5/manifest.json',
  );
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  validateG5Manifest(manifest);
  for (const [relative, markers] of sourceRequirements) {
    const body = await readFile(path.join(desktopRoot, relative), 'utf8');
    for (const marker of markers)
      if (!body.includes(marker)) throw new Error(`${relative} is missing G5 marker: ${marker}`);
  }
  const weeklyUi = await readFile(path.join(desktopRoot, 'src/components/tray-quota-panel.tsx'), 'utf8');
  if (!weeklyUi.includes('scheduleTrayPopoverWindowSize'))
    throw new Error('weekly/tray popover implementation is missing');

  const artifactRoot = path.join(desktopRoot, 'src-tauri/target/release/bundle');
  if (requireArtifacts) {
    const app = path.join(artifactRoot, 'macos', manifest.artifacts[0]);
    const dmg = path.join(artifactRoot, 'dmg', manifest.artifacts[1]);
    await access(app);
    await access(dmg);
    if (!(await stat(dmg)).size) throw new Error('G5 DMG is empty');
    const appEntries = await readdir(path.join(app, 'Contents/MacOS'));
    for (const binary of ['localagentmanager', 'lam', 'lam-auth-helper', 'lam-provider-gateway'])
      if (!appEntries.includes(binary)) throw new Error(`packaged app is missing ${binary}`);
  }
  return { scenarios: manifest.scenarios.length, sourceChecks: sourceRequirements.length };
}

const thisFile = fileURLToPath(import.meta.url);
if (process.argv[1] && path.resolve(process.argv[1]) === thisFile) {
  const desktopRoot = path.resolve(path.dirname(thisFile), '..');
  const result = await verifyG5({
    desktopRoot,
    requireArtifacts: process.argv.includes('--require-artifacts'),
  });
  process.stdout.write(`G5 verified: ${result.scenarios} scenarios, ${result.sourceChecks} source checks\n`);
}

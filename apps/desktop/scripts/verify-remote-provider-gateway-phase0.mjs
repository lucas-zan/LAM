#!/usr/bin/env node

import { readdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';

const mandatoryScenarios = new Map([
  ['text-stream', 'captured'],
  ['text-non-stream', 'unsupported'],
  ['function-tool', 'captured'],
  ['resume', 'captured'],
  ['route-inventory', 'captured'],
  ['auth-helper-trim', 'captured'],
  ['auth-helper-empty', 'captured'],
  ['auth-refresh-401', 'captured'],
  ['retry-429', 'captured'],
  ['retry-500', 'captured'],
  ['malformed-sse', 'captured'],
  ['disconnect', 'captured'],
]);
const requiredRoutes = ['GET /v1/models', 'POST /v1/responses'];
const requiredThreatIds = [
  'port_spoofing',
  'token_exposure',
  'auth_command_execution',
  'ssrf_dns',
  'sensitive_headers',
  'resource_exhaustion',
  'path_symlink',
  'binding_rotation',
  'content_leakage',
  'version_mismatch',
  'unsupported_platform',
];
const unsafePatterns = [
  ['synthetic marker', /LAM_TEST_(?:SECRET|CONTENT)_[A-Z0-9_.-]+/],
  ['API key', /\b(?:sk-|ghp_)[A-Za-z0-9_-]+/],
  ['bearer token', /authorization["']?\s*[:=]\s*["']?Bearer\s+(?!<redacted>)[^\s"']+/i],
  ['personal path', /(?:\/Users\/[^/\s]+|\/home\/[^/\s]+|[A-Z]:\\Users\\[^\\\s]+)/i],
  ['email', /\b[^\s@"']+@[^\s@"']+\b/],
];

function fail(message) {
  throw new Error(`Phase 0 gate: ${message}`);
}

function assertEqual(actual, expected, label) {
  if (!isDeepStrictEqual(actual, expected)) {
    fail(`${label} mismatch`);
  }
}

function assertSafeFixture(body, relativePath) {
  for (const [label, pattern] of unsafePatterns) {
    if (pattern.test(body)) fail(`unsafe fixture content (${label}): ${relativePath}`);
  }
}

function fnv1a64(bytes) {
  let hash = 0xcbf29ce484222325n;
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = BigInt.asUintN(64, hash * 0x100000001b3n);
  }
  return hash.toString(16).padStart(16, '0');
}

async function listFiles(root, relative = '') {
  const entries = await readdir(path.join(root, relative), { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const child = path.posix.join(relative, entry.name);
    if (entry.isDirectory()) files.push(...(await listFiles(root, child)));
    else if (entry.isFile()) files.push(child);
    else fail(`fixture contains non-regular entry: ${child}`);
  }
  return files.sort();
}

function validateManifestContract(manifest) {
  if (manifest.schemaVersion !== 1) fail('unsupported contract manifest schema');
  assertEqual(
    manifest.target,
    {
      architecture: 'arm64',
      captureDate: '2026-07-10',
      codexVersion: '0.144.1',
      osVersion: '15.6',
      platform: 'darwin',
    },
    'exact-tested target',
  );
  assertEqual(
    manifest.supportPolicy,
    { kind: 'exact-tested', versions: ['0.144.1'] },
    'support policy',
  );
  assertEqual(manifest.requiredRouteSet, requiredRoutes, 'required routes');
  assertEqual(manifest.observedRouteSet, requiredRoutes, 'observed routes');
  assertEqual(
    manifest.officialSources,
    [
      'https://learn.chatgpt.com/docs/config-file/config-advanced#custom-model-providers',
      'https://developers.openai.com/api/docs/guides/streaming-responses',
    ],
    'official sources',
  );
  if (manifest.stateMode !== 'full-input') fail('state mode mismatch');
  if (!Array.isArray(manifest.volatileFields) || manifest.volatileFields.length === 0) {
    fail('volatile field policy is missing');
  }

  const scenarios = new Map(manifest.scenarios?.map((scenario) => [scenario.id, scenario]));
  for (const [id, status] of mandatoryScenarios) {
    const scenario = scenarios.get(id);
    if (!scenario) fail(`missing mandatory scenario: ${id}`);
    if (scenario.status !== status) fail(`scenario status mismatch: ${id}`);
    if (scenario.runs !== 2 || scenario.deterministic !== true) {
      fail(`scenario is not deterministic across two runs: ${id}`);
    }
    if (!Array.isArray(scenario.artifacts)) fail(`scenario artifacts missing: ${id}`);
    if (status === 'captured' && scenario.artifacts.length === 0) {
      fail(`captured scenario has no artifacts: ${id}`);
    }
    if (status === 'unsupported' && scenario.artifacts.length !== 0) {
      fail(`unsupported scenario unexpectedly owns artifacts: ${id}`);
    }
  }
  if (scenarios.size !== mandatoryScenarios.size) fail('unexpected scenario set');
}

function validateApproval(approval, manifest) {
  if (approval.schemaVersion !== 1 || approval.gate !== 'remote-provider-gateway-phase0') {
    fail('unsupported approval schema');
  }
  assertEqual(approval.target, manifest.target, 'approval target');
  assertEqual(approval.routes.required, requiredRoutes, 'approval required routes');
  assertEqual(
    approval.routes.unsupported,
    ['GET /v1/responses/{id}', 'POST /v1/responses/{id}', 'POST /v1/responses/{id}/cancel'],
    'approval unsupported routes',
  );
  if (
    approval.retry.codexRequestMaxRetries !== 0 ||
    approval.retry.codexStreamMaxRetries !== 0 ||
    approval.retry.gatewayApplicationRetries !== 0 ||
    approval.retry.maxConnectionAttempts !== 2
  ) {
    fail('approved retry policy mismatch');
  }
  if (
    approval.store.strategy !== 'versioned-file-cross-process-lock-atomic-rename' ||
    approval.store.commitPoint !== 'binding-cas-after-config-commit'
  ) {
    fail('approved store strategy mismatch');
  }
  if (
    approval.credentials.storageTransportSplit !== true ||
    approval.credentials.gatewayBearerRequired !== true ||
    approval.credentials.gatewayTokenStorage !== 'macos-keychain-only'
  ) {
    fail('approved credential model mismatch');
  }
  if (
    approval.state.mode !== manifest.stateMode ||
    approval.state.responseStore !== false ||
    approval.state.previousResponseId !== 'reject'
  ) {
    fail('approved state model mismatch');
  }
  assertEqual(approval.security.threatIds, requiredThreatIds, 'approved threat ids');
}

function renderCoverageReport(manifest, approval) {
  const supported = manifest.scenarios.filter((scenario) => scenario.status === 'captured');
  const unsupported = manifest.scenarios.filter((scenario) => scenario.status === 'unsupported');
  const rows = (items) =>
    items.map((scenario) => `- **\`${scenario.id}\`**: ${scenario.evidence}`).join('\n');
  return `# Remote Provider Gateway Contract Coverage

> Generated by \`pnpm test:gateway-phase0 -- --write-report\`. Do not edit by hand.

## Approved target

- Codex: exact-tested \`${manifest.target.codexVersion}\`
- Platform: \`${manifest.target.platform} ${manifest.target.architecture}\`, OS \`${manifest.target.osVersion}\`
- Capture date: \`${manifest.target.captureDate}\`
- State mode: \`${manifest.stateMode}\`; response store disabled; \`previous_response_id\` rejected

## Route contract

- Required and observed: ${manifest.requiredRouteSet.map((route) => `\`${route}\``).join(', ')}
- Explicitly unsupported: ${approval.routes.unsupported.map((route) => `\`${route}\``).join(', ')}

## Captured behavior

${rows(supported)}

## Explicitly unsupported or unobserved

${rows(unsupported)}

## Approved implementation decisions

- Store: \`${approval.store.strategy}\`; commit point \`${approval.store.commitPoint}\`.
- Credentials: storage and transport are separate; Gateway bearer is mandatory; token storage is \`${approval.credentials.gatewayTokenStorage}\`.
- Retry: Codex request/stream retries \`0/0\`; Gateway application retries \`0\`; at most ${approval.retry.maxConnectionAttempts} connection attempts only before proven delivery.
- Sidecar identity: \`${approval.sidecar.identity}\`.
- MVP platform policy: \`${approval.platform.policy}\`; unsupported platforms are read-only with zero Remote Provider mutation.
- Threat matrix IDs: ${approval.security.threatIds.map((id) => `\`${id}\``).join(', ')}.

## Verification

The offline gate verifies manifest schema, exact target, mandatory cases and routes, two-run determinism, artifact ownership, byte counts, FNV-1a checksums, sanitization markers, approval consistency, and this report's exact generated content. It does not invoke Codex or network.
`;
}

export async function verifyPhaseZero({ fixtureRoot, approvalPath, reportPath }) {
  const manifestPath = path.join(fixtureRoot, 'manifest.json');
  const manifestBody = await readFile(manifestPath, 'utf8');
  const approvalBody = await readFile(approvalPath, 'utf8');
  assertSafeFixture(manifestBody, 'manifest.json');
  assertSafeFixture(approvalBody, path.basename(approvalPath));
  const manifest = JSON.parse(manifestBody);
  const approval = JSON.parse(approvalBody);

  validateManifestContract(manifest);
  validateApproval(approval, manifest);

  const declared = new Map(manifest.artifacts.map((artifact) => [artifact.path, artifact]));
  const referenced = new Set(manifest.scenarios.flatMap((scenario) => scenario.artifacts));
  assertEqual([...referenced].sort(), [...declared.keys()].sort(), 'scenario artifact references');
  const actual = (await listFiles(fixtureRoot)).filter((file) => file !== 'manifest.json');
  assertEqual([...declared.keys()].sort(), actual, 'manifest artifact ownership');

  for (const [relative, artifact] of declared) {
    if (path.isAbsolute(relative) || relative.split('/').includes('..')) {
      fail(`unsafe artifact path: ${relative}`);
    }
    const bytes = await readFile(path.join(fixtureRoot, relative));
    assertSafeFixture(bytes.toString('utf8'), relative);
    if (bytes.byteLength !== artifact.bytes) fail(`byte count mismatch: ${relative}`);
    if (fnv1a64(bytes) !== artifact.fnv1a64) fail(`checksum mismatch: ${relative}`);
  }

  const report = renderCoverageReport(manifest, approval);
  const checkedInReport = await readFile(reportPath, 'utf8');
  assertSafeFixture(checkedInReport, path.basename(reportPath));
  if (checkedInReport !== report) fail('coverage report is stale');

  return {
    artifactCount: declared.size,
    report,
    requiredRoutes,
    scenarioCount: manifest.scenarios.length,
  };
}

function optionValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

async function main() {
  const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const repoRoot = path.resolve(desktopRoot, '../..');
  const fixtureRoot =
    optionValue('--fixture-root') ??
    path.join(desktopRoot, 'src-tauri/tests/fixtures/codex-gateway-contract');
  const approvalPath =
    optionValue('--approval') ??
    path.join(desktopRoot, 'src-tauri/tests/fixtures/remote-provider-gateway-phase0-approval.json');
  const reportPath =
    optionValue('--report') ??
    path.join(repoRoot, 'docs/remote-provider-gateway-contract-coverage.md');

  if (process.argv.includes('--write-report')) {
    const manifest = JSON.parse(await readFile(path.join(fixtureRoot, 'manifest.json'), 'utf8'));
    const approval = JSON.parse(await readFile(approvalPath, 'utf8'));
    validateManifestContract(manifest);
    validateApproval(approval, manifest);
    await writeFile(reportPath, renderCoverageReport(manifest, approval));
  }

  const result = await verifyPhaseZero({ approvalPath, fixtureRoot, reportPath });
  process.stdout.write(
    `Phase 0 gate passed: ${result.scenarioCount} scenarios, ${result.artifactCount} artifacts\n`,
  );
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  main().catch((error) => {
    process.stderr.write(`${error.stack ?? error}\n`);
    process.exitCode = 1;
  });
}

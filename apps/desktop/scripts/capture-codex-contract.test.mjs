import assert from 'node:assert/strict';
import { access, mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  assertDeterministicCapture,
  assertSafeCapture,
  normalizeCapture,
  redactHeaders,
  runProcess,
  sanitizeRequestForFixture,
  writeJsonArtifact,
} from './capture-codex-contract.mjs';

test('normalizes declared volatile values and object ordering', () => {
  const input = {
    z: 'http://127.0.0.1:43123/v1',
    id: 'resp-live-123',
    nested: { cwd: '/private/tmp/codex-capture-a/workspace', a: 1 },
    created_at: 1_783_670_000,
  };

  const normalized = normalizeCapture(input, {
    port: 43123,
    tempRoot: '/private/tmp/codex-capture-a',
  });

  assert.deepEqual(normalized, {
    created_at: '<timestamp>',
    id: '<response-id>',
    nested: { a: 1, cwd: '/fixture/workspace' },
    z: 'http://127.0.0.1:<port>/v1',
  });
});

test('redacts authorization and fixture helper output', () => {
  assert.deepEqual(
    redactHeaders({ Authorization: 'Bearer FIXTURE_AUTH_TOKEN', 'X-Fixture': 'safe' }),
    { authorization: '<redacted>', 'x-fixture': 'safe' },
  );
  assert.equal(normalizeCapture('  FIXTURE_AUTH_TOKEN\n', {}).trim(), '<redacted>');
});

test('rejects real-looking credentials and personal paths', () => {
  for (const unsafe of [
    'sk-example',
    'Bearer value',
    '/Users/person/.codex',
    'name@example.test',
  ]) {
    assert.throws(() => assertSafeCapture(unsafe));
  }
  assert.doesNotThrow(() => assertSafeCapture('/fixture/workspace <redacted>'));
});

test('writes deterministic JSON artifacts with a trailing newline', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'codex-capture-test-'));
  try {
    await writeJsonArtifact(root, 'nested/result.json', { z: 2, a: 1 });
    assert.equal(
      await readFile(path.join(root, 'nested/result.json'), 'utf8'),
      '{\n  "a": 1,\n  "z": 2\n}\n',
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('kills and reaps a child that exceeds its hard timeout', async () => {
  const result = await runProcess(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {
    timeoutMs: 50,
  });

  assert.equal(result.timedOut, true);
  assert.notEqual(result.code, 0);
});

test('kills the full process group when a timed out wrapper spawns a child', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'codex-process-group-test-'));
  const marker = path.join(root, 'orphan-marker');
  const grandchild = `setTimeout(() => require('node:fs').writeFileSync(${JSON.stringify(marker)}, 'orphan'), 200)`;
  const wrapper = `require('node:child_process').spawn(process.execPath, ['-e', ${JSON.stringify(grandchild)}], { stdio: 'ignore' }); setInterval(() => {}, 1000)`;
  try {
    const result = await runProcess(process.execPath, ['-e', wrapper], { timeoutMs: 50 });
    assert.equal(result.timedOut, true);
    await new Promise((resolve) => setTimeout(resolve, 300));
    await assert.rejects(access(marker));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('preserves request structure while redacting generated instruction content', () => {
  const request = {
    body: {
      instructions: 'large built-in instructions',
      input: [
        {
          role: 'developer',
          type: 'message',
          content: [{ type: 'input_text', text: 'generated context' }],
        },
        {
          role: 'user',
          type: 'message',
          content: [{ type: 'input_text', text: 'LAM_CAPTURE_TEXT_STREAM' }],
        },
      ],
    },
    headers: {},
    method: 'POST',
    path: '/v1/responses',
  };

  const sanitized = sanitizeRequestForFixture(request);

  assert.equal(sanitized.body.instructions, '<codex-built-in-instructions>');
  assert.equal(sanitized.body.input[0].content[0].text, '<codex-generated-developer-context>');
  assert.equal(sanitized.body.input[1].content[0].text, 'LAM_CAPTURE_TEXT_STREAM');
  assert.deepEqual(sanitized.normalization.redactedFields, [
    '$.body.input[0].content[0].text',
    '$.body.instructions',
  ]);
});

test('fails closed when normalized capture runs differ', () => {
  assert.doesNotThrow(() => assertDeterministicCapture({ a: [1] }, { a: [1] }, 'same'));
  assert.throws(
    () => assertDeterministicCapture({ a: [1] }, { a: [2] }, 'changed'),
    /changed capture is nondeterministic/,
  );
});

test('keeps one representative tool schema and summarizes the rest', () => {
  const request = {
    body: {
      tools: [
        {
          type: 'function',
          name: 'exec_command',
          description: 'keep',
          parameters: { type: 'object' },
        },
        {
          type: 'function',
          name: 'write_stdin',
          description: 'large',
          parameters: { properties: {} },
        },
        {
          type: 'namespace',
          name: 'multi_agent_v1',
          tools: [{ name: 'spawn', parameters: { type: 'object' } }],
        },
      ],
    },
  };

  const sanitized = sanitizeRequestForFixture(request);

  assert.equal(sanitized.body.tools[0].description, 'keep');
  assert.equal(sanitized.body.tools[1].description, '<tool-description-redacted>');
  assert.deepEqual(sanitized.body.tools[1].parameters, { redacted: '<tool-schema-redacted>' });
  assert.deepEqual(sanitized.body.tools[2].tools, { redacted: '<tool-field-redacted>' });
  assert.equal(sanitized.normalization.toolCount, 3);
  assert.deepEqual(sanitized.normalization.toolNames, [
    'exec_command',
    'write_stdin',
    'multi_agent_v1',
  ]);
});

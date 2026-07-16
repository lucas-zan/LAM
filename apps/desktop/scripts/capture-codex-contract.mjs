#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { chmod, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const fixtureToken = 'FIXTURE_AUTH_TOKEN';
const fixtureModel = 'fixture-model';
const textPrompt = 'LAM_CAPTURE_TEXT_STREAM';
const textResponse = 'LAM_CAPTURE_TEXT_RESPONSE';
const toolPrompt = 'LAM_CAPTURE_FUNCTION_TOOL';
const resumeFirstPrompt = 'LAM_CAPTURE_RESUME_FIRST';
const resumeSecondPrompt = 'LAM_CAPTURE_RESUME_SECOND';
const defaultTimeoutMs = 20_000;

export function fixtureCodexCatalog() {
  return {
    models: [
      {
        slug: fixtureModel,
        display_name: 'Fixture Model',
        supported_reasoning_levels: [],
        shell_type: 'shell_command',
        visibility: 'list',
        supported_in_api: true,
        priority: 1,
        base_instructions:
          'You are a coding agent. Follow developer and user instructions and use available tools carefully.',
        supports_reasoning_summaries: false,
        support_verbosity: false,
        truncation_policy: { mode: 'tokens', limit: 10_000 },
        supports_parallel_tool_calls: false,
        experimental_supported_tools: [],
      },
    ],
  };
}

export function assertNoModelMetadataFallback(result) {
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  if (/Model metadata .* not found|Defaulting to fallback metadata/i.test(output)) {
    throw new Error('Codex used fallback model metadata');
  }
}

function sortValue(value) {
  if (Array.isArray(value)) return value.map(sortValue);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, child]) => [key, sortValue(child)]),
  );
}

function normalizeString(value, context) {
  let normalized = value.replaceAll(fixtureToken, '<redacted>');
  if (context.tempRoot) normalized = normalized.replaceAll(context.tempRoot, '/fixture');
  if (context.port) normalized = normalized.replaceAll(String(context.port), '<port>');
  return normalized
    .replace(/\b[^\s@"']+@[^\s@"']+\b/g, '<email-redacted>')
    .replace(/"turn_started_at_unix_ms":\d+/g, '"turn_started_at_unix_ms":"<timestamp>"')
    .replace(/\bresp[-_][A-Za-z0-9_-]+\b/g, '<response-id>')
    .replace(/\bmsg[-_][A-Za-z0-9_-]+\b/g, '<message-id>')
    .replace(/\bcall[-_][A-Za-z0-9_-]+\b/g, '<call-id>')
    .replace(/\bitem[-_][A-Za-z0-9_-]+\b/g, '<item-id>')
    .replace(/Chunk ID: [A-Za-z0-9_-]+/g, 'Chunk ID: <chunk-id>')
    .replace(/Wall time: [0-9.]+ seconds/g, 'Wall time: <duration> seconds')
    .replace(/\b\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z\b/g, '<timestamp>')
    .replace(/\b[0-9a-f]{8}-[0-9a-f-]{27,}\b/gi, '<uuid>');
}

function normalizeValue(value, context, key = '') {
  if (typeof value === 'string') return normalizeString(value, context);
  if (Array.isArray(value)) return value.map((item) => normalizeValue(item, context));
  if (!value || typeof value !== 'object') {
    return ['created_at', 'timestamp', 'elapsed_ms', 'duration_ms'].includes(key)
      ? '<timestamp>'
      : value;
  }
  return Object.fromEntries(
    Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([childKey, child]) => [
        childKey,
        ['created_at', 'timestamp', 'elapsed_ms', 'duration_ms'].includes(childKey)
          ? '<timestamp>'
          : normalizeValue(child, context, childKey),
      ]),
  );
}

export function normalizeCapture(value, context = {}) {
  return normalizeValue(value, context);
}

export function redactHeaders(headers) {
  const redacted = {};
  for (const [name, value] of Object.entries(headers)) {
    const key = name.toLowerCase();
    redacted[key] = ['authorization', 'cookie', 'set-cookie', 'x-api-key'].includes(key)
      ? '<redacted>'
      : value;
  }
  return sortValue(redacted);
}

function redactGeneratedMessage(message, index, redactedFields) {
  if (!['developer', 'user'].includes(message.role) || !Array.isArray(message.content)) {
    return message;
  }
  const isGenerated =
    message.role === 'developer' ||
    message.content.some((part) => part.text?.startsWith('<environment_context>'));
  if (!isGenerated) return message;
  const marker =
    message.role === 'developer'
      ? '<codex-generated-developer-context>'
      : '<codex-generated-environment-context>';
  return {
    ...message,
    content: message.content.map((part, contentIndex) => {
      if (typeof part.text !== 'string') return part;
      redactedFields.push(`$.body.input[${index}].content[${contentIndex}].text`);
      return { ...part, text: marker };
    }),
  };
}

function sanitizeTools(tools, redactedFields) {
  return tools.map((tool, index) => {
    if (tool.name === 'exec_command') return tool;
    return Object.fromEntries(
      Object.entries(tool).map(([key, value]) => {
        if (['name', 'type'].includes(key) || value === null || typeof value !== 'object') {
          if (key !== 'description') return [key, value];
        }
        redactedFields.push(`$.body.tools[${index}].${key}`);
        if (key === 'description') return [key, '<tool-description-redacted>'];
        if (key === 'parameters') return [key, { redacted: '<tool-schema-redacted>' }];
        return [key, { redacted: '<tool-field-redacted>' }];
      }),
    );
  });
}

export function sanitizeRequestForFixture(request) {
  const copy = structuredClone(request);
  const redactedFields = [];
  if (typeof copy.body?.instructions === 'string') {
    copy.body.instructions = '<codex-built-in-instructions>';
    redactedFields.push('$.body.instructions');
  }
  if (Array.isArray(copy.body?.input)) {
    copy.body.input = copy.body.input.map((message, index) =>
      redactGeneratedMessage(message, index, redactedFields),
    );
  }
  const toolNames = Array.isArray(copy.body?.tools)
    ? copy.body.tools.map((tool) => tool.name ?? tool.type)
    : null;
  const toolKeys = Array.isArray(copy.body?.tools)
    ? copy.body.tools.map((tool) => ({
        keys: Object.keys(tool).sort(),
        name: tool.name ?? tool.type,
      }))
    : null;
  if (toolNames) copy.body.tools = sanitizeTools(copy.body.tools, redactedFields);
  return sortValue({
    ...copy,
    normalization: {
      redactedFields: redactedFields.sort(),
      ...(toolNames ? { toolCount: toolNames.length, toolKeys, toolNames } : {}),
    },
  });
}

export function assertDeterministicCapture(left, right, label) {
  const first = JSON.stringify(sortValue(left));
  const second = JSON.stringify(sortValue(right));
  if (first !== second) throw new Error(`${label} capture is nondeterministic`);
}

export function assertSafeCapture(body) {
  for (const pattern of [
    /\bsk-[A-Za-z0-9_-]+/,
    /\bghp_[A-Za-z0-9_-]+/,
    /Bearer\s+\S+/i,
    /\/Users\/[^/\s]+/,
    /\/home\/[^/\s]+/,
    /[A-Z]:\\Users\\[^\\\s]+/i,
    /\b[^\s@]+@[^\s@]+\b/,
  ]) {
    if (pattern.test(body)) throw new Error(`unsafe capture content matched ${pattern}`);
  }
}

export async function writeJsonArtifact(root, relative, value) {
  const target = path.join(root, relative);
  await mkdir(path.dirname(target), { recursive: true });
  const body = `${JSON.stringify(sortValue(value), null, 2)}\n`;
  assertSafeCapture(body);
  await writeFile(target, body, 'utf8');
}

export function runProcess(command, args, options = {}) {
  const timeoutMs = options.timeoutMs ?? defaultTimeoutMs;
  return new Promise((resolve, reject) => {
    const usesProcessGroup = process.platform !== 'win32';
    const child = spawn(command, args, {
      cwd: options.cwd,
      detached: usesProcessGroup,
      env: options.env,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    let timedOut = false;
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));
    child.on('error', reject);
    const timer = setTimeout(() => {
      timedOut = true;
      try {
        if (usesProcessGroup && child.pid) process.kill(-child.pid, 'SIGKILL');
        else child.kill('SIGKILL');
      } catch (error) {
        if (error.code !== 'ESRCH') reject(error);
      }
    }, timeoutMs);
    child.on('close', (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal, stdout, stderr, timedOut });
    });
    child.stdin.end(options.input ?? '');
  });
}

function responseBase(status, output = [], usage = null, options = {}) {
  return {
    id: options.responseId ?? 'resp-fixture-text',
    object: 'response',
    created_at: 1_700_000_000,
    status,
    background: false,
    billing: { payer: 'developer' },
    completed_at: status === 'completed' ? 1_700_000_001 : null,
    error: null,
    incomplete_details: null,
    instructions: null,
    max_output_tokens: null,
    max_tool_calls: null,
    model: fixtureModel,
    output,
    parallel_tool_calls: true,
    previous_response_id: options.previousResponseId ?? null,
    prompt_cache_key: null,
    prompt_cache_retention: null,
    reasoning: { effort: 'medium', summary: null },
    safety_identifier: null,
    service_tier: 'default',
    store: false,
    temperature: 1,
    text: { format: { type: 'text' }, verbosity: 'medium' },
    tool_choice: 'auto',
    tools: [],
    top_logprobs: 0,
    top_p: 1,
    truncation: 'disabled',
    usage,
    user: null,
    metadata: {},
  };
}

function textMessage(status, text, messageId) {
  return {
    id: messageId,
    type: 'message',
    status,
    role: 'assistant',
    content: text ? [{ type: 'output_text', annotations: [], logprobs: [], text }] : [],
  };
}

export function buildTextEvents(options = {}) {
  const responseId = options.responseId ?? 'resp-fixture-text';
  const messageId = options.messageId ?? 'msg-fixture-text';
  const outputText = options.text ?? textResponse;
  const completeMessage = textMessage('completed', outputText, messageId);
  const usage = {
    input_tokens: 10,
    input_tokens_details: { cached_tokens: 0 },
    output_tokens: 4,
    output_tokens_details: { reasoning_tokens: 0 },
    total_tokens: 14,
  };
  return [
    {
      type: 'response.created',
      sequence_number: 0,
      response: responseBase('in_progress', [], null, { responseId }),
    },
    {
      type: 'response.output_item.added',
      sequence_number: 1,
      output_index: 0,
      item: textMessage('in_progress', '', messageId),
    },
    {
      type: 'response.content_part.added',
      sequence_number: 2,
      output_index: 0,
      item_id: messageId,
      content_index: 0,
      part: { type: 'output_text', annotations: [], logprobs: [], text: '' },
    },
    {
      type: 'response.output_text.delta',
      sequence_number: 3,
      output_index: 0,
      item_id: messageId,
      content_index: 0,
      delta: outputText,
      logprobs: [],
    },
    {
      type: 'response.output_text.done',
      sequence_number: 4,
      output_index: 0,
      item_id: messageId,
      content_index: 0,
      text: outputText,
      logprobs: [],
    },
    {
      type: 'response.content_part.done',
      sequence_number: 5,
      output_index: 0,
      item_id: messageId,
      content_index: 0,
      part: completeMessage.content[0],
    },
    {
      type: 'response.output_item.done',
      sequence_number: 6,
      output_index: 0,
      item: completeMessage,
    },
    {
      type: 'response.completed',
      sequence_number: 7,
      response: responseBase('completed', [completeMessage], usage, { responseId }),
    },
  ];
}

export function buildFunctionToolEvents() {
  const responseId = 'resp-fixture-tool';
  const argumentsJson = JSON.stringify({ cmd: 'printf LAM_CAPTURE_TOOL_OUTPUT', login: false });
  const completedItem = {
    id: 'item-fixture-tool',
    type: 'function_call',
    status: 'completed',
    name: 'exec_command',
    call_id: 'call-fixture-tool',
    arguments: argumentsJson,
  };
  return [
    {
      type: 'response.created',
      sequence_number: 0,
      response: responseBase('in_progress', [], null, { responseId }),
    },
    {
      type: 'response.output_item.added',
      sequence_number: 1,
      output_index: 0,
      item: { ...completedItem, status: 'in_progress', arguments: '' },
    },
    {
      type: 'response.function_call_arguments.delta',
      sequence_number: 2,
      output_index: 0,
      item_id: completedItem.id,
      delta: argumentsJson,
    },
    {
      type: 'response.function_call_arguments.done',
      sequence_number: 3,
      output_index: 0,
      item_id: completedItem.id,
      name: completedItem.name,
      arguments: argumentsJson,
    },
    {
      type: 'response.output_item.done',
      sequence_number: 4,
      output_index: 0,
      item: completedItem,
    },
    {
      type: 'response.completed',
      sequence_number: 5,
      response: responseBase(
        'completed',
        [completedItem],
        {
          input_tokens: 10,
          input_tokens_details: { cached_tokens: 0 },
          output_tokens: 5,
          output_tokens_details: { reasoning_tokens: 0 },
          total_tokens: 15,
        },
        { responseId },
      ),
    },
  ];
}

function sendSse(response, events) {
  response.writeHead(200, {
    'cache-control': 'no-cache',
    connection: 'keep-alive',
    'content-type': 'text/event-stream',
  });
  for (const event of events) {
    response.write(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`);
  }
  response.end();
}

function sendError(response, status, message, retryAfter) {
  const headers = { 'content-type': 'application/json' };
  if (retryAfter !== undefined) headers['retry-after'] = String(retryAfter);
  response.writeHead(status, headers);
  response.end(JSON.stringify({ error: { code: 'fixture_error', message, type: 'fixture' } }));
}

function sendMalformedSse(response) {
  response.writeHead(200, { 'content-type': 'text/event-stream' });
  const created = buildTextEvents()[0];
  response.write(`event: ${created.type}\ndata: ${JSON.stringify(created)}\n\n`);
  response.end('event: response.output_text.delta\ndata: {not-json}\n\n');
}

function sendHangingSse(response, state) {
  response.writeHead(200, { 'content-type': 'text/event-stream' });
  const created = buildTextEvents()[0];
  response.write(`event: ${created.type}\ndata: ${JSON.stringify(created)}\n\n`);
  response.on('close', () => {
    state.clientCloseObserved = true;
  });
}

async function requestBody(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  const raw = Buffer.concat(chunks).toString('utf8');
  if (!raw) return { raw, json: null };
  try {
    return { raw, json: JSON.parse(raw) };
  } catch {
    return { raw, json: null };
  }
}

async function handleRequest(request, response, state) {
  const body = await requestBody(request);
  state.requests.push({
    method: request.method,
    path: request.url,
    headers: redactHeaders(request.headers),
    body: body.json ?? body.raw,
  });
  state.authorizationChecks.push(request.headers.authorization === `Bearer ${fixtureToken}`);
  if (request.method === 'POST' && request.url?.replace(/\?.*$/, '').endsWith('/responses')) {
    state.responseAttempts += 1;
    if (state.scenario === 'auth-refresh-401' && state.responseAttempts === 1) {
      sendError(response, 401, 'fixture unauthorized');
      return;
    }
    if (state.scenario === 'retry-429') {
      sendError(response, 429, 'fixture rate limited', 0);
      return;
    }
    if (state.scenario === 'retry-500') {
      sendError(response, 500, 'fixture server error');
      return;
    }
    if (state.scenario === 'malformed-sse') {
      sendMalformedSse(response);
      return;
    }
    if (state.scenario === 'disconnect') {
      sendHangingSse(response, state);
      return;
    }
    const responseIndex = state.eventBatches.length;
    const events =
      state.scenario === 'function-tool' && responseIndex === 0
        ? buildFunctionToolEvents()
        : buildTextEvents({
            responseId: `resp-fixture-${state.scenario}-${responseIndex + 1}`,
            messageId: `msg-fixture-${state.scenario}-${responseIndex + 1}`,
            text: state.scenario === 'function-tool' ? 'LAM_CAPTURE_TOOL_DONE' : textResponse,
          });
    state.eventBatches.push(events);
    sendSse(response, events);
    return;
  }
  if (request.method === 'GET' && request.url?.replace(/\?.*$/, '').endsWith('/models')) {
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify(fixtureCodexCatalog()));
    return;
  }
  response.writeHead(404, { 'content-type': 'application/json' });
  response.end(JSON.stringify({ error: { message: 'fixture route not found' } }));
}

export async function startCaptureServer(options = {}) {
  const state = {
    authorizationChecks: [],
    clientCloseObserved: false,
    requests: [],
    eventBatches: [],
    responseAttempts: 0,
    scenario: options.scenario ?? 'text-stream',
  };
  const server = createServer((request, response) => {
    handleRequest(request, response, state).catch((error) => {
      response.writeHead(500, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ error: { message: error.message } }));
    });
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  return {
    port: address.port,
    state,
    close: () => {
      server.closeAllConnections?.();
      return new Promise((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      );
    },
  };
}

function helperSource() {
  return `import { appendFile } from 'node:fs/promises';
const [logPath, mode] = process.argv.slice(2);
let stdin = '';
for await (const chunk of process.stdin) stdin += chunk;
await appendFile(logPath, JSON.stringify({ mode, stdinBytes: Buffer.byteLength(stdin) }) + '\\n');
if (mode === 'empty') process.stdout.write('\\n');
else process.stdout.write('  ${fixtureToken}  \\n');
`;
}

function configSource(port, helperPath, helperLog, helperMode, options = {}) {
  const quoted = (value) => JSON.stringify(value);
  const retryConfig =
    options.disableRetries === false ? '' : 'request_max_retries = 0\nstream_max_retries = 0\n';
  return `model = ${quoted(fixtureModel)}
model_provider = "lam_capture"
check_for_update_on_startup = false
web_search = "disabled"
${retryConfig}

[features]
apps = false
plugin_sharing = false
plugins = false
remote_plugin = false

[model_providers.lam_capture]
name = "LAM Codex Contract Capture"
base_url = "http://127.0.0.1:${port}/v1"
wire_api = "responses"

[model_providers.lam_capture.auth]
command = ${quoted(process.execPath)}
args = [${quoted(helperPath)}, ${quoted(helperLog)}, ${quoted(helperMode)}]
timeout_ms = 2000
refresh_interval_ms = 0
`;
}

async function createCaptureEnvironment(port, options = {}) {
  const helperMode = options.helperMode ?? 'trim';
  const root = await realpath(await mkdtemp(path.join(os.tmpdir(), 'lam-codex-contract-')));
  const codexHome = path.join(root, 'codex-home');
  const workspace = path.join(root, 'workspace');
  const helperPath = path.join(root, 'fixture-auth-helper.mjs');
  const helperLog = path.join(root, 'helper-invocations.jsonl');
  await mkdir(codexHome, { recursive: true });
  await mkdir(workspace, { recursive: true });
  await writeFile(helperPath, helperSource(), 'utf8');
  await chmod(helperPath, 0o700);
  await writeFile(
    path.join(codexHome, 'config.toml'),
    configSource(port, helperPath, helperLog, helperMode, options),
    'utf8',
  );
  return { root, codexHome, workspace, helperLog };
}

function captureEnv(environment) {
  const blockedProxy = 'http://127.0.0.1:9';
  return {
    ALL_PROXY: blockedProxy,
    AWS_EC2_METADATA_DISABLED: 'true',
    PATH: process.env.PATH ?? '/usr/bin:/bin',
    HOME: environment.root,
    CODEX_HOME: environment.codexHome,
    LANG: 'C.UTF-8',
    GIT_TERMINAL_PROMPT: '0',
    HTTPS_PROXY: blockedProxy,
    HTTP_PROXY: blockedProxy,
    NO_COLOR: '1',
    NO_PROXY: '127.0.0.1,localhost',
    TMPDIR: environment.root,
  };
}

async function readHelperLog(logPath) {
  try {
    const body = await readFile(logPath, 'utf8');
    return body
      .trim()
      .split('\n')
      .filter(Boolean)
      .map((line) => JSON.parse(line));
  } catch (error) {
    if (error.code === 'ENOENT') return [];
    throw error;
  }
}

function codexArgs(workspace, prompt, ephemeral = true) {
  return [
    'exec',
    '--json',
    ...(ephemeral ? ['--ephemeral'] : []),
    '--ignore-rules',
    '--skip-git-repo-check',
    '--sandbox',
    'read-only',
    '-C',
    workspace,
    prompt,
  ];
}

async function verifyTarget(codexBin) {
  const version = await runProcess(codexBin, ['--version'], { timeoutMs: 5000 });
  if (version.code !== 0 || !version.stdout.includes('codex-cli 0.144.1')) {
    throw new Error(`expected codex-cli 0.144.1, received ${version.stdout || version.stderr}`);
  }
  if (process.platform !== 'darwin' || process.arch !== 'arm64') {
    throw new Error(`expected darwin arm64, received ${process.platform} ${process.arch}`);
  }
}

export async function captureTextStream(options = {}) {
  const codexBin = options.codexBin ?? process.env.CODEX_BIN ?? 'codex';
  await verifyTarget(codexBin);
  const server = await startCaptureServer();
  const environment = await createCaptureEnvironment(server.port);
  try {
    const result = await runProcess(codexBin, codexArgs(environment.workspace, textPrompt), {
      cwd: environment.workspace,
      env: captureEnv(environment),
      timeoutMs: options.timeoutMs ?? defaultTimeoutMs,
    });
    const context = { port: server.port, tempRoot: environment.root };
    return {
      result: normalizeCapture(result, context),
      requests: normalizeCapture(server.state.requests, context),
      events: normalizeCapture(server.state.eventBatches[0], context),
      helperInvocations: normalizeCapture(await readHelperLog(environment.helperLog), context),
    };
  } finally {
    await server.close();
    await rm(environment.root, { recursive: true, force: true });
  }
}

export async function captureFunctionTool(options = {}) {
  const codexBin = options.codexBin ?? process.env.CODEX_BIN ?? 'codex';
  await verifyTarget(codexBin);
  const server = await startCaptureServer({ scenario: 'function-tool' });
  const environment = await createCaptureEnvironment(server.port);
  try {
    const result = await runProcess(codexBin, codexArgs(environment.workspace, toolPrompt), {
      cwd: environment.workspace,
      env: captureEnv(environment),
      timeoutMs: options.timeoutMs ?? defaultTimeoutMs,
    });
    const context = { port: server.port, tempRoot: environment.root };
    return {
      result: normalizeCapture(result, context),
      requests: normalizeCapture(server.state.requests, context),
      eventBatches: normalizeCapture(server.state.eventBatches, context),
      helperInvocations: normalizeCapture(await readHelperLog(environment.helperLog), context),
    };
  } finally {
    await server.close();
    await rm(environment.root, { recursive: true, force: true });
  }
}

function threadIdFrom(stdout) {
  for (const line of stdout.trim().split('\n')) {
    const event = JSON.parse(line);
    if (event.type === 'thread.started') return event.thread_id;
  }
  throw new Error('Codex did not emit thread.started');
}

function resumeArgs(threadId) {
  return [
    'exec',
    'resume',
    '--json',
    '--ignore-rules',
    '--skip-git-repo-check',
    threadId,
    resumeSecondPrompt,
  ];
}

export async function captureResume(options = {}) {
  const codexBin = options.codexBin ?? process.env.CODEX_BIN ?? 'codex';
  await verifyTarget(codexBin);
  const server = await startCaptureServer({ scenario: 'resume' });
  const environment = await createCaptureEnvironment(server.port);
  const processOptions = {
    cwd: environment.workspace,
    env: captureEnv(environment),
    timeoutMs: options.timeoutMs ?? defaultTimeoutMs,
  };
  try {
    const first = await runProcess(
      codexBin,
      codexArgs(environment.workspace, resumeFirstPrompt, false),
      processOptions,
    );
    if (first.code !== 0) throw new Error(`initial resume turn failed: ${first.stderr}`);
    const second = await runProcess(
      codexBin,
      resumeArgs(threadIdFrom(first.stdout)),
      processOptions,
    );
    const context = { port: server.port, tempRoot: environment.root };
    return {
      first: normalizeCapture(first, context),
      second: normalizeCapture(second, context),
      requests: normalizeCapture(server.state.requests, context),
      eventBatches: normalizeCapture(server.state.eventBatches, context),
      helperInvocations: normalizeCapture(await readHelperLog(environment.helperLog), context),
    };
  } finally {
    await server.close();
    await rm(environment.root, { recursive: true, force: true });
  }
}

function scenarioPrompt(scenario) {
  return `LAM_CAPTURE_${scenario.toUpperCase().replaceAll('-', '_')}`;
}

export async function captureScriptedFailure(scenario, options = {}) {
  const codexBin = options.codexBin ?? process.env.CODEX_BIN ?? 'codex';
  await verifyTarget(codexBin);
  const server = await startCaptureServer({ scenario });
  const environment = await createCaptureEnvironment(server.port, {
    disableRetries: false,
    helperMode: options.helperMode ?? 'trim',
  });
  try {
    const result = await runProcess(
      codexBin,
      codexArgs(environment.workspace, scenarioPrompt(scenario)),
      {
        cwd: environment.workspace,
        env: captureEnv(environment),
        timeoutMs: options.timeoutMs ?? defaultTimeoutMs,
      },
    );
    const context = { port: server.port, tempRoot: environment.root };
    return normalizeCapture(
      {
        authorizationChecks: server.state.authorizationChecks,
        clientCloseObserved: server.state.clientCloseObserved,
        helperInvocations: await readHelperLog(environment.helperLog),
        requests: server.state.requests,
        responseAttempts: server.state.responseAttempts,
        result,
      },
      context,
    );
  } finally {
    await server.close();
    await rm(environment.root, { recursive: true, force: true });
  }
}

function resultOutcome(result) {
  if (result.timedOut) return 'timeout-cancelled';
  return result.code === 0 ? 'success' : 'codex-error';
}

function stdinBytes(capture) {
  return Math.max(0, ...capture.helperInvocations.map((item) => item.stdinBytes));
}

function authHeaderObservation(capture) {
  return capture.authorizationChecks.every(Boolean) ? '<redacted>' : '<invalid>';
}

function failureArtifacts(captures) {
  return {
    'observations/auth-helper.json': {
      empty: {
        authorizationHeader: authHeaderObservation(captures.empty),
        helperInvocations: captures.empty.helperInvocations.length,
        httpRequests: captures.empty.requests.length,
        outcome: resultOutcome(captures.empty.result),
        reportsEmptyToken: /empty token/i.test(
          captures.empty.result.stderr + captures.empty.result.stdout,
        ),
        stdinBytes: stdinBytes(captures.empty),
      },
      refresh401: {
        authorizationHeader: authHeaderObservation(captures.refresh401),
        helperInvocations: captures.refresh401.helperInvocations.length,
        httpRequests: captures.refresh401.responseAttempts,
        outcome: resultOutcome(captures.refresh401.result),
        stdinBytes: stdinBytes(captures.refresh401),
      },
      trim: {
        authorizationHeader: authHeaderObservation(captures.trim),
        helperInvocations: captures.trim.helperInvocations.length,
        httpRequests: captures.trim.requests.length,
        outcome: resultOutcome(captures.trim.result),
        stdinBytes: stdinBytes(captures.trim),
      },
    },
    'observations/retries.json': {
      malformedSse: {
        outcome: resultOutcome(captures.malformed.result),
        requestAttempts: captures.malformed.responseAttempts,
      },
      status429: {
        outcome: resultOutcome(captures.status429.result),
        requestAttempts: captures.status429.responseAttempts,
        retryAfter: '0',
      },
      status500: {
        outcome: resultOutcome(captures.status500.result),
        requestAttempts: captures.status500.responseAttempts,
      },
    },
    'observations/disconnect.json': {
      clientCloseObserved: captures.disconnect.clientCloseObserved,
      codexExit: resultOutcome(captures.disconnect.result),
      requestAttempts: captures.disconnect.responseAttempts,
    },
  };
}

async function captureFailureRun() {
  return failureArtifacts({
    disconnect: await captureScriptedFailure('disconnect', { timeoutMs: 500 }),
    empty: await captureScriptedFailure('auth-helper-empty', { helperMode: 'empty' }),
    malformed: await captureScriptedFailure('malformed-sse'),
    refresh401: await captureScriptedFailure('auth-refresh-401'),
    status429: await captureScriptedFailure('retry-429'),
    status500: await captureScriptedFailure('retry-500', { timeoutMs: 60_000 }),
    trim: await captureScriptedFailure('auth-helper-trim'),
  });
}

async function writeFailureCapture(output) {
  const first = await captureFailureRun();
  const second = await captureFailureRun();
  assertDeterministicCapture(first, second, 'failure-path');
  for (const [relative, value] of Object.entries(first)) {
    await writeJsonArtifact(output, relative, value);
  }
  await writeJsonArtifact(output, 'observations/failure-determinism.json', {
    failurePathRuns: 2,
    normalizedMatch: true,
  });
}

function optionValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function responseRequests(requests) {
  return requests.filter(
    (request) =>
      request.method === 'POST' && request.path.replace(/\?.*$/, '').endsWith('/responses'),
  );
}

function assertSuccessful(result, label) {
  if (result.code !== 0) throw new Error(`${label} failed: ${result.stderr || result.stdout}`);
  assertNoModelMetadataFallback(result);
}

async function writeTextCapture(output) {
  const capture = await captureTextStream();
  const responseRequest = responseRequests(capture.requests)[0];
  if (!responseRequest) throw new Error('Codex did not call the Responses endpoint');
  assertSuccessful(capture.result, 'text-stream Codex');
  await writeJsonArtifact(
    output,
    'normal/text-stream-request.json',
    sanitizeRequestForFixture(responseRequest),
  );
  await writeJsonArtifact(output, 'normal/text-stream-events.json', capture.events);
  await writeJsonArtifact(output, 'observations/text-stream-dry-run.json', {
    helperInvocations: capture.helperInvocations,
    result: capture.result,
    routes: capture.requests.map((request) => `${request.method} ${request.path}`),
  });
}

async function writeFunctionCapture(output) {
  const capture = await captureFunctionTool();
  assertSuccessful(capture.result, 'function-tool Codex');
  const requests = responseRequests(capture.requests);
  if (requests.length !== 2)
    throw new Error(`expected two tool requests, received ${requests.length}`);
  await writeJsonArtifact(
    output,
    'normal/function-tool-initial-request.json',
    sanitizeRequestForFixture(requests[0]),
  );
  await writeJsonArtifact(
    output,
    'normal/function-tool-followup-request.json',
    sanitizeRequestForFixture(requests[1]),
  );
  await writeJsonArtifact(output, 'observations/function-tool.json', {
    eventBatches: capture.eventBatches,
    helperInvocations: capture.helperInvocations,
    result: capture.result,
  });
}

async function writeResumeCapture(output) {
  const capture = await captureResume();
  assertSuccessful(capture.first, 'resume first turn');
  assertSuccessful(capture.second, 'resume second turn');
  const requests = responseRequests(capture.requests);
  if (requests.length !== 2)
    throw new Error(`expected two resume requests, received ${requests.length}`);
  await writeJsonArtifact(
    output,
    'normal/resume-initial-request.json',
    sanitizeRequestForFixture(requests[0]),
  );
  await writeJsonArtifact(
    output,
    'normal/resume-request.json',
    sanitizeRequestForFixture(requests[1]),
  );
  await writeJsonArtifact(output, 'observations/resume.json', {
    eventBatches: capture.eventBatches,
    first: capture.first,
    helperInvocations: capture.helperInvocations,
    second: capture.second,
  });
}

function routeKey(request) {
  return `${request.method} ${request.path.replace(/\?.*$/, '')}`;
}

function normalArtifacts(text, tool, resume) {
  const textRequests = responseRequests(text.requests);
  const toolRequests = responseRequests(tool.requests);
  const resumeRequests = responseRequests(resume.requests);
  const allRequests = [...text.requests, ...tool.requests, ...resume.requests];
  return {
    'normal/text-stream-request.json': sanitizeRequestForFixture(textRequests[0]),
    'normal/text-stream-events.json': text.events,
    'normal/function-tool-initial-request.json': sanitizeRequestForFixture(toolRequests[0]),
    'normal/function-tool-followup-request.json': sanitizeRequestForFixture(toolRequests[1]),
    'normal/function-tool-events.json': tool.eventBatches,
    'normal/resume-initial-request.json': sanitizeRequestForFixture(resumeRequests[0]),
    'normal/resume-request.json': sanitizeRequestForFixture(resumeRequests[1]),
    'normal/resume-events.json': resume.eventBatches,
    'observations/route-inventory.json': {
      notObserved: ['GET /v1/responses/{response_id}', 'POST /v1/responses/{response_id}/cancel'],
      observed: [...new Set(allRequests.map(routeKey))].sort(),
    },
    'observations/model-catalog.json': {
      catalog: fixtureCodexCatalog(),
      fallbackMetadataWarning: false,
      routes: [...new Set(allRequests.map(routeKey).filter((route) => route.includes('/models')))].sort(),
    },
    'observations/normal.json': {
      functionTool: {
        helperInvocations: tool.helperInvocations.length,
        requestCount: toolRequests.length,
        resultCode: tool.result.code,
      },
      resume: {
        firstResultCode: resume.first.code,
        helperInvocations: resume.helperInvocations.length,
        previousResponseId: resumeRequests[1].body.previous_response_id ?? null,
        requestCount: resumeRequests.length,
        secondResultCode: resume.second.code,
        stateMode: 'full-input',
      },
      textStream: {
        helperInvocations: text.helperInvocations.length,
        requestCount: textRequests.length,
        resultCode: text.result.code,
        stream: textRequests[0].body.stream,
      },
    },
  };
}

async function captureNormalRun() {
  return normalArtifacts(
    await captureTextStream(),
    await captureFunctionTool(),
    await captureResume(),
  );
}

async function writeNormalCapture(output) {
  const first = await captureNormalRun();
  const second = await captureNormalRun();
  assertDeterministicCapture(first, second, 'normal-path');
  for (const [relative, value] of Object.entries(first)) {
    await writeJsonArtifact(output, relative, value);
  }
  await writeJsonArtifact(output, 'observations/determinism.json', {
    normalPathRuns: 2,
    normalizedMatch: true,
  });
}

async function main() {
  const output = optionValue('--output');
  const scenario = optionValue('--scenario') ?? 'text-stream';
  if (!output) throw new Error('--output is required');
  if (scenario === 'text-stream') return writeTextCapture(output);
  if (scenario === 'function-tool') return writeFunctionCapture(output);
  if (scenario === 'resume') return writeResumeCapture(output);
  if (scenario === 'normal') return writeNormalCapture(output);
  if (scenario === 'failures') return writeFailureCapture(output);
  throw new Error(`scenario not implemented yet: ${scenario}`);
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  main().catch((error) => {
    process.stderr.write(`${error.stack ?? error}\n`);
    process.exitCode = 1;
  });
}

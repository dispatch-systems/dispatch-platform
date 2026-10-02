// Sign in with Dispatch, proved with the real apps. Each MCP client installed here signs in
// to a private Dispatch server the way its user would, with no key: it adds the server and
// prints the sign-in link, a browser signs in as the platform owner and approves it, and the
// app finishes on its own. Then the app connects and calls a tool, and the owner's Connected
// apps list shows it. Every app runs in a temporary home of its own, so the user's own
// configuration and sign-ins are never read or changed. It runs here on demand, never in CI.
//
//   npm run agents:signin -- [--clients claude,codex] [--hermes] [--mcp-remote] [--paste]
//                            [--build | --artifact <dir>] [--keep]
//
// By default each app takes the approval back on its own listener, as on the user's own
// computer, after the command the Connect tab gives. --paste takes the remote (SSH) way: the
// browser stops at the address the app is sent back to, and that address is pasted into the
// app. --hermes adds Hermes Agent (if `hermes` is on PATH) and --mcp-remote adds the
// mcp-remote bridge (through npx).
import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import type { AddressInfo } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { chromium, type Browser } from '@playwright/test';
import { built, demo, fixture } from '../testing/fixture-server.js';

const { values } = parseArgs({
  options: {
    clients: { type: 'string', default: 'claude,codex' },
    hermes: { type: 'boolean', default: false },
    'mcp-remote': { type: 'boolean', default: false },
    paste: { type: 'boolean', default: false },
    build: { type: 'boolean', default: false },
    /** Another build to serve, such as another checkout's `.build`. */
    artifact: { type: 'string', default: built.env.DISPATCH_ARTIFACT_ROOT },
    keep: { type: 'boolean', default: false },
  },
});
const clients = values.clients!.split(',').filter(Boolean);
if (values.hermes) clients.push('hermes');
if (values['mcp-remote']) clients.push('mcp-remote');
const paste = values.paste!;
/** The DSP every app is approved for, which its tool call must name. */
const DSP = 'Northline Logistics';
const SERVER = 'dispatch';

type Result = { client: string; steps: [string, boolean | null, string][] };

/** Tokens, codes and keys never reach the output. */
const redact = (text: string) =>
  text
    .replace(/\bds[kar]_(?:live|dev)_[A-Za-z0-9]+/g, '[token]')
    .replace(/([?&#](?:code|access_token|refresh_token)=)[^&\s"']+/g, '$1[redacted]');
/** A terminal's output as text: no colours, cursor moves or carriage returns. */
const plain = (text: string) =>
  text
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '')
    .replace(/\x1b\[[0-9;?<>=]*[ -/]*[@-~]/g, '')
    .replace(/\x1b[()][0-9A-Za-z]|\x1b[=>78DEHM]/g, '')
    .replace(/\r/g, '');
const quote = (arg: string) => `'${arg.replaceAll("'", `'\\''`)}'`;
const tail = (text: string, lines = 12) => redact(text.trim().split('\n').slice(-lines).join('\n'));

/**
 * A command on a terminal of its own, as the apps' sign-in needs, read as it goes. Without
 * `pty` it runs on plain pipes instead, as an MCP server over stdio does, its stdout apart.
 */
class Terminal {
  text = '';
  stdout = '';
  readonly child: ChildProcess;
  readonly done: Promise<number | null>;
  private readonly waiting = new Set<() => void>();
  constructor(args: string[], env: NodeJS.ProcessEnv, cwd: string, pty = true) {
    // A wide terminal, so no app wraps the long sign-in link across lines.
    const command = `stty cols 4000 rows 50 2>/dev/null; exec ${args.map(quote).join(' ')}`;
    this.child = spawn(
      pty ? 'script' : args[0]!,
      pty ? ['-qefc', command, '/dev/null'] : args.slice(1),
      { env, cwd, stdio: ['pipe', 'pipe', 'pipe'], detached: true },
    );
    const read = (out: boolean) => (chunk: Buffer) => {
      this.text += plain(String(chunk));
      if (out) this.stdout += String(chunk);
      for (const wake of this.waiting) wake();
    };
    this.child.stdout!.on('data', read(true));
    this.child.stderr!.on('data', read(false));
    this.done = new Promise((resolve) =>
      this.child.on('close', (code) => {
        resolve(code);
        for (const wake of this.waiting) wake();
      }),
    );
  }
  get running() {
    return this.child.exitCode === null && this.child.signalCode === null;
  }
  /** The first match of `pattern` in the output (or stdout alone), waiting up to `ms`. */
  until(pattern: RegExp, ms = 60_000, stdout = false) {
    return new Promise<RegExpMatchArray>((resolve, reject) => {
      const check = () => {
        const found = (stdout ? this.stdout : this.text).match(pattern);
        if (found || !this.running) {
          clearTimeout(timer);
          this.waiting.delete(check);
          if (found) resolve(found);
          else reject(new Error(`exited before ${pattern}:\n${tail(this.text)}`));
        }
      };
      const timer = setTimeout(() => {
        this.waiting.delete(check);
        reject(new Error(`no ${pattern} within ${ms / 1000}s:\n${tail(this.text)}`));
      }, ms);
      this.waiting.add(check);
      check();
    });
  }
  type(text: string) {
    this.child.stdin!.write(text);
  }
  /** Ends the command and everything it started, if it is still running. */
  async stop(grace = 3000) {
    if (!this.running) return;
    await Promise.race([this.done, new Promise((r) => setTimeout(r, grace))]);
    for (const signal of ['SIGTERM', 'SIGKILL'] as const) {
      if (!this.running) return;
      try {
        process.kill(-this.child.pid!, signal);
      } catch {}
      await Promise.race([this.done, new Promise((r) => setTimeout(r, 2000))]);
    }
  }
}

/** Whether a command is installed. */
const onPath = (command: string) =>
  (process.env.PATH ?? '')
    .split(path.delimiter)
    .some((dir) => dir && fs.existsSync(path.join(dir, command)));

/** A command run to its end, with what it printed. */
function run(args: string[], env: NodeJS.ProcessEnv, cwd: string, ms = 60_000) {
  return new Promise<{ code: number | null; output: string }>((resolve) => {
    const child = spawn(args[0]!, args.slice(1), { env, cwd, stdio: ['ignore', 'pipe', 'pipe'] });
    let output = '';
    child.stdout.on('data', (chunk) => (output += chunk));
    child.stderr.on('data', (chunk) => (output += chunk));
    const timer = setTimeout(() => child.kill('SIGKILL'), ms);
    child.on('error', (error) => {
      clearTimeout(timer);
      resolve({ code: null, output: `${output}\n${error.message}` });
    });
    child.on('close', (code) => {
      clearTimeout(timer);
      resolve({ code, output: plain(output) });
    });
  });
}

/**
 * The owner's side, in a browser: open the app's sign-in link, sign in, approve it for the
 * one DSP with the Essential tools, and answer where the browser is sent back to the app.
 * With `stop`, the browser goes no further than that address, as when the app is elsewhere.
 */
async function approve(browser: Browser, link: string, stop: boolean, shot?: string) {
  const redirect = new URL(new URL(link).searchParams.get('redirect_uri')!);
  const back = `${redirect.origin}${redirect.pathname}?`;
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  try {
    const page = await context.newPage();
    // As in the browser tests: the sign-in van never loads, which only costs time here.
    await page.route('**/renderer-*.js', (route) =>
      route.fulfill({ contentType: 'text/javascript', body: 'export function startVan() {}\n' }),
    );
    if (stop)
      await page.route(`${redirect.origin}/**`, (route) =>
        route.fulfill({ contentType: 'text/html', body: '<p>Copy this address.</p>' }),
      );
    await page.goto(link);
    await page.getByLabel('Email address').fill(demo.email);
    await page.getByLabel('Password', { exact: true }).fill(demo.password);
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    const form = page
      .getByRole('form')
      .filter({ has: page.getByRole('button', { name: 'Approve', exact: true }) });
    await form.getByText('Choose DSPs', { exact: true }).click();
    await form.getByRole('checkbox', { name: DSP }).check();
    await form.getByRole('button', { name: 'Essential', exact: true }).click();
    // Only an app Dispatch doesn't know is called unverified, in words, on the page.
    const verified = (await form.getByText(/Unverified app/).count()) === 0;
    if (shot) await page.screenshot({ path: shot });
    const sent = page.waitForRequest((request) => request.url().startsWith(back), {
      timeout: 30_000,
    });
    await form.getByRole('button', { name: 'Approve', exact: true }).click();
    const url = (await sent).url();
    // Let the app's own listener answer the browser before the page goes.
    if (!stop) await page.waitForLoadState('load').catch(() => {});
    return { url, verified };
  } finally {
    await context.close();
  }
}

/**
 * A stand-in for the apps' model, since an app in a home of its own has no subscription
 * signed in. It speaks just enough of Anthropic's Messages API and OpenAI's Responses and
 * Chat Completions APIs to ask for Dispatch's whoami tool once (through the app's tool
 * search when it defers its tools) and then to repeat what the tool answered. So the call
 * itself is the app's own, over its own sign-in; only the choice to make it is scripted.
 */
async function standIn(log?: string) {
  /** What the tool answered, as each app handed it back. */
  const returned: string[] = [];
  const sse = (events: [string, unknown][]) =>
    events.map(([event, data]) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('');
  const text = (value: unknown): string =>
    typeof value === 'string'
      ? value
      : Array.isArray(value)
        ? value.map((part) => text(part?.text ?? part?.content ?? '')).join('')
        : '';
  /** Anthropic: one tool call or one text reply, as the request's turn needs. */
  const anthropic = (body: any) => {
    const tools: string[] = (body.tools ?? []).map((tool: any) => tool.name);
    // Anywhere in the conversation: an app may add its own messages after a tool's answer.
    const results = (body.messages ?? [])
      .flatMap((message: any) => (Array.isArray(message.content) ? message.content : []))
      .filter((part: any) => part.type === 'tool_result');
    const answered = results.find((part: any) => String(part.tool_use_id).startsWith('toolu_who'));
    const searched = results.some((part: any) => String(part.tool_use_id).startsWith('toolu_find'));
    const use = answered
      ? null
      : tools.includes('mcp__dispatch__whoami') || searched
        ? { id: `toolu_who${Date.now()}`, name: 'mcp__dispatch__whoami', input: {} }
        : tools.includes('ToolSearch')
          ? {
              id: `toolu_find${Date.now()}`,
              name: 'ToolSearch',
              input: { query: 'select:mcp__dispatch__whoami' },
            }
          : null;
    if (answered) returned.push(text(answered.content));
    const reply = answered ? `Dispatch says: ${text(answered.content)}` : 'No Dispatch tool here.';
    const block = use
      ? { type: 'tool_use', id: use.id, name: use.name, input: {} }
      : { type: 'text', text: '' };
    const message = {
      id: `msg_${Date.now()}`,
      type: 'message',
      role: 'assistant',
      model: body.model,
      content: [],
      stop_reason: null,
      stop_sequence: null,
      usage: { input_tokens: 1, output_tokens: 1 },
    };
    const delta = use
      ? { type: 'input_json_delta', partial_json: JSON.stringify(use.input) }
      : { type: 'text_delta', text: reply };
    const stop = use ? 'tool_use' : 'end_turn';
    if (!body.stream)
      return {
        type: 'application/json',
        body: JSON.stringify({
          ...message,
          content: [use ? { ...block, input: use.input } : { type: 'text', text: reply }],
          stop_reason: stop,
        }),
      };
    return {
      type: 'text/event-stream',
      body: sse([
        ['message_start', { type: 'message_start', message }],
        ['content_block_start', { type: 'content_block_start', index: 0, content_block: block }],
        ['content_block_delta', { type: 'content_block_delta', index: 0, delta }],
        ['content_block_stop', { type: 'content_block_stop', index: 0 }],
        [
          'message_delta',
          {
            type: 'message_delta',
            delta: { stop_reason: stop, stop_sequence: null },
            usage: { output_tokens: 1 },
          },
        ],
        ['message_stop', { type: 'message_stop' }],
      ]),
    };
  };
  /** OpenAI Responses: the same, as Codex reads it. */
  const responses = (body: any) => {
    const flat = (body.tools ?? []).flatMap((tool: any) =>
      tool.type === 'namespace'
        ? (tool.tools ?? []).map((inner: any) => ({ namespace: tool.name, name: inner.name }))
        : [{ name: tool.name }],
    );
    // Codex groups an MCP server's tools under a namespace, `mcp__dispatch`.
    const whoami = flat.find(
      (tool: any) =>
        (tool.namespace === 'mcp__dispatch' && tool.name === 'whoami') ||
        /^mcp_+dispatch_+whoami$/.test(tool.name),
    );
    const output = (body.input ?? []).find(
      (item: any) =>
        item.type === 'function_call_output' && String(item.call_id).startsWith('call_whoami'),
    );
    if (output) returned.push(text(output.output));
    const item = output
      ? {
          type: 'message',
          role: 'assistant',
          id: `msg_${Date.now()}`,
          content: [{ type: 'output_text', text: `Dispatch says: ${text(output.output)}` }],
        }
      : whoami
        ? {
            type: 'function_call',
            id: `fc_${Date.now()}`,
            call_id: 'call_whoami',
            ...(whoami.namespace ? { namespace: whoami.namespace } : {}),
            name: whoami.name,
            arguments: '{}',
          }
        : {
            type: 'message',
            role: 'assistant',
            id: `msg_${Date.now()}`,
            content: [{ type: 'output_text', text: 'No Dispatch tool here.' }],
          };
    const response = {
      id: `resp_${Date.now()}`,
      object: 'response',
      status: 'completed',
      model: body.model,
      output: [item],
      usage: {
        input_tokens: 1,
        input_tokens_details: { cached_tokens: 0 },
        output_tokens: 1,
        output_tokens_details: { reasoning_tokens: 0 },
        total_tokens: 2,
      },
    };
    if (!body.stream) return { type: 'application/json', body: JSON.stringify(response) };
    return {
      type: 'text/event-stream',
      body: sse([
        ['response.created', { type: 'response.created', response: { ...response, output: [] } }],
        [
          'response.output_item.added',
          { type: 'response.output_item.added', output_index: 0, item },
        ],
        ['response.output_item.done', { type: 'response.output_item.done', output_index: 0, item }],
        ['response.completed', { type: 'response.completed', response }],
      ]),
    };
  };
  /** OpenAI Chat Completions: the same, as Hermes reads it from a provider of its own. */
  const completions = (body: any) => {
    const names: string[] = (body.tools ?? []).map((tool: any) => tool.function?.name);
    const direct = names.find((name) => /^mcp_+dispatch_+whoami$/.test(name ?? ''));
    // Hermes defers MCP tools: one `tool_call` tool invokes them by name.
    const whoami = direct
      ? { name: direct, arguments: '{}' }
      : names.includes('tool_call')
        ? {
            name: 'tool_call',
            arguments: JSON.stringify({
              calls: [{ name: 'mcp__dispatch__whoami', arguments: {} }],
            }),
          }
        : null;
    const result = (body.messages ?? []).find(
      (message: any) => message.role === 'tool' && message.tool_call_id === 'call_whoami',
    );
    if (result) returned.push(text(result.content));
    const call = !result && whoami !== null;
    const message = call
      ? {
          role: 'assistant',
          content: null,
          tool_calls: [{ id: 'call_whoami', type: 'function', function: whoami }],
        }
      : {
          role: 'assistant',
          content: result ? `Dispatch says: ${text(result.content)}` : 'No Dispatch tool here.',
        };
    const finish = call ? 'tool_calls' : 'stop';
    const head = { id: `chatcmpl_${Date.now()}`, created: 0, model: body.model };
    const usage = { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 };
    if (!body.stream)
      return {
        type: 'application/json',
        body: JSON.stringify({
          ...head,
          object: 'chat.completion',
          choices: [{ index: 0, message, finish_reason: finish }],
          usage,
        }),
      };
    const delta = call
      ? { role: 'assistant', tool_calls: [{ index: 0, ...message.tool_calls![0] }] }
      : { role: 'assistant', content: message.content };
    const chunk = (choice: object, extra = {}) =>
      `data: ${JSON.stringify({ ...head, object: 'chat.completion.chunk', choices: [{ index: 0, ...choice }], ...extra })}\n\n`;
    return {
      type: 'text/event-stream',
      body:
        chunk({ delta, finish_reason: null }) +
        chunk({ delta: {}, finish_reason: finish }, { usage }) +
        'data: [DONE]\n\n',
    };
  };
  const server = http.createServer((request, response) => {
    let raw = '';
    request.on('data', (chunk) => (raw += chunk));
    request.on('end', () => {
      let body: any = {};
      try {
        body = JSON.parse(raw || '{}');
      } catch {}
      if (log)
        fs.appendFileSync(log, `${JSON.stringify({ path: request.url, body: redact(raw) })}\n`);
      const route = new URL(request.url ?? '/', 'http://x').pathname;
      const answer =
        route === '/v1/messages'
          ? anthropic(body)
          : route === '/v1/messages/count_tokens'
            ? { type: 'application/json', body: '{"input_tokens":1}' }
            : route === '/v1/responses'
              ? responses(body)
              : route === '/v1/chat/completions'
                ? completions(body)
                : route === '/v1/models'
                  ? {
                      type: 'application/json',
                      body: '{"object":"list","data":[{"id":"stand-in","object":"model"}]}',
                    }
                  : null;
      if (!answer) {
        response.writeHead(404, { 'content-type': 'application/json' });
        response.end('{"error":{"type":"not_found_error","message":"not here"}}');
        return;
      }
      response.writeHead(200, { 'content-type': answer.type });
      response.end(answer.body);
    });
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address() as AddressInfo;
  return { url: `http://127.0.0.1:${port}`, returned, close: () => server.close() };
}

/** How a tool call went: the tool the app called, and whether what came back names the DSP. */
type Call = { ok: boolean; detail: string };

/** The Dispatch tool calls in Claude Code's stream-json output, and its last answer. */
function claudeCalls(output: string): Call {
  const calls: string[] = [];
  let returned = '';
  let answer = '';
  for (const line of output.split('\n')) {
    if (!line.trim().startsWith('{')) continue;
    let event;
    try {
      event = JSON.parse(line);
    } catch {
      continue;
    }
    for (const part of event.message?.content ?? []) {
      if (event.type === 'assistant' && part.type === 'tool_use') calls.push(part.name);
      if (event.type === 'user' && part.type === 'tool_result')
        returned +=
          typeof part.content === 'string'
            ? part.content
            : (part.content ?? []).map((c: { text?: string }) => c.text ?? '').join('');
    }
    if (event.type === 'result') answer = event.result ?? '';
  }
  const called = calls.includes('mcp__dispatch__whoami');
  return {
    ok: called && returned.includes(DSP) && answer.includes(DSP),
    detail: called
      ? `${calls.join(' > ')} returned ${returned.includes(DSP) ? DSP : JSON.stringify(returned.slice(0, 160))}`
      : `no whoami call: ${JSON.stringify(answer.slice(0, 200))}`,
  };
}

/** The same from `codex exec --json`. */
function codexCalls(output: string): Call {
  const calls: string[] = [];
  let returned = '';
  let answer = '';
  for (const line of output.split('\n')) {
    if (!line.trim().startsWith('{')) continue;
    let event;
    try {
      event = JSON.parse(line);
    } catch {
      continue;
    }
    const item = event.item ?? {};
    if (event.type === 'item.completed' && item.type === 'mcp_tool_call') {
      calls.push(`${item.server}.${item.tool} (${item.status})`);
      returned += (item.result?.content ?? []).map((c: { text?: string }) => c.text ?? '').join('');
      if (item.error) returned += JSON.stringify(item.error);
    }
    if (event.type === 'item.completed' && item.type === 'agent_message') answer = item.text;
    if (event.type === 'turn.failed' || event.type === 'error')
      answer ||= JSON.stringify(event.error ?? event.message);
  }
  const called = calls.some((call) => call.startsWith('dispatch.whoami (completed'));
  return {
    ok: called && returned.includes(DSP) && answer.includes(DSP),
    detail: calls.length
      ? `${calls.join(' > ')} returned ${returned.includes(DSP) ? DSP : JSON.stringify(returned.slice(0, 160))}`
      : `no whoami call: ${JSON.stringify(answer.slice(0, 200))}`,
  };
}

const ASK = `Use the Dispatch whoami tool and reply with the DSP name only.`;

type Client = {
  /** The app's name on the approval page and in Connected apps. */
  app: string;
  /** Whether Dispatch knows it by its published document. */
  verified: boolean;
  /** Commands that set the server up without signing in. */
  setup?: (mcp: string) => string[][];
  /** The command that signs in, on a terminal unless `pty` is false. */
  signin: (mcp: string) => string[];
  pty?: boolean;
  /** False for an app that takes no pasted address, which always uses its listener. */
  pastes?: boolean;
  /** Whether the sign-in command goes on as the app's MCP server, which `call` uses. */
  serves?: boolean;
  /** Questions it asks once the browser is done, each with its answer. */
  prompts?: [RegExp, string][];
  /** What it prints once signed in. */
  signedIn: RegExp;
  /** The command that lists its servers, and what says this one is signed in. */
  list?: { command: string[]; ok: RegExp };
  /**
   * One tool call through the app: by a command of its own whose model is the stand-in at
   * `model`, or through the command that signed in, still running.
   */
  call?: (context: {
    model: string;
    /** What the tool answered through the stand-in, this app's call only. */
    returned: string[];
    env: NodeJS.ProcessEnv;
    cwd: string;
    signin: Terminal;
  }) => Promise<Call>;
};

/** An MCP request over a stdio server's stdin, and its answer from its stdout. */
async function rpc(server: Terminal, id: number, method: string, params: unknown) {
  server.type(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
  const line = (await server.until(new RegExp(`^\\{.*"id":${id}[,}].*$`, 'm'), 30_000, true))[0];
  return JSON.parse(line);
}

const CLIENTS: Record<string, Client> = {
  claude: {
    app: 'Claude Code',
    verified: true,
    setup: (mcp) => [
      ['claude', 'mcp', 'add', '--transport', 'http', '--scope', 'user', SERVER, mcp],
    ],
    signin: () => ['claude', 'mcp', 'login', SERVER, '--no-browser'],
    signedIn: /Authenticated with "dispatch"/,
    // Its health check connects with the app's token.
    list: { command: ['claude', 'mcp', 'list'], ok: /^dispatch: .*Connected/m },
    // With the agent test set's flags: no tools but Dispatch's, found by tool search.
    call: async ({ model, env, cwd }) => {
      const { output } = await run(
        [
          'claude',
          '-p',
          ASK,
          '--model',
          'sonnet',
          '--disable-slash-commands',
          '--no-session-persistence',
          '--output-format',
          'stream-json',
          '--verbose',
          '--tools',
          'ToolSearch',
          '--allowedTools',
          'mcp__dispatch__whoami',
        ],
        { ...env, ANTHROPIC_BASE_URL: model, ANTHROPIC_API_KEY: 'stand-in' },
        cwd,
        120_000,
      );
      return claudeCalls(output);
    },
  },
  codex: {
    app: 'Codex',
    verified: true,
    // `codex mcp add` signs in by itself; the remote way adds it, then signs in apart.
    setup: (mcp) => (paste ? [['codex', 'mcp', 'add', SERVER, '--url', mcp]] : []),
    signin: (mcp) =>
      paste
        ? ['codex', 'mcp', 'login', SERVER, '--no-browser']
        : ['codex', 'mcp', 'add', SERVER, '--url', mcp],
    signedIn: /Successfully logged in/,
    list: { command: ['codex', 'mcp', 'list'], ok: /^dispatch\s.*\bOAuth\b/m },
    call: async ({ model, env, cwd }) => {
      const { output } = await run(
        [
          'codex',
          'exec',
          '--skip-git-repo-check',
          '-s',
          'read-only',
          '-m',
          'stand-in',
          '-c',
          'model_provider="stand-in"',
          '-c',
          `model_providers.stand-in={name="stand-in",base_url="${model}/v1",wire_api="responses",request_max_retries=0,stream_max_retries=0}`,
          '--json',
          ASK,
        ],
        env,
        cwd,
        120_000,
      );
      return codexCalls(output);
    },
  },
  // Signs in while it adds the server, then asks which tools to turn on.
  hermes: {
    app: 'Hermes Agent',
    verified: true,
    signin: (mcp) => [
      'hermes',
      'mcp',
      'add',
      SERVER,
      '--url',
      mcp,
      '--auth',
      'oauth',
      '--connect-timeout',
      '300',
    ],
    prompts: [[/Enable all \d+ tools\?/, 'Y\r']],
    signedIn: /Saved 'dispatch' to/,
    // Its test connects with the app's token and lists the tools.
    list: {
      command: ['hermes', 'mcp', 'test', SERVER],
      ok: /Connected \(\d+ms\)\s*\n.*Tools discovered: [1-9]\d*/,
    },
    call: async ({ model, returned, env, cwd }) => {
      // The stand-in as its model, beside the server `hermes mcp add` saved.
      fs.appendFileSync(
        path.join(env.HOME!, '.hermes', 'config.yaml'),
        `\nmodel:\n  provider: custom\n  base_url: ${model}/v1\n  default: stand-in\n` +
          `  api_key: stand-in\n`,
      );
      const { output } = await run(
        ['hermes', 'chat', '-q', ASK, '--oneshot', '-Q'],
        env,
        cwd,
        120_000,
      );
      const got = returned.join('');
      return {
        ok: got.includes(DSP) && output.includes(DSP),
        detail: got
          ? `whoami returned ${got.includes(DSP) ? DSP : JSON.stringify(got.slice(0, 160))}`
          : `no whoami call: ${JSON.stringify(tail(output, 3))}`,
      };
    },
  },
  // The bridge for apps that only run local servers: it signs in when it starts, registering
  // itself, then serves Dispatch's tools over stdio, where the harness calls one directly.
  'mcp-remote': {
    app: 'MCP CLI Proxy',
    verified: false,
    signin: (mcp) => ['npx', '-y', 'mcp-remote', mcp],
    pty: false,
    pastes: false,
    serves: true,
    signedIn: /Proxy established successfully/,
    call: async ({ signin }) => {
      const started = await rpc(signin, 1, 'initialize', {
        protocolVersion: '2025-06-18',
        capabilities: {},
        clientInfo: { name: 'dispatch-signin', version: '1' },
      });
      if (!started.result) return { ok: false, detail: JSON.stringify(started.error) };
      signin.type(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
      const called = await rpc(signin, 2, 'tools/call', { name: 'whoami', arguments: {} });
      const returned = (called.result?.content ?? [])
        .map((c: { text?: string }) => c.text ?? '')
        .join('');
      return {
        ok: !called.result?.isError && returned.includes(DSP),
        detail: `whoami returned ${returned.includes(DSP) ? DSP : JSON.stringify(called).slice(0, 200)}`,
      };
    },
  },
};

async function main() {
  // The approval page is the dashboard's, so the server is the built artifact.
  const artifact = path.resolve(values.artifact!);
  const own = artifact === built.env.DISPATCH_ARTIFACT_ROOT;
  if (own && (values.build || !fs.existsSync(path.join(artifact, 'release.json'))))
    execFileSync('npm', ['run', 'build'], { stdio: 'inherit' });
  if (!fs.existsSync(path.join(artifact, 'release.json')))
    throw new Error(`No build in ${artifact}`);
  const work = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-signin-'));
  // Opening a browser does nothing: the harness's own browser answers every link.
  const noBrowser = path.join(work, 'no-browser');
  fs.writeFileSync(noBrowser, '#!/bin/sh\nexit 0\n', { mode: 0o755 });
  // Nothing of the user's sign-ins or session reaches the apps, nor their keyring.
  const inherited = Object.fromEntries(
    Object.entries(process.env).filter(
      ([name]) =>
        !/^(CLAUDE|ANTHROPIC|CODEX|OPENAI|MCP_|DBUS_SESSION|GNOME_KEYRING|SSH_AUTH)/.test(name),
    ),
  );
  const npmCache = execFileSync('npm', ['config', 'get', 'cache'], { encoding: 'utf8' }).trim();
  const f = await fixture({
    binary: path.join(artifact, 'services/rust/dispatch-backend'),
    env: { DISPATCH_ARTIFACT_ROOT: artifact },
  });
  const browser = await chromium.launch();
  const model = await standIn(values.keep ? path.join(work, 'model.jsonl') : undefined);
  const results: Result[] = [];
  try {
    const owner = await f.client();
    const origin = f.env.DISPATCH_ORIGIN!;
    const mcp = `${origin}/api/v1/mcp`;
    for (const name of clients) {
      const client = CLIENTS[name];
      const result: Result = { client: name, steps: [] };
      results.push(result);
      const step = (label: string, ok: boolean | null, detail = '') => {
        result.steps.push([label, ok, detail]);
        console.log(
          `${ok === null ? 'skip' : ok ? 'pass' : 'FAIL'}  ${name.padEnd(10)} ${label.padEnd(14)} ${detail}`,
        );
      };
      if (!client) {
        step('sign-in', false, 'unknown client');
        continue;
      }
      const home = fs.mkdtempSync(path.join(work, `${name}-`));
      const cwd = path.join(home, 'project');
      fs.mkdirSync(path.join(home, '.codex'), { recursive: true });
      fs.mkdirSync(cwd);
      // The apps keep what they sign in with in files here, never in the desktop keyring.
      fs.writeFileSync(
        path.join(home, '.codex', 'config.toml'),
        'check_for_update_on_startup = false\nmcp_oauth_credentials_store = "file"\n',
      );
      const env: NodeJS.ProcessEnv = {
        ...inherited,
        HOME: home,
        XDG_CONFIG_HOME: path.join(home, '.config'),
        XDG_DATA_HOME: path.join(home, '.local/share'),
        XDG_STATE_HOME: path.join(home, '.local/state'),
        XDG_CACHE_HOME: path.join(home, '.cache'),
        CLAUDE_CONFIG_DIR: path.join(home, '.claude'),
        CODEX_HOME: path.join(home, '.codex'),
        BROWSER: noBrowser,
        DISABLE_AUTOUPDATER: '1',
        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1',
        npm_config_cache: npmCache,
        npm_config_update_notifier: 'false',
      };
      if (!onPath(client.signin(mcp)[0]!)) {
        step('sign-in', null, `${client.signin(mcp)[0]} is not on PATH`);
        continue;
      }
      // The remote way is for apps that take a pasted address; any other uses its listener.
      const pasting = paste && client.pastes !== false;
      let terminal: Terminal | undefined;
      try {
        try {
          for (const command of client.setup?.(mcp) ?? []) {
            // Adding without signing in; `codex mcp add` signs in at once, so it stops there.
            const adding = new Terminal(command, env, cwd);
            await Promise.race([adding.done, adding.until(/\/oauth\/authorize\?/, 30_000)]).catch(
              () => {},
            );
            await adding.stop(0);
          }
          terminal = new Terminal(client.signin(mcp), env, cwd, client.pty !== false);
          const escaped = origin.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
          const link = (await terminal.until(new RegExp(`${escaped}/oauth/authorize\\?\\S+`)))[0];
          const { url, verified } = await approve(
            browser,
            link,
            pasting,
            values.keep ? path.join(work, `${name}-approval.png`) : undefined,
          );
          if (pasting) {
            // The app asks for the address once it has printed the link.
            await terminal.until(/paste|callback url/i, 10_000).catch(() => {});
            terminal.type(`${url}\r`);
          }
          // Hermes, once its listener has the approval, still waits on a pasted address and
          // takes the first answer to its next question as one; so an answer is given again
          // while nothing comes of it.
          let repeated = 0;
          for (const [question, answer] of client.prompts ?? []) {
            await terminal.until(question, 60_000);
            const seen = terminal.text.length;
            // Anything printed after the answer's own echo.
            const answered = () =>
              /\S/.test(terminal!.text.slice(seen).replaceAll(answer.trim(), ''));
            terminal.type(answer);
            while (repeated < 3) {
              await new Promise((resolve) => setTimeout(resolve, 5_000));
              if (answered()) break;
              repeated++;
              terminal.type(answer);
            }
          }
          await terminal.until(client.signedIn, 60_000);
          step(
            'sign-in',
            verified === client.verified,
            `${pasting ? 'pasted back' : 'own listener'}; approval page says ${verified ? 'verified' : 'unverified'}` +
              (repeated ? `; its question needed ${repeated + 1} answers` : ''),
          );
        } catch (error) {
          step('sign-in', false, redact(String((error as Error).message)));
          continue;
        }
        // Signed in, the command ends, or for a bridge goes on serving the tool call below.
        if (!client.serves) await terminal.stop();
        if (client.list) {
          const listed = await run(client.list.command, env, cwd);
          const shown = listed.output.match(client.list.ok)?.[0] ?? tail(listed.output, 4);
          step(
            'listed',
            client.list.ok.test(listed.output),
            redact(shown.trim().replace(/\s*\n\s*/g, ' / ')),
          );
        }
        if (client.call) {
          model.returned.length = 0;
          const call = await client
            .call({ model: model.url, returned: model.returned, env, cwd, signin: terminal })
            .catch((error: Error) => ({ ok: false, detail: error.message }));
          step('tool call', call.ok, redact(call.detail));
        }
      } finally {
        await terminal?.stop(0);
      }
      const { keys } = await owner.read('/api/platform/agents');
      const apps = (keys as any[]).filter(
        (key) => key.kind === 'app' && !key.revokedAt && key.client?.name === client.app,
      );
      const app = apps[0];
      step(
        'connected app',
        apps.length === 1 &&
          app.client.verified === client.verified &&
          app.client.status === 'connected' &&
          app.tools === 'essential',
        app
          ? `kind ${app.kind}, ${app.client.name}, ${app.client.verified ? 'verified' : 'unverified'}, ` +
              `${app.client.status}, ${app.tools} tools, last used by ${app.lastClient ?? 'nothing yet'}`
          : 'not listed',
      );
    }
  } finally {
    model.close();
    await browser.close();
    await f.close();
    if (values.keep) console.log(`\nKept the apps' homes in ${work}`);
    else fs.rmSync(work, { recursive: true, force: true });
  }
  report(results);
  if (results.some((r) => r.steps.some(([, ok]) => ok === false))) process.exitCode = 1;
}

function report(results: Result[]) {
  const labels = [...new Set(results.flatMap((r) => r.steps.map(([label]) => label)))];
  const cell = (r: Result, label: string) => {
    const found = r.steps.find(([l]) => l === label);
    return found ? (found[1] === null ? 'skip' : found[1] ? 'pass' : 'FAIL') : '-';
  };
  console.log(
    `\n| Client | ${labels.join(' | ')} |\n| --- |${labels.map(() => ' --- |').join('')}`,
  );
  for (const r of results)
    console.log(`| ${r.client} | ${labels.map((l) => cell(r, l)).join(' | ')} |`);
}

await main();

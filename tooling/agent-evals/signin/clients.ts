import fs from 'node:fs';
import path from 'node:path';
import { rpc, run, tail, type Terminal } from './transport.js';

/** How a tool call went: the tool the app called, and whether what came back names the DSP. */
type Call = { ok: boolean; detail: string };

const ASK = `Use the Dispatch whoami tool and reply with the DSP name only.`;

type Client = {
  /** The app's name on the approval page and in the Apps list. */
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

export function clientAdapters({
  paste,
  dsp: DSP,
  server: SERVER,
}: {
  paste: boolean;
  dsp: string;
  server: string;
}): Record<string, Client> {
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
        returned += (item.result?.content ?? [])
          .map((c: { text?: string }) => c.text ?? '')
          .join('');
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

  return {
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
}

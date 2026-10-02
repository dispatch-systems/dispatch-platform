// The agent test set: real agents answer real questions about a synthetic DSP through
// Dispatch, and their answers are checked against the databases. It runs the command-line
// agents already signed in on this machine (Claude Code and Codex, on their own
// subscriptions), never an API key, so it runs here on demand and never in CI.
//
//   npm run agents:eval -- [--models claude:haiku,codex:gpt-6-luna] [--only id,id]
//                          [--tools full|essential] [--parallel 4] [--rest]
//
// --rest also asks every question with no MCP at all: Claude Code with the Dispatch skill,
// curl and a key, the way a harness without MCP would.
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fixture } from '../testing/fixture-server.js';
import { check, questions, type Question } from './questions.js';

const { values } = parseArgs({
  options: {
    models: { type: 'string', default: 'claude:haiku,claude:sonnet,codex:gpt-6-luna' },
    only: { type: 'string' },
    tools: { type: 'string', default: 'full' },
    parallel: { type: 'string', default: '4' },
    rest: { type: 'boolean', default: false },
    out: { type: 'string', default: path.join(os.tmpdir(), 'dispatch-agent-eval') },
  },
});
const out = path.resolve(values.out!);
fs.mkdirSync(out, { recursive: true });

type Run = {
  model: string;
  question: Question;
  answer: string | null;
  correct: boolean;
  tools: string[];
  seconds: number;
  /** How much the model read from Dispatch: the answers' characters, all calls together. */
  bytes: number;
  error?: string;
};

const PREFIX =
  'Answer this question about the delivery company using the Dispatch tools. ' +
  'Do not guess or estimate. End your reply with exactly one line: ANSWER: <answer>, ' +
  'written as ';

function run(command: string, args: string[], env: NodeJS.ProcessEnv, cwd: string) {
  return new Promise<{ stdout: string; code: number | null }>((resolve) => {
    const child = spawn(command, args, { env, cwd, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.resume();
    const timer = setTimeout(() => child.kill('SIGTERM'), 300_000);
    // A command that cannot start, as when an agent is not installed, fails its run alone.
    child.on('error', (error) => {
      clearTimeout(timer);
      resolve({ stdout: `${stdout}\nspawn failed: ${error.message}`, code: null });
    });
    child.on('close', (code) => {
      clearTimeout(timer);
      resolve({ stdout, code });
    });
  });
}

/** The last ANSWER a reply gives, on its own line or at the end of one. */
const finalAnswer = (text: string) =>
  [...text.matchAll(/ANSWER\**:\**\s*(.+?)\s*$/gm)].at(-1)?.[1]?.replace(/\*+$/, '').trim() ?? null;

async function main() {
  const f = await fixture();
  try {
    await f.stop();
    const world = JSON.parse(f.cli(['seed-agents']).trim());
    await f.start();
    const owner = await f.client();
    const made = await owner.post('/api/platform/agents/keys', {
      name: 'Agent test set',
      allDsps: false,
      dsps: [world.dsp],
      access: 'read',
      tools: values.tools,
      // Addresses on, as the owner's own key has them: feedback is asked about by address.
      locations: true,
      expiresAt: null,
    });
    if (made.status !== 200) throw new Error(made.body);
    const key: string = made.value.token;
    const server = `http://127.0.0.1:${f.env.PORT}`;
    const timezone = 'America/Chicago';
    let asked = questions({
      root: f.root,
      dsp: world.dsp,
      from: world.from,
      to: world.to,
      timezone,
    });
    if (values.only) asked = asked.filter((q) => values.only!.split(',').includes(q.id));
    fs.writeFileSync(path.join(out, 'questions.json'), JSON.stringify(asked, null, 2));

    const work = fs.mkdtempSync(path.join(out, 'cwd-'));
    const mcpConfig = path.join(out, 'mcp.json');
    fs.writeFileSync(
      mcpConfig,
      JSON.stringify({
        mcpServers: {
          dispatch: {
            type: 'http',
            url: `${server}/api/v1/mcp`,
            headers: { Authorization: `Bearer ${key}` },
          },
        },
      }),
      { mode: 0o600 },
    );
    const skill = await (
      await fetch(`${server}/api/v1/skill`, { headers: { authorization: `Bearer ${key}` } })
    ).text();
    const env = { ...process.env, DISPATCH_KEY: key, DISPATCH_URL: server };

    const ask = async (model: string, question: Question): Promise<Run> => {
      const [client, name] = model.split(':') as [string, string];
      const prompt = `${PREFIX}${question.format}.\n\n${question.ask}`;
      const started = Date.now();
      const tools: string[] = [];
      let text = '';
      let bytes = 0;
      if (client === 'claude' || client === 'rest') {
        // Only the tools the round is about: Dispatch's MCP tools, or curl. Nothing else, so
        // a run cannot wander into other tools or other sessions on this machine.
        const args = [
          '-p',
          prompt,
          '--model',
          name,
          '--strict-mcp-config',
          '--disable-slash-commands',
          '--no-session-persistence',
          '--output-format',
          'stream-json',
          '--verbose',
        ];
        if (client === 'claude')
          args.push(
            '--mcp-config',
            mcpConfig,
            '--tools',
            'ToolSearch',
            '--allowedTools',
            'mcp__dispatch',
          );
        else
          args.push(
            '--tools',
            'Bash',
            '--allowedTools',
            'Bash(curl:*)',
            'Bash(jq:*)',
            '--append-system-prompt',
            // Claude Code refuses a command that expands a variable, so the test key is given
            // as it is; it reaches only this throwaway server.
            `${skill.replaceAll(f.env.DISPATCH_ORIGIN!, server)}\n\nThere are no Dispatch MCP ` +
              `tools here: use curl against ${server}, sending the header ` +
              `"Authorization: Bearer ${key}" written out in full.`,
          );
        const { DISPATCH_KEY: _, ...withoutKey } = env;
        const result = await run('claude', args, client === 'rest' ? env : withoutKey, work);
        for (const line of result.stdout.split('\n')) {
          // Only the event lines; a warning the CLI prints between them is not one.
          if (!line.trim().startsWith('{')) continue;
          let event;
          try {
            event = JSON.parse(line);
          } catch {
            continue;
          }
          if (event.type === 'assistant')
            for (const part of event.message.content) {
              if (part.type === 'tool_use' && part.name !== 'ToolSearch')
                tools.push(
                  part.name === 'Bash'
                    ? `curl ${String(part.input.command).match(/\/api\/v1\/[^\s"']*/)?.[0] ?? ''}`
                    : `${part.name.replace('mcp__dispatch__', '')}${JSON.stringify(part.input)}`,
                );
            }
          if (event.type === 'user')
            for (const part of event.message.content ?? [])
              if (part.type === 'tool_result')
                bytes += Buffer.byteLength(
                  typeof part.content === 'string'
                    ? part.content
                    : (part.content ?? []).map((c: { text?: string }) => c.text ?? '').join(''),
                  'utf8',
                );
          if (event.type === 'result') text = event.result ?? '';
        }
      } else if (client === 'codex') {
        const result = await run(
          'codex',
          [
            'exec',
            '--ignore-user-config',
            '--skip-git-repo-check',
            '-m',
            name,
            '-s',
            'read-only',
            '-c',
            `mcp_servers.dispatch.url="${server}/api/v1/mcp"`,
            '-c',
            'mcp_servers.dispatch.bearer_token_env_var="DISPATCH_KEY"',
            '--json',
            prompt,
          ],
          env,
          work,
        );
        for (const line of result.stdout.split('\n')) {
          if (!line.trim().startsWith('{')) continue;
          let event;
          try {
            event = JSON.parse(line);
          } catch {
            continue;
          }
          const item = event.item ?? {};
          if (event.type === 'item.completed' && item.type === 'mcp_tool_call') {
            tools.push(`${item.tool}${JSON.stringify(item.arguments ?? {})}`);
            for (const c of item.result?.content ?? [])
              bytes += Buffer.byteLength(c.text ?? '', 'utf8');
          }
          if (event.type === 'item.completed' && item.type === 'agent_message') text = item.text;
        }
      } else throw new Error(`unknown client ${client}`);
      const answer = finalAnswer(text);
      return {
        model,
        question,
        answer,
        correct: check(question, answer),
        tools,
        seconds: Math.round((Date.now() - started) / 100) / 10,
        bytes,
        ...(answer === null ? { error: text.slice(-300) } : {}),
      };
    };

    const models = values.models!.split(',').filter(Boolean);
    if (values.rest) models.push('rest:haiku');
    const jobs = models.flatMap((model) => asked.map((question) => () => ask(model, question)));
    const runs: Run[] = [];
    const parallel = Number(values.parallel);
    let next = 0;
    await Promise.all(
      Array.from({ length: parallel }, async () => {
        while (next < jobs.length) {
          const run = await jobs[next++]!();
          runs.push(run);
          console.log(
            `${run.correct ? 'pass' : 'FAIL'}  ${run.model.padEnd(22)} ${run.question.id.padEnd(24)} ` +
              `${String(run.seconds).padStart(5)}s ${String(run.bytes).padStart(6)}B  ${run.tools.join(' > ')}` +
              (run.correct
                ? ''
                : `\n      said ${JSON.stringify(run.answer)}, expected ${run.question.expected.join(' | ')}`),
          );
        }
      }),
    );
    report(runs, models, asked);
  } finally {
    await f.close();
  }
}

function report(runs: Run[], models: string[], asked: Question[]) {
  const lines = [
    '# Agent test set',
    '',
    '| Model | Correct | Median time | Median tool calls | Median bytes read |',
    '| --- | --- | --- | --- | --- |',
  ];
  const median = (values: number[]) =>
    [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)] ?? 0;
  for (const model of models) {
    const mine = runs.filter((run) => run.model === model);
    const correct = mine.filter((run) => run.correct).length;
    lines.push(
      `| ${model} | ${correct}/${mine.length} | ${median(mine.map((run) => run.seconds))}s | ${median(mine.map((run) => run.tools.length))} | ${median(mine.map((run) => run.bytes))} |`,
    );
  }
  lines.push('', '## Misses', '');
  for (const run of runs.filter((run) => !run.correct))
    lines.push(
      `- **${run.model}** ${run.question.id}: said ${JSON.stringify(run.answer)}, expected ${run.question.expected.join(' | ')} (${run.tools.join(' > ') || 'no tools'})`,
    );
  lines.push(
    '',
    '## By question',
    '',
    `| Question | ${models.join(' | ')} |`,
    `| --- |${models.map(() => ' --- |').join('')}`,
  );
  for (const question of asked)
    lines.push(
      `| ${question.id} | ${models
        .map((model) =>
          runs.find((run) => run.model === model && run.question.id === question.id)?.correct
            ? 'yes'
            : '**no**',
        )
        .join(' | ')} |`,
    );
  const file = path.join(out, 'report.md');
  fs.writeFileSync(file, `${lines.join('\n')}\n`);
  fs.writeFileSync(path.join(out, 'runs.json'), JSON.stringify(runs, null, 2));
  console.log(`\n${lines.slice(0, 4 + models.length).join('\n')}\n\nReport: ${file}`);
}

await main();

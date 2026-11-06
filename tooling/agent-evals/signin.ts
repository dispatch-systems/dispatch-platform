// Sign in with Dispatch, proved with the real apps. Each MCP client installed here signs in
// to a private Dispatch server the way its user would, with no key: it adds the server and
// prints the sign-in link, a browser signs in as the platform owner and approves it, and the
// app finishes on its own. Then the app connects and calls a tool, and the owner's Apps list
// shows it. Every app runs in a temporary home of its own, so the user's own
// configuration and sign-ins are never read or changed. It runs here on demand, never in CI.
//
//   npm run agents:signin -- [--clients claude,codex] [--hermes] [--mcp-remote] [--paste]
//                            [--build | --artifact <dir>] [--keep]
//
// By default each app takes the approval back on its own listener, as on the user's own
// computer, after the command Connect an app gives. --paste takes the remote (SSH) way: the
// browser stops at the address the app is sent back to, and that address is pasted into the
// app. --hermes adds Hermes Agent (if `hermes` is on PATH) and --mcp-remote adds the
// mcp-remote bridge (through npx).
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { chromium, type Browser } from '@playwright/test';
import { built, demo, fixture } from '../testing/fixture-server.js';
import { clientAdapters } from './signin/clients.js';
import { standIn } from './signin/protocol.js';
import { onPath, redact, run, tail, Terminal } from './signin/transport.js';

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

/**
 * The owner's side, in a browser: open the app's sign-in link, sign in, approve it for the
 * one DSP, and answer where the browser is sent back to the app.
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

const CLIENTS = clientAdapters({ paste, dsp: DSP, server: SERVER });

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
        !/^(CLAUDE|ANTHROPIC|CODEX|OPENAI|HERMES|MCP_|DBUS_SESSION|GNOME_KEYRING|SSH_AUTH)/.test(
          name,
        ),
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
        HERMES_HOME: path.join(home, '.hermes'),
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
          // Connecting is open for ten minutes from the owner's side, as copying a command does.
          const opened = await owner.post('/api/platform/oauth/pairing', {});
          if (opened.status !== 200) throw new Error(`pairing: ${opened.status} ${opened.body}`);
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
      // Approved for the one DSP, read only.
      step(
        'connected app',
        apps.length === 1 &&
          app.client.verified === client.verified &&
          app.client.status === 'connected' &&
          !app.allDsps &&
          app.dsps.length === 1 &&
          app.access === 'read',
        app
          ? `kind ${app.kind}, ${app.client.name}, ${app.client.verified ? 'verified' : 'unverified'}, ` +
              `${app.client.status}, reaches ${app.dsps.length} DSP, ${app.access}, ` +
              `last used by ${app.lastClient ?? 'nothing yet'}`
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

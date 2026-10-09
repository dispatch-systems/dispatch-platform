import { collectorDatabase } from './collector-storage.js';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import http from 'node:http';
import { spawn, execFileSync, type ChildProcess } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';

// The one place tests and tooling start a private Dispatch server: temporary state,
// a free port, the fixture environment, seeded data, `serve`, the health wait and login.

/** The accounts `seed` creates; `bootstrap` gives the owner the same password. */
export const demo = {
  email: 'owner@dispatch.test',
  member: 'member@dispatch.test',
  password: 'Dispatch-demo-2026!',
  /** The short code of the DSP the member belongs to: they sign in at its address. */
  memberDsp: 'nll',
};
/** The release build `npm run build` writes, which browser and smoke checks serve. */
export const built = {
  binary: path.resolve('.build/services/rust/dispatch-backend'),
  env: { DISPATCH_ARTIFACT_ROOT: path.resolve('.build') },
};
export type FixtureOptions = {
  /** `seed` loads the demo DSPs (default); false bootstraps an empty platform. */
  seed?: boolean;
  /** False leaves the seeded fixture stopped so callers can prepare data before serving. */
  start?: boolean;
  /** Added to, or replacing, the fixture environment. */
  env?: NodeJS.ProcessEnv;
  /** Defaults to DISPATCH_TEST_BINARY or the debug build. */
  binary?: string;
  /** `inherit` streams the server's output instead of keeping it for `logs()`. */
  output?: 'capture' | 'inherit';
};
const defaultBinary = path.resolve(
  process.env.DISPATCH_TEST_BINARY ?? 'target/debug/dispatch-backend',
);
export async function freePort() {
  const listener = net.createServer();
  await new Promise<void>((resolve, reject) => {
    listener.once('error', reject);
    listener.listen(0, '127.0.0.1', resolve);
  });
  const port = (listener.address() as net.AddressInfo).port;
  await new Promise<void>((resolve) => listener.close(() => resolve()));
  return port;
}
/** Private state, environment and seeded data for a server that is not running yet. */
export async function prepare(options: boolean | FixtureOptions = true) {
  let binary = typeof options === 'boolean' ? defaultBinary : (options.binary ?? defaultBinary);
  const seed = typeof options === 'boolean' ? options : (options.seed ?? true);
  const overrides = typeof options === 'boolean' ? {} : (options.env ?? {});
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-fixture-'));
  const cleanup = () => fs.rmSync(root, { recursive: true, force: true });
  try {
    if (overrides.DISPATCH_FIXTURE_PROVIDER_URL) {
      const executable = path.join(root, 'dispatch-backend');
      fs.copyFileSync(binary, executable, fs.constants.COPYFILE_FICLONE);
      fs.chmodSync(executable, 0o700);
      binary = executable;
    }
    const port = await freePort();
    // Keep the server transport pinned to loopback, while advertising localhost as
    // the application origin. WebAuthn permits localhost for development but does
    // not permit an IP address as a relying-party ID.
    const address = `http://127.0.0.1:${port}`;
    const origin = `http://localhost:${port}`;
    const env = {
      ...process.env,
      NODE_ENV: 'development',
      DISPATCH_STANDALONE: '1',
      DISPATCH_ENVIRONMENT: 'preview',
      DISPATCH_DEV_MAIL_MODE: 'capture',
      DISPATCH_PRODUCTION_MAIL_MODE: 'capture',
      DISPATCH_STATE_ROOT: root,
      DISPATCH_PROVIDER_MODE: 'fixture',
      DISPATCH_ORIGIN: origin,
      PORT: String(port),
      ...overrides,
    };
    const cli = (args: string[], input?: string) =>
      execFileSync(binary, args, { env, input, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
    if (seed)
      execFileSync(binary, ['seed'], {
        env: { ...env, DISPATCH_PROVIDER_MODE: 'fixture' },
        stdio: 'pipe',
      });
    else cli(['bootstrap', demo.email, 'Fresh', 'Owner'], demo.password);
    return { root, binary, env, port, address, cli, cleanup };
  } catch (error) {
    cleanup();
    throw error;
  }
}
export async function fixture(options: boolean | FixtureOptions = true) {
  const { root, binary, env, address, cli, cleanup } = await prepare(options);
  let origin = address;
  const overrides = typeof options === 'boolean' ? {} : (options.env ?? {});
  const inherit = typeof options !== 'boolean' && options.output === 'inherit';
  const password = demo.password;
  let server: ChildProcess | undefined;
  let logs = '';
  const requestTimeout = overrides.DISPATCH_FIXTURE_PROVIDER_URL ? 180000 : 15000;
  /** The untouched response, for pages and assets that are not JSON. */
  const raw = (url: string, body?: unknown, headers: Record<string, string> = {}) =>
    fetch(origin + url, {
      method: body === undefined ? 'GET' : 'POST',
      headers: { origin: env.DISPATCH_ORIGIN, 'content-type': 'application/json', ...headers },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(requestTimeout),
    });
  const request = async (url: string, body?: unknown, headers: Record<string, string> = {}) => {
    if (headers.host) {
      return await new Promise<{
        status: number;
        statusCode: number;
        body: string;
        json: <T = any>() => T;
        headers: Headers;
        value: any;
      }>((resolve, reject) => {
        const req = http.request(
          origin + url,
          {
            method: body === undefined ? 'GET' : 'POST',
            headers: {
              origin: env.DISPATCH_ORIGIN,
              'content-type': 'application/json',
              ...headers,
            },
            signal: AbortSignal.timeout(requestTimeout),
          },
          (res) => {
            let text = '';
            const responseHeaders = new Headers();
            for (const [name, values] of Object.entries(res.headers))
              for (const value of Array.isArray(values)
                ? values
                : values === undefined
                  ? []
                  : [values])
                responseHeaders.append(name, value);
            res.once('error', reject);
            res.on('data', (chunk) => (text += chunk));
            res.on('end', () => {
              try {
                const value = JSON.parse(text);
                resolve({
                  status: res.statusCode!,
                  statusCode: res.statusCode!,
                  body: text,
                  json: <T = any>() => JSON.parse(text) as T,
                  headers: responseHeaders,
                  value,
                });
              } catch (error) {
                reject(error);
              }
            });
          },
        );
        req.once('error', reject);
        req.end(body === undefined ? undefined : JSON.stringify(body));
      });
    }
    const response = await raw(url, body, headers);
    const value = await response.json();
    return {
      status: response.status,
      statusCode: response.status,
      body: JSON.stringify(value),
      json: <T = any>() => value as T,
      headers: response.headers,
      value,
    };
  };
  // Whether a server of this fixture has served, after which its origin can no longer move.
  let served = false;
  const start = async (): Promise<void> => {
    const child = spawn(binary, ['serve'], {
      env,
      stdio: inherit ? ['ignore', 'inherit', 'inherit'] : ['ignore', 'pipe', 'pipe'],
    });
    server = child;
    let startupError: Error | undefined;
    child.once('error', (error) => {
      startupError = error;
    });
    const from = logs.length;
    // A browser that fails to start, or a request the server failed, says why only in the
    // server's log. Show those lines in the test output as they happen, so a failure on a
    // CI runner explains itself.
    const keep = (data: Buffer) => {
      logs += data;
      for (const line of String(data).split('\n'))
        if (line.includes('"browser.start_failed"') || /"status":5\d\d\b/.test(line))
          process.stderr.write(`${line}\n`);
    };
    child.stdout?.on('data', keep);
    child.stderr?.on('data', keep);
    // The port was free when prepare() chose it, but another process can listen on it before
    // this server binds, and would answer the health check in its place. Only this server's
    // own start line, logged once it holds the port, proves the answer is its own.
    await until(async () => {
      if (startupError) throw startupError;
      if (child.exitCode !== null || child.signalCode !== null) return true;
      if (!inherit && !logs.includes('"event":"core.started"', from)) return false;
      try {
        return (await request('/api/health')).status === 200;
      } catch {
        return false;
      }
    });
    if (child.exitCode === null && child.signalCode === null) {
      served = true;
      return;
    }
    // Lost the port before anything used this origin: take another. Later, fail with the log.
    assert(
      !served &&
        !overrides.PORT &&
        !overrides.DISPATCH_ORIGIN &&
        logs.includes('"kind":"AddrInUse"', from),
      logs,
    );
    const port = await freePort();
    Object.assign(env, { PORT: String(port), DISPATCH_ORIGIN: `http://localhost:${port}` });
    origin = `http://127.0.0.1:${port}`;
    return start();
  };
  const stop = async (signal: NodeJS.Signals = 'SIGTERM') => {
    if (server?.pid && server.exitCode === null && server.signalCode === null) {
      const process = server;
      await new Promise<void>((resolve, reject) => {
        let timeoutError: Error | undefined;
        const timer = setTimeout(() => {
          timeoutError = new Error(`Server failed to stop: ${logs}`);
          process.kill('SIGKILL');
        }, 10000);
        process.once('exit', () => {
          clearTimeout(timer);
          if (timeoutError) reject(timeoutError);
          else resolve();
        });
        process.kill(signal);
      });
    }
  };
  const database = <T>(area: string, callback: (db: DatabaseSync) => T): T => {
    const db = new DatabaseSync(path.join(root, area));
    try {
      // The fixture server's mail worker can briefly hold the write lock.
      db.exec('PRAGMA busy_timeout=5000');
      return callback(db);
    } finally {
      db.close();
    }
  };
  /**
   * The Host and Origin of a DSP's own address, whose short code is `code`, or of the invite
   * page as `invite`: a request with them comes from those pages. Without one, the admin's.
   */
  const at = (code?: string): Record<string, string> => {
    if (!code) return {};
    const host = `${code}.localhost:${env.PORT}`;
    return { host, origin: `http://${host}` };
  };
  /**
   * A signed-in client: the platform owner at the admin's address, or a member at their DSP's,
   * the demo member's by default.
   */
  const client = async (
    email = demo.email,
    secret = password,
    dsp = email === demo.member ? demo.memberDsp : undefined,
  ) => {
    const site = at(dsp);
    const login = await request('/api/auth/login', { email, password: secret }, site);
    assert.equal(login.status, 200, JSON.stringify(login.value));
    const headers: Record<string, string> = {
      ...site,
      cookie: login.headers.get('set-cookie')!.split(';')[0]!,
      'x-dispatch-recovery-code-format': 'grouped-v1',
    };
    const session = await request('/api/session', undefined, headers);
    headers['x-csrf-token'] = session.value.csrf;
    return {
      session: session.value,
      headers,
      get: (url: string) => request(url, undefined, headers),
      /**
       * What a GET answers, for a test that reads it, often while polling. A server
       * shedding load answers 503 `platform_busy`, which a client tries again, as the
       * dashboard does; any other answer but 200 fails with the server's own error.
       */
      read: async (url: string): Promise<any> => {
        const start = Date.now();
        let response = await request(url, undefined, headers);
        while (
          response.status === 503 &&
          response.value?.error === 'platform_busy' &&
          Date.now() - start < 10000
        ) {
          await new Promise((resolve) => setTimeout(resolve, 200));
          response = await request(url, undefined, headers);
        }
        assert.equal(response.status, 200, `GET ${url}: ${response.body}`);
        return response.value;
      },
      post: (url: string, body: unknown = {}) => request(url, body, headers),
      select: async (id: string) => {
        const view = await request('/api/session/dsp', { dspId: id }, headers);
        assert.equal(view.status, 200, JSON.stringify(view.value));
        headers['x-dispatch-view'] = view.value.token;
        return view.value;
      },
    };
  };
  const close = async () => {
    try {
      await stop();
    } finally {
      cleanup();
    }
  };
  try {
    if (typeof options === 'boolean' || options.start !== false) await start();
  } catch (error) {
    await close();
    throw error;
  }
  return {
    root,
    binary,
    env,
    cli,
    start,
    stop,
    request,
    raw,
    at,
    client,
    database,
    /** A DSP's own database, where its people, roles and invitations are kept. */
    people: <T>(dspId: string, callback: (db: DatabaseSync) => T): T =>
      database(`dsps/${dspId}/data/dispatch.sqlite`, callback),
    collector: <T>(dspId: string, callback: (db: DatabaseSync) => T): T =>
      database(path.relative(root, collectorDatabase(root, dspId, 'paycom')), callback),
    pid: () => server!.pid!,
    /** Resolves when the running server exits, however it was stopped. */
    exited: () =>
      new Promise<void>((resolve) =>
        server!.exitCode === null && server!.signalCode === null
          ? server!.once('exit', () => resolve())
          : resolve(),
      ),
    logs: () => logs,
    close,
  };
}
export async function until(check: () => Promise<boolean>, timeout = 12000) {
  const start = Date.now();
  while (!(await check())) {
    assert(Date.now() - start < timeout, 'Condition did not become true');
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

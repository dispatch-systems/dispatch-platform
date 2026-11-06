import { spawn, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

/** Tokens, codes and keys never reach the output. */
export const redact = (text: string) =>
  text
    .replace(/\bds[kar]_(?:live|dev)_[A-Za-z0-9]+/g, '[token]')
    .replace(/([?&#](?:code|access_token|refresh_token)=)[^&\s"']+/g, '$1[redacted]');
/** A terminal's output as text: no colours, cursor moves or carriage returns. */
const plain = (text: string) =>
  // The terminal's escape sequences are what's stripped: the control characters are the point.
  /* eslint-disable no-control-regex */
  text
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '')
    .replace(/\x1b\[[0-9;?<>=]*[ -/]*[@-~]/g, '')
    .replace(/\x1b[()][0-9A-Za-z]|\x1b[=>78DEHM]/g, '')
    .replace(/\r/g, '');
/* eslint-enable no-control-regex */
const quote = (arg: string) => `'${arg.replaceAll("'", `'\\''`)}'`;
export const tail = (text: string, lines = 12) =>
  redact(text.trim().split('\n').slice(-lines).join('\n'));

/**
 * A command on a terminal of its own, as the apps' sign-in needs, read as it goes. Without
 * `pty` it runs on plain pipes instead, as an MCP server over stdio does, its stdout apart.
 */
export class Terminal {
  text = '';
  stdout = '';
  readonly child: ChildProcess;
  readonly done: Promise<number | null>;
  private readonly waiting = new Set<() => void>();
  private failed = false;
  constructor(args: string[], env: NodeJS.ProcessEnv, cwd: string, pty = true) {
    // A wide terminal, so no app wraps the long sign-in link across lines. The pseudo-terminal
    // comes from util-linux `script`; BSD `script` (macOS) takes other flags and isn't supported.
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
    this.done = new Promise((resolve) => {
      this.child.on('close', (code) => {
        resolve(code);
        for (const wake of this.waiting) wake();
      });
      // A command that can't start fails its client's step instead of the whole run.
      this.child.on('error', (error) => {
        this.failed = true;
        this.text += `\n${error.message}\n`;
        resolve(null);
        for (const wake of this.waiting) wake();
      });
    });
  }
  get running() {
    return !this.failed && this.child.exitCode === null && this.child.signalCode === null;
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
export const onPath = (command: string) =>
  (process.env.PATH ?? '')
    .split(path.delimiter)
    .some((dir) => dir && fs.existsSync(path.join(dir, command)));

/** A command run to its end, with what it printed. */
export function run(args: string[], env: NodeJS.ProcessEnv, cwd: string, ms = 60_000) {
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

/** An MCP request over a stdio server's stdin, and its answer from its stdout. */
export async function rpc(server: Terminal, id: number, method: string, params: unknown) {
  server.type(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
  const line = (await server.until(new RegExp(`^\\{.*"id":${id}[,}].*$`, 'm'), 30_000, true))[0];
  return JSON.parse(line);
}

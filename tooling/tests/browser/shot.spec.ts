import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from '@playwright/test';
import { pathToFileURL } from 'node:url';
import { promisify } from 'node:util';
import { inflateSync } from 'node:zlib';
import { repositoryRoot as root } from '../../scaffold/common.js';

const run = promisify(execFile);

/** A PNG's size, and the colour of its first pixel. */
function read(file: string) {
  const png = fs.readFileSync(file);
  const data: Buffer[] = [];
  for (let at = 8; at < png.length; at += 12 + png.readUInt32BE(at)) {
    if (png.toString('latin1', at + 4, at + 8) === 'IDAT')
      data.push(png.subarray(at + 8, at + 8 + png.readUInt32BE(at)));
  }
  // Each row starts with its filter's byte; whichever it is, the first pixel is stored as it is.
  const [, red, green, blue] = inflateSync(Buffer.concat(data));
  return { size: `${png.readUInt32BE(16)}x${png.readUInt32BE(20)}`, first: [red, green, blue] };
}

// It runs `shot` itself, which needs the browser the browser suite installs.
test('a shot clicks each control it names in turn, then saves the window a pixel to a pixel', async () => {
  const dir = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'dispatch-shot-test-'));
  try {
    // The page is white; the dialog its button opens covers it in red, and in blue once saved.
    const page = path.join(dir, 'page.html');
    fs.writeFileSync(
      page,
      `<style>body { background: rgb(255, 255, 255); }
        dialog::backdrop { background: rgb(255, 0, 0); }
        dialog.saved::backdrop { background: rgb(0, 0, 255); }</style>
      <button onclick="document.querySelector('dialog').showModal()">Write up</button>
      <button>Close week</button><button>Close day</button>
      <dialog><button onclick="this.parentElement.classList.add('saved')">Save</button></dialog>`,
    );
    const storage = path.join(dir, 'storage.json');
    fs.writeFileSync(storage, JSON.stringify({ cookies: [], origins: [] }));
    const shot = async (out: string, clicks: string[]) => {
      await run(
        process.execPath,
        [
          'node_modules/tsx/dist/cli.mjs',
          'tooling/dev/shot.ts',
          ...['--url', pathToFileURL(page).href, '--storage', storage, '--host', '127.0.0.1'],
          ...['--size', '1920x1080', '--theme', 'light', '--out', path.join(dir, out)],
          ...clicks.flatMap((click) => ['--click', click]),
        ],
        { cwd: root },
      );
      return read(path.join(dir, out));
    };
    assert.deepEqual(await shot('page.png', []), { size: '1920x1080', first: [255, 255, 255] });
    // Save is only there once the dialog opened.
    assert.deepEqual(await shot('opened.png', ['Write up']), {
      size: '1920x1080',
      first: [255, 0, 0],
    });
    assert.deepEqual(await shot('saved.png', ['Write up', 'Save']), {
      size: '1920x1080',
      first: [0, 0, 255],
    });
    for (const [click, said] of [
      ['Delete', 'Nothing to click is named "Delete".'],
      ['Close', '2 controls are named "Close".'],
    ]) {
      await assert.rejects(shot('refused.png', [click!]), (error: { stderr: string }) =>
        error.stderr.includes(said!),
      );
    }
    assert(!fs.existsSync(path.join(dir, 'refused.png')));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

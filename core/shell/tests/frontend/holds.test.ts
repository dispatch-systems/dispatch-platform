import test from 'node:test';
import assert from 'node:assert/strict';
import { askHolds, holdAddress } from '../../frontend/runtime/holds.js';

/**
 * A tab at `hash` whose history the address moves along as a browser's does: a link followed
 * adds an entry, Back returns to the one before, and each move is heard in a task of its own, by
 * the holds first and then by the app, which notes where it heard the address go. With
 * `numbered`, the tab numbers its entries as the Navigation API does.
 */
function tab(hash: string, numbered: boolean) {
  class HashChangeEvent extends Event {
    oldURL: string;
    newURL: string;
    constructor(type: string, init: { oldURL: string; newURL: string }) {
      super(type);
      this.oldURL = init.oldURL;
      this.newURL = init.newURL;
    }
  }
  const entries = [`https://dispatch.test/${hash}`];
  let index = 0;
  // What the tab still has to do, as a browser does it, after the moment it's asked.
  let pending = 0;
  const later = (run: () => void) => {
    pending++;
    setTimeout(() => {
      pending--;
      run();
    });
  };
  const window = new EventTarget();
  const navigation = Object.assign(new EventTarget(), {
    get currentEntry() {
      return { index };
    },
  });
  // The entry moved from, as the Navigation API tells it: none once replaced.
  const moved = (from: number) =>
    navigation.dispatchEvent(
      Object.assign(new Event('currententrychange'), { from: { index: from } }),
    );
  const changed = (from: string) =>
    later(() =>
      window.dispatchEvent(
        new HashChangeEvent('hashchange', { oldURL: from, newURL: entries[index]! }),
      ),
    );
  const history = {
    state: null,
    replaceState: (_state: unknown, _title: string, url: string) => {
      entries[index] = new URL(url, entries[index]).href;
      moved(-1);
    },
    go: (delta: number) =>
      later(() => {
        const from = index;
        index += delta;
        moved(from);
        changed(entries[from]!);
      }),
  };
  Object.assign(globalThis, {
    window,
    history,
    HashChangeEvent,
    navigation: numbered ? navigation : undefined,
  });
  const heard: string[] = [];
  window.addEventListener('hashchange', askHolds as (event: Event) => void);
  window.addEventListener('hashchange', (event) =>
    heard.push(new URL((event as HashChangeEvent).newURL).hash),
  );
  return {
    heard,
    /** The tab's entries, by their hashes, and the one open. */
    history: () => ({ entries: entries.map((url) => new URL(url).hash), at: index }),
    follow: (to: string) => {
      const from = entries[index]!;
      entries.splice(++index, entries.length, `https://dispatch.test/${to}`);
      moved(index - 1);
      changed(from);
    },
    back: () => history.go(-1),
    /** Waits for every move under way to be heard, however busy the machine. */
    settled: async () => {
      do await new Promise((resolve) => setTimeout(resolve));
      while (pending > 0);
    },
  };
}

test('a page with unsaved edits holds the address until let, the history it pages through as it was', async (t) => {
  const browser = tab('#dsp/dsp_a/dvic', true);
  browser.follow('#dsp/dsp_a/settings?tab=dvic');
  await browser.settled();
  const asked: string[] = [];
  let saved: (() => void) | undefined;
  // An editor asks Save or Discard; the page's other hold is asked only once it lets.
  t.after(
    holdAddress((then) => {
      asked.push('editor');
      saved = then;
    }),
  );
  t.after(
    holdAddress((then) => {
      asked.push('other');
      then();
    }),
  );
  const held = {
    entries: ['#dsp/dsp_a/dvic', '#dsp/dsp_a/settings?tab=dvic'],
    at: 1,
  };
  // The browser's Back, and a save that fails: the page stays, at its entry, and nothing heard.
  browser.back();
  await browser.settled();
  assert.deepEqual(asked, ['editor']);
  assert.deepEqual(browser.history(), held);
  assert.deepEqual(browser.heard, ['#dsp/dsp_a/settings?tab=dvic']);
  // Back again, and saved: the move is made once more, heard once, and asked of no hold twice.
  browser.back();
  await browser.settled();
  saved!();
  await browser.settled();
  assert.deepEqual(asked, ['editor', 'editor', 'other']);
  assert.deepEqual(browser.history(), { ...held, at: 0 });
  assert.deepEqual(browser.heard, ['#dsp/dsp_a/settings?tab=dvic', '#dsp/dsp_a/dvic']);
});

test('where the browser numbers no entries, a move held is put back in place, and made again once let', async (t) => {
  const browser = tab('#dsp/dsp_a/settings?tab=timecard', false);
  let asked = 0;
  let saved: (() => void) | undefined;
  t.after(
    holdAddress((then) => {
      asked++;
      saved = then;
    }),
  );
  browser.follow('#dsp/dsp_a/dvic');
  await browser.settled();
  assert.equal(asked, 1);
  // Unnumbered, its own entry is put back at the page's address, and nothing heard.
  assert.deepEqual(browser.history(), {
    entries: ['#dsp/dsp_a/settings?tab=timecard', '#dsp/dsp_a/settings?tab=timecard'],
    at: 1,
  });
  assert.deepEqual(browser.heard, []);
  saved!();
  await browser.settled();
  assert.equal(asked, 1);
  assert.deepEqual(browser.history(), {
    entries: ['#dsp/dsp_a/settings?tab=timecard', '#dsp/dsp_a/dvic'],
    at: 1,
  });
  assert.deepEqual(browser.heard, ['#dsp/dsp_a/dvic']);
});

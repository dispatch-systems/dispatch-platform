import test from 'node:test';
import assert from 'node:assert/strict';
import { consumeHashToken } from '../../frontend/runtime/navigation.js';

/** A browser at `hash`, with the history a link's secret is removed from. */
function at(hash: string) {
  const location = { pathname: '/', search: '', hash };
  const history = {
    state: null,
    replaceState: (_state: unknown, _title: string, url: string) => {
      location.hash = url.slice(url.indexOf('#'));
    },
  };
  Object.assign(globalThis, { window: { location }, location, history });
  return location;
}

test("a link's secret leaves the address at once, and survives its screen rendering again", () => {
  const location = at('#invite?token=secret');
  assert.equal(consumeHashToken('invite'), 'secret');
  assert.equal(location.hash, '#invite');
  // React renders the screen again, or throws its first render away and renders it anew.
  assert.equal(consumeHashToken('invite'), 'secret');
  // Once the address leaves the link's page, the secret is gone, even back at the page.
  location.hash = '#dsps';
  assert.equal(consumeHashToken('invite'), undefined);
  location.hash = '#invite';
  assert.equal(consumeHashToken('invite'), undefined);
});

test("the way back out of a page with a sidebar of its own outlives its view opening again, not another role's", async () => {
  const { installFeatures } = await import('../../frontend/runtime/slots.js');
  const { credentials } = await import('../../frontend/runtime/api.js');
  const { noteWayBack, wayBack } = await import('../../frontend/runtime/navigation.js');
  const page = (id: string, own?: true) =>
    ({
      id,
      scope: 'dsp',
      label: id,
      nav: true,
      preload: async () => undefined,
      render: () => null,
      ...(own && { sidebar: { load: async () => () => null } }),
    }) as never;
  installFeatures([{ name: 'stand_in', routes: [page('roster'), page('settings', true)] }]);
  credentials('csrf', 'dsp_a.signed');
  noteWayBack('#dsp/dsp_a/roster?day=1', '#dsp/dsp_a/settings');
  // Reopened as the DSP's name changes, its view passes through none.
  credentials('csrf');
  credentials('csrf', 'dsp_a.signed-again');
  assert.equal(wayBack('dsp_a'), '#dsp/dsp_a/roster?day=1');
  credentials('csrf', 'dsp_a.member.signed');
  assert.notEqual(wayBack('dsp_a'), '#dsp/dsp_a/roster?day=1');
});

/**
 * A tab at `hash` whose history the address moves along as a browser's does: a link followed
 * adds an entry, Back returns to the one before, and each move is heard in a task of its own, by
 * the holds first and then by the app, which notes where it heard the address go. With
 * `numbered`, the tab numbers its entries as the Navigation API does.
 */
async function tab(hash: string, numbered: boolean) {
  const { askHolds } = await import('../../frontend/runtime/navigation.js');
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
    setTimeout(() =>
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
      setTimeout(() => {
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
    /** Waits for every move under way to be heard. */
    settled: () => new Promise((resolve) => setTimeout(resolve, 10)),
  };
}

test('a page with unsaved edits holds the address until let, the history it pages through as it was', async (t) => {
  const { holdAddress } = await import('../../frontend/runtime/navigation.js');
  const browser = await tab('#dsp/dsp_a/dvic', true);
  browser.follow('#dsp/dsp_a/settings?tab=dvic');
  await browser.settled();
  const asked: string[] = [];
  let saved: (() => void) | undefined;
  // An editor asks Save or Discard; the page's other hold is asked only once it lets.
  t.after(
    holdAddress((to, then) => {
      asked.push(`editor ${to}`);
      saved = then;
    }),
  );
  t.after(
    holdAddress((to, then) => {
      asked.push(`other ${to}`);
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
  assert.deepEqual(asked, ['editor #dsp/dsp_a/dvic']);
  assert.deepEqual(browser.history(), held);
  assert.deepEqual(browser.heard, ['#dsp/dsp_a/settings?tab=dvic']);
  // Back again, and saved: the move is made once more, heard once, and asked of no hold twice.
  browser.back();
  await browser.settled();
  saved!();
  await browser.settled();
  assert.deepEqual(asked, [
    'editor #dsp/dsp_a/dvic',
    'editor #dsp/dsp_a/dvic',
    'other #dsp/dsp_a/dvic',
  ]);
  assert.deepEqual(browser.history(), { ...held, at: 0 });
  assert.deepEqual(browser.heard, ['#dsp/dsp_a/settings?tab=dvic', '#dsp/dsp_a/dvic']);
});

test('where the browser numbers no entries, a move held is put back in place, and made again once let', async (t) => {
  const { installFeatures } = await import('../../frontend/runtime/slots.js');
  const { holdAddress } = await import('../../frontend/runtime/navigation.js');
  const page = (id: string, settings?: true) =>
    ({
      id,
      scope: 'dsp',
      label: id,
      preload: async () => undefined,
      render: () => null,
      ...(settings && { hostsSettings: true }),
    }) as never;
  installFeatures([
    { name: 'settings', routes: [page('settings', true)] },
    {
      name: 'timecard',
      routes: [page('timecard')],
      movedToSettings: { 'paycom-settings': 'timecard' },
    },
  ]);
  const browser = await tab('#dsp/dsp_a/settings?tab=timecard', false);
  const asked: (string | undefined)[] = [];
  let saved: (() => void) | undefined;
  t.after(
    holdAddress((to, then) => {
      asked.push(to);
      saved = then;
    }),
  );
  // A link to where its page once was is asked as one to where it is now.
  browser.follow('#dsp/dsp_a/paycom-settings');
  await browser.settled();
  assert.deepEqual(asked, ['#dsp/dsp_a/settings?tab=timecard']);
  // Unnumbered, its own entry is put back at the page's address, and nothing heard.
  assert.deepEqual(browser.history(), {
    entries: ['#dsp/dsp_a/settings?tab=timecard', '#dsp/dsp_a/settings?tab=timecard'],
    at: 1,
  });
  assert.deepEqual(browser.heard, []);
  saved!();
  await browser.settled();
  assert.deepEqual(asked, ['#dsp/dsp_a/settings?tab=timecard']);
  assert.deepEqual(browser.history(), {
    entries: ['#dsp/dsp_a/settings?tab=timecard', '#dsp/dsp_a/paycom-settings'],
    at: 1,
  });
  assert.deepEqual(browser.heard, ['#dsp/dsp_a/paycom-settings']);
});

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

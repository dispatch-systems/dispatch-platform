import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import '../support/manifests.js';
import { featureCatalog } from '../../../core/shell/frontend/runtime/features.js';
import { features } from '../../frontend/features.js';
import { errorLabel } from '../../../core/shell/frontend/runtime/api.js';
import {
  cacheRules,
  connectionCard,
  connectionCards,
  loadPlatformSlots,
  switchIcon,
} from '../../../core/shell/frontend/runtime/slots.js';
import { manifests } from '../../../core/shell/frontend/runtime/slots/installed.js';

const once = (ids: readonly string[], what: string) =>
  assert.deepEqual(
    ids.filter((id, index) => ids.indexOf(id) !== index),
    [],
    `${what} declared more than once`,
  );

test("each page's switch has its icon, declared once", async () => {
  const modules = await Promise.all(features.map((feature) => feature.platformSlots?.()));
  once(
    modules.flatMap((module) => (module?.slots.switch ? [module.slots.switch.id] : [])),
    'switches',
  );
  await loadPlatformSlots();
  for (const page of featureCatalog.filter((entry) => entry.kind === 'page'))
    assert(switchIcon(page.id), `${page.id} has no icon`);
});

test('each connection has one card', () => {
  once(
    connectionCards().map((card) => card.provider),
    'connection cards',
  );
  for (const connection of featureCatalog.filter((entry) => entry.kind === 'connection'))
    assert(connectionCard(connection.id), `${connection.id} has no card`);
});

test('a read of collected data belongs to one owner', () => {
  const owners = cacheRules().map((rules) => rules.collected ?? []);
  owners.forEach((prefixes, owner) =>
    owners.forEach((others, other) => {
      if (other !== owner)
        for (const prefix of prefixes)
          for (const otherPrefix of others)
            assert(
              !prefix.startsWith(otherPrefix) && !otherPrefix.startsWith(prefix),
              `${prefix} and ${otherPrefix} overlap`,
            );
    }),
  );
});

test("each error code is worded once, and core's own wording doesn't hide an owner's", () => {
  const worded = features.flatMap((feature) =>
    Object.entries({ ...feature.errors, ...feature.scheduleIssues }),
  );
  once(
    worded.map(([code]) => code),
    'error codes',
  );
  for (const [code, label] of worded) assert.equal(errorLabel(code), label, code);
});

test("each page's tabs from other features have their own addresses on it", () => {
  const tabs = features.flatMap((feature) => feature.pageTabs ?? []);
  for (const page of new Set(tabs.map((tab) => tab.page)))
    once(
      tabs.filter((tab) => tab.page === page).map((tab) => tab.id),
      `${page}'s tabs`,
    );
});

test("one page at most is where a DSP lands, one draws Settings, and one owner saves a DSP's setup", () => {
  const pages = features.flatMap((feature) => feature.routes ?? []);
  assert(pages.filter((page) => page.scope === 'dsp' && page.landing).length <= 1);
  assert(pages.filter((page) => page.scope === 'dsp' && page.hostsSettings).length <= 1);
  assert(features.filter((feature) => feature.dspSetup).length <= 1);
});

test("each widget has ids of its own under its feature's name, and every id shipped is one", () => {
  const declared = manifests().flatMap((owner) =>
    (owner.widgets ?? []).map((widget) => ({ owner: owner.name, widget })),
  );
  const ids = declared.flatMap(({ widget }) => [widget.id, ...(widget.formerly ?? [])]);
  once(ids, 'widget ids');
  for (const { owner, widget } of declared) {
    for (const id of [widget.id, ...(widget.formerly ?? [])])
      assert(id.startsWith(`${owner}.`), `${id} is not under ${owner}'s name`);
    once(widget.sizes, `${widget.id}'s sizes`);
  }
  const shipped = fs
    .readFileSync(new URL('widgets.txt', import.meta.url), 'utf8')
    .split('\n')
    .filter((line) => line && !line.startsWith('#'));
  assert.deepEqual(
    shipped.filter((id) => !ids.includes(id)),
    [],
    "a widget id that shipped is no widget's id or former id",
  );
  assert.deepEqual(
    declared.map(({ widget }) => widget.id).filter((id) => !shipped.includes(id)),
    [],
    'a widget is missing from app/tests/frontend/widgets.txt',
  );
});

import test from 'node:test';
import assert from 'node:assert/strict';
import '../support/manifests.js';
import { features } from '../../../core/tenancy/api/index.js';
import { permissions } from '../../../core/accounts/api/index.js';
import {
  featureCatalog,
  featureLabel,
  grants,
  schedulesFeature,
} from '../../../core/shell/frontend/runtime/features.js';
import {
  capabilityLabel,
  previewSwitch,
  sideEffects,
} from '../../../core/platform_owner/frontend/dsps/switches.js';

test('the generated feature catalog covers every feature and its dependencies', () => {
  assert.deepEqual(
    featureCatalog.map((feature) => feature.id),
    [...features],
  );
  assert.equal(new Set(features).size, features.length);
  assert(
    featureCatalog.some((feature) => feature.id === schedulesFeature && feature.kind === 'page'),
  );
  const owned: string[] = [];
  for (const feature of featureCatalog) {
    assert(feature.label.length > 0, `${feature.id} has no label`);
    for (const permission of feature.permissions) {
      assert(permissions.includes(permission), `${feature.id} owns an unknown permission`);
      owned.push(permission);
    }
    if (feature.kind === 'tab') {
      assert(featureCatalog.some((page) => page.kind === 'page' && page.id === feature.page));
      assert.deepEqual(feature.permissions, []);
      assert.deepEqual(feature.requires, []);
    }
    for (const capability of feature.requires) {
      assert.notEqual(capabilityLabel(capability), capability, `${capability} has no label`);
      assert(
        featureCatalog.some((provider) => provider.provides?.includes(capability)),
        `${feature.id} requires ${capability} without a provider`,
      );
    }
  }
  assert.equal(new Set(owned).size, owned.length, 'a permission has more than one owning feature');
});

test('a switch brings its dependencies along, as the backend does', () => {
  const all = [...features];
  assert.deepEqual(previewSwitch(all, 'uniforms', false), [
    { feature: 'uniforms', enabled: false },
  ]);
  assert.deepEqual(previewSwitch(all, 'uniforms', true), []);
  // Disabling a provider disables the pages left without one.
  assert.deepEqual(previewSwitch(all, 'cortex', false), [
    { feature: 'cortex', enabled: false },
    { feature: 'timecard', enabled: false },
    { feature: 'routes', enabled: false },
    { feature: 'dvic', enabled: false },
    { feature: 'weekly_scorecard', enabled: false },
    { feature: 'driver_match', enabled: false },
  ]);
  // Driver Match needs both sides: losing Paycom takes it too.
  assert.deepEqual(previewSwitch(all, 'paycom', false), [
    { feature: 'paycom', enabled: false },
    { feature: 'timecard', enabled: false },
    { feature: 'driver_match', enabled: false },
  ]);
  // Enabling a page enables the one provider of each capability it lacks. Tabs default on.
  const tabs = features.filter((f) => f.startsWith('timecard.'));
  assert.deepEqual(previewSwitch(['uniforms', 'paycom', ...tabs], 'timecard', true), [
    { feature: 'cortex', enabled: true },
    { feature: 'timecard', enabled: true },
  ]);
  assert.deepEqual(previewSwitch(['uniforms', ...tabs], 'timecard', true), [
    { feature: 'paycom', enabled: true },
    { feature: 'cortex', enabled: true },
    { feature: 'timecard', enabled: true },
  ]);
  assert.deepEqual(previewSwitch(['uniforms'], 'paycom', true), [
    { feature: 'paycom', enabled: true },
  ]);
});

test('a tab goes alone, but the last one takes its page and a page brings them back', () => {
  const all = [...features];
  assert.deepEqual(previewSwitch(all, 'dvic.day', false), [
    { feature: 'dvic.day', enabled: false },
  ]);
  const withoutDay = all.filter((f) => f !== 'dvic.day');
  assert.deepEqual(previewSwitch(withoutDay, 'dvic.week', false), [
    { feature: 'dvic.week', enabled: false },
    { feature: 'dvic', enabled: false },
  ]);
  // A page switched off keeps its tabs; switched on without any, it brings them all.
  assert.deepEqual(previewSwitch(all, 'dvic', false), [{ feature: 'dvic', enabled: false }]);
  const bare = all.filter((f) => !f.startsWith('dvic'));
  assert.deepEqual(previewSwitch(bare, 'dvic', true), [
    { feature: 'dvic', enabled: true },
    { feature: 'dvic.day', enabled: true },
    { feature: 'dvic.week', enabled: true },
  ]);
  assert.deepEqual(previewSwitch([...bare, 'dvic.week'], 'dvic', true), [
    { feature: 'dvic', enabled: true },
  ]);
  // A tab of a page that is off switches alone.
  assert.deepEqual(previewSwitch([...bare, 'dvic.week'], 'dvic.week', false), [
    { feature: 'dvic.week', enabled: false },
  ]);
  const page = featureCatalog.find((f) => f.id === 'dvic')!;
  const tab = featureCatalog.find((f) => f.id === 'dvic.week')!;
  assert.deepEqual(sideEffects(page, previewSwitch(bare, 'dvic', true)!), []);
  assert.deepEqual(sideEffects(tab, previewSwitch(withoutDay, 'dvic.week', false)!), [
    { feature: 'dvic', enabled: false },
  ]);
  assert.equal(featureLabel('timecard.employees'), 'Timecard · Employee Search');
  assert.equal(featureLabel('timecard'), 'Timecard');
});

test('a permission exists only with its feature', () => {
  assert.equal(grants(['uniforms'], 'uniforms.view'), true);
  assert.equal(grants(['timecard'], 'uniforms.view'), false);
  assert.equal(grants([], 'members.invite'), true);
  assert.equal(grants(['cortex'], 'connections.manage'), true);
  assert.equal(grants(['timecard', 'uniforms'], 'connections.manage'), false);
  for (const permission of permissions) assert.equal(grants(features, permission), true);
});

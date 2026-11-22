import fs from 'node:fs';
import path from 'node:path';
import { repositoryRoot as root } from '../../../../tooling/scaffold/common.js';
import {
  test,
  expect,
  expectEverySize,
  login,
  openDsp,
} from '../../../../core/shell/tests/support/fixtures.js';

/** Every widget a release has shipped, as the slots rules hold them. */
const widgets = fs
  .readFileSync(path.join(root, 'app/tests/rules/widgets.txt'), 'utf8')
  .split('\n')
  .map((line) => line.trim())
  .filter((line) => line && !line.startsWith('#'));

/**
 * The custom ID that owns a widget's feature, as `features/custom/<owner>/<folder>/` holds a
 * feature named `<owner>_<folder>`; none for a shared feature's widget.
 */
function customOwner(id: string) {
  const [owner, ...folder] = id.split('.')[0]!.split('_');
  const custom = path.join(root, 'features/custom', owner!, folder.join('_'));
  return folder.length && fs.existsSync(custom) ? owner : undefined;
}
/** Each custom ID's widgets, which only a DSP with that ID has. */
const custom = new Map<string, string[]>();
for (const id of widgets) {
  const owner = customOwner(id);
  if (owner) custom.set(owner, [...(custom.get(owner) ?? []), id]);
}

// The seeded DSP has every shared feature switched on, and its owner holds every permission. What
// each widget shows of a feature's own data, its feature's tests check at every size.
test('every widget fits each size it declares, as the seeded DSP shows it', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await login(page);
  await openDsp(page, dsp.name);
  for (const id of widgets.filter((id) => !customOwner(id)))
    await expectEverySize(page, dsp.id, id);
});

// A custom feature is only a DSP's with its owner's custom ID: the seeded DSP takes it and
// switches the feature on, as each custom feature's own tests do. A DSP keeps its custom ID while
// that ID owns features, so each owner's widgets have a test, and a seeded DSP, of their own.
for (const [customId, ids] of custom)
  test(`every widget of ${customId}'s custom features fits each size it declares`, async ({
    page,
    dispatch,
  }) => {
    const owner = await dispatch.client();
    const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
    const given = await owner.post(`/api/platform/dsps/${dsp.id}/custom-id`, { customId });
    expect(given.status, given.body).toBe(200);
    for (const feature of new Set(ids.map((id) => id.split('.')[0]))) {
      const on = await owner.post(`/api/platform/dsps/${dsp.id}/features`, {
        feature,
        enabled: true,
      });
      expect(on.status, on.body).toBe(200);
    }
    await login(page);
    await openDsp(page, dsp.name);
    for (const id of ids) await expectEverySize(page, dsp.id, id);
  });

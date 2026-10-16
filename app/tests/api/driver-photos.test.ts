import test from 'node:test';
import assert from 'node:assert/strict';
import type { Driver, DriverMatch } from '../../../features/driver_match/api/index.js';
import { collectRoster } from '../../../features/driver_profiles/tests/support/roster.js';
import { fixture, until } from '../../../core/shell/tests/support/support.js';

test("Driver Match shows the photo Driver Profiles' roster holds of each person", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  await collectRoster(owner);
  let pictured: Driver | undefined;
  await until(async () => {
    const result: DriverMatch = await owner.read('/api/dsp/driver-match');
    pictured = result.drivers.find((driver) => driver.photo);
    return Boolean(pictured);
  });
  assert.ok(pictured!.ids.some((id) => id.photo));
  const url = `/api/dsp/driver-match/drivers/${pictured!.code}/photo`;
  const photo = await f.raw(url, undefined, owner.headers);
  assert.equal(photo.status, 200);
  assert.equal(photo.headers.get('content-type'), 'image/png');
});

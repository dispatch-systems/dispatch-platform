import test from 'node:test';
import assert from 'node:assert/strict';
import type { DriverMatch, Person } from '../../../features/driver_match/api/index.js';
import type { Employee, EmployeesResponse } from '../../../features/timecard/api/index.js';
import { collectRoster } from '../../../features/driver_profiles/tests/support/roster.js';
import { fixture, until } from '../../../core/shell/tests/support/support.js';

test("Driver Match and Timecard's employees show the photo Driver Profiles' roster holds of each person", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  await collectRoster(owner);
  let pictured: Person | undefined;
  await until(async () => {
    const result: DriverMatch = await owner.read('/api/dsp/driver-match');
    pictured = result.people.find((person) => person.photo);
    return Boolean(pictured);
  });
  assert.ok(pictured!.ids.some((id) => id.photo));
  const url = `/api/dsp/driver-match/people/${pictured!.code}/photo`;
  const photo = await f.raw(url, undefined, owner.headers);
  assert.equal(photo.status, 200);
  assert.equal(photo.headers.get('content-type'), 'image/png');

  // An employee shows the photo of whoever Driver Match finds they are, through the directory.
  let employee: Employee | undefined;
  await until(async () => {
    const result: EmployeesResponse = await owner.read('/api/dsp/employees?limit=all');
    employee = result.employees.find((e) => e.photo);
    return Boolean(employee);
  });
  const { people }: DriverMatch = await owner.read('/api/dsp/driver-match');
  const person = people.find((d) =>
    d.ids.some((id) => id.source === 'paycom' && id.id === employee!.code),
  )!;
  assert.equal(employee!.photo, person.photo);
  const detail = await owner.read(`/api/dsp/employees/${employee!.code}`);
  assert.equal(detail.employee.photo, person.photo);
  const theirs = await f.raw(
    `/api/dsp/driver-match/people/${person.code}/photo`,
    undefined,
    owner.headers,
  );
  const served = await f.raw(
    `/api/dsp/employees/${employee!.code}/photo`,
    undefined,
    owner.headers,
  );
  assert.equal(served.status, 200);
  assert.equal(served.headers.get('content-type'), 'image/png');
  assert.deepEqual(
    Buffer.from(await served.arrayBuffer()),
    Buffer.from(await theirs.arrayBuffer()),
  );
});

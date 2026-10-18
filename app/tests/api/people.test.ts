import test from 'node:test';
import assert from 'node:assert/strict';
import type { Person as DirectoryPerson } from '../../../core/manifest/api/generated/Person.js';
import type { DriverMatch, Person } from '../../../features/driver_match/api/index.js';
import type { Employee, EmployeesResponse } from '../../../features/timecard/api/index.js';
import { collectRoster } from '../../../features/driver_profiles/tests/support/roster.js';
import { fixture, until } from '../../../core/shell/tests/support/support.js';

test("Driver Match, Timecard's employees and the directory show the photo Driver Profiles' roster holds of each person", async (t) => {
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
  const listed: DirectoryPerson = await owner.read(`/api/dsp/people/${pictured!.code}`);
  assert.deepEqual(
    [listed.name, listed.role, listed.photo],
    [pictured!.name, pictured!.role, pictured!.photo],
  );
  const photo = await f.raw(`/api/dsp/people/${pictured!.code}/photo`, undefined, owner.headers);
  assert.equal(photo.status, 200);
  assert.equal(photo.headers.get('content-type'), 'image/png');

  // An employee names the person the directory finds they are, and shows their photo.
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
  assert.deepEqual([employee!.person, employee!.photo], [person.code, person.photo]);
  const detail = await owner.read(`/api/dsp/employees/${employee!.code}`);
  assert.deepEqual([detail.employee.person, detail.employee.photo], [person.code, person.photo]);
});

test("every member of a DSP sees its people and their contacts, and no other DSP's", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = await collectRoster(owner);
  const role = (await owner.get('/api/dsp/roles')).value.find(
    (r: { name: string }) => r.name === 'Member',
  );
  const saved = await owner.post(`/api/dsp/roles/${role.id}`, { name: 'Member', permissions: [] });
  assert.equal(saved.status, 200, saved.body);
  // Changing a role ends the views signed before it.
  await owner.select(north.id);
  let everyone: DirectoryPerson[] = [];
  await until(async () => {
    everyone = await owner.read('/api/dsp/people');
    return everyone.some((person) => person.photo && person.emails.length);
  });
  const pictured = everyone.find((person) => person.photo && person.emails.length)!;

  const member = await f.client('member@dispatch.test');
  await member.select(north.id);
  assert.deepEqual(await member.read('/api/dsp/people'), everyone);
  assert.deepEqual(await member.read(`/api/dsp/people/${pictured.code}`), pictured);

  const summit = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  await owner.select(summit.id);
  assert.equal((await owner.get(`/api/dsp/people/${pictured.code}`)).status, 404);
  const elsewhere = await f.raw(`/api/dsp/people/${pictured.code}/photo`, undefined, owner.headers);
  assert.equal(elsewhere.status, 404);
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../../../core/shell/tests/support/support.js';
import { dispatchPhone, hrLine, paired, withHrLine } from '../support/line.js';

// HR Line keeps its contacts on its phone: a name saved on its page is handed to its phone to save
// and shown at once, and what the phone says of its own contacts is what the page shows; a number
// Driver Match knows is still its person's. Dispatch Phone's phones keep none.

type Contact = { number: string; person: { name: string } };
const JORDAN = '+14255550199';
const SAM = '+14255550123';

test("A name saved on HR Line goes to its phone, and the phone's contacts are what HR Line shows", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  await withHrLine(owner);
  const hr = await paired(f, owner, hrLine, '(206) 555-0111');
  const dispatch = await paired(f, owner, dispatchPhone, '(206) 555-0177');
  await hr.texted('4255550199', 'About my paycheck');
  let id = '';
  await until(async () => {
    const listed = (await owner.read(hrLine)).conversations as { id: string; number: string }[];
    id = listed.find((each) => each.number === JORDAN)?.id ?? '';
    return id !== '';
  });

  // Saved on the page, it shows at once and waits in the phone's outbox.
  const named = await owner.post(`${hrLine}/conversations/${id}/contact`, { name: 'Jordan Ellis' });
  assert.equal(named.value.with.name, 'Jordan Ellis', named.body);
  const outbox = await f.request('/api/communications/phone/outbox', undefined, hr.key);
  const [saving] = outbox.value.contacts as { number: string; name: string; at: number }[];
  assert.deepEqual([saving?.number, saving?.name], [JORDAN, 'Jordan Ellis'], outbox.body);

  // The phone saves it, then its own contacts change: a new one, and the saved one removed.
  const reported = (body: object) => f.request('/api/communications/phone/contacts', body, hr.key);
  const saved = await reported({ saved: [{ number: JORDAN, at: saving!.at }] });
  assert.equal(saved.status, 200, saved.body);
  const shown = async () =>
    ((await owner.read(`${hrLine}/texts/contacts`)).contacts as Contact[])
      .filter((each) => [JORDAN, SAM].includes(each.number))
      .map((each) => [each.number, each.person.name]);
  assert.deepEqual(await shown(), [[JORDAN, 'Jordan Ellis']]);
  const changed = await reported({
    changed: [
      { number: '(425) 555-0123', name: ' Sam Ortiz ' },
      { number: 'Carrier', name: 'X' },
    ],
    removed: [JORDAN],
  });
  assert.equal(changed.status, 200, changed.body);
  assert.deepEqual(await shown(), [[SAM, 'Sam Ortiz']]);
  const conversation = await owner.read(`${hrLine}/conversations/${id}`);
  assert.equal(conversation.conversation.with.name, '(425) 555-0199');

  // Malformed, or of none, it's refused; and a phone of Dispatch Phone's keeps no contacts.
  assert.equal((await reported({})).status, 400);
  assert.equal((await reported({ changed: [{ number: SAM }] })).status, 400);
  const theirs = await f.request(
    '/api/communications/phone/contacts',
    { removed: [SAM] },
    dispatch.key,
  );
  assert.equal(theirs.value.error, 'phones_contacts_not_kept', theirs.body);
});

import type {
  Communications,
  CommunicationsWorkspace,
  ComposeLimits,
  Conversation,
  Correspondent,
  Message,
  PersonRole,
  Profile,
  Sender,
} from '../../api/index.js';

// What Message display's preview shows: a made-up morning at the DSP, never its own
// conversations. Its times run up to `now`.

const team = 'T_PREVIEW';
const workspace: CommunicationsWorkspace = {
  teamId: team,
  name: 'Drivers',
  on: true,
  chatLink: '',
  needsUpdate: false,
  logo: null,
};
const person = (userId: string, name: string, role: PersonRole): Correspondent => ({
  userId,
  name,
  code: null,
  role,
  title: null,
  photo: null,
});
const teammate = (userId: string, name: string): Sender => ({
  userId,
  phoneId: null,
  name,
  code: null,
  photo: null,
});
const keisha = teammate('U_KEISHA', 'Keisha Brown');
const devon = teammate('U_DEVON', 'Devon Carter');
const jamal = person('U_JAMAL', 'Jamal Williams', 'driver');

/** The list, the conversation open in it, its messages and what's known of whom it's with. */
export function sample(now: number) {
  const ago = (minutes: number) => now - minutes * 60_000;
  const message = (id: string, text: string, minutes: number, sender?: Sender): Message => ({
    id,
    ts: `${ago(minutes) / 1000}`,
    direction: sender ? 'out' : 'in',
    text,
    at: ago(minutes),
    editedAt: null,
    inThread: false,
    sender: sender ?? null,
    files: [],
    group: null,
    status: null,
    number: null,
  });
  const messages = [
    message('m1', 'Morning. Van 208 won’t start, battery’s dead I think', 33),
    message('m2', 'Sorry Jamal, grabbing the jump pack now', 30, keisha),
    message('m3', 'Stay with the van, I’ll be there in 5', 30, keisha),
    message('m4', 'Jumped it but the battery light is still on', 19),
    message('m5', 'Don’t take 208 out. Swap to 214, keys are at the desk', 16, devon),
    message('m6', '214 is good. Leaving the station now', 10),
  ];
  const conversation = (
    id: string,
    who: Correspondent,
    last: Message,
    waiting: boolean,
  ): Conversation => ({
    id,
    kind: 'slack',
    teamId: team,
    number: null,
    driverId: null,
    contact: false,
    blocked: false,
    with: who,
    state: waiting ? 'waiting' : 'responded',
    waitingSince: waiting ? last.at : null,
    lastAt: last.at,
    last,
    matched: [],
    groupId: null,
  });
  const open = conversation('c_jamal', jamal, messages.at(-1)!, true);
  const conversations = [
    conversation(
      'c_sofia',
      person('U_SOFIA', 'Sofia Ramirez', 'driver'),
      message('s1', 'Running 10 min late, accident on 405', 49),
      true,
    ),
    conversation(
      'c_brianna',
      person('U_BRIANNA', 'Brianna Scott', 'driver'),
      message('b1', 'Can I swap Saturday with Tyler?', 31),
      true,
    ),
    conversation(
      'c_destiny',
      person('U_DESTINY', 'Destiny Harris', 'driver'),
      message('d1', 'Dog loose at stop 112, can’t get to the door', 17),
      true,
    ),
    open,
    conversation(
      'c_andre',
      person('U_ANDRE', 'Andre Thompson', 'driver'),
      message('a1', 'Code is 4471, try it again', 52, keisha),
      false,
    ),
    conversation(
      'c_kevin',
      person('U_KEVIN', "Kevin O'Brien", 'driver'),
      message('k1', 'Friday, same as always', 24 * 60, devon),
      false,
    ),
    conversation(
      'c_nia',
      person('U_NIA', 'Nia Robinson', 'helper'),
      message('n1', 'Badge is at the front desk', 26 * 60, keisha),
      false,
    ),
  ];
  const inbox: Communications = {
    revision: 0,
    waiting: 4,
    conversations,
    groups: [],
    more: false,
    moreTexts: false,
    workspaces: [workspace],
    texting: false,
    routes: false,
    phones: [],
    typing: { version: 0, writing: [] },
  };
  const profile: Profile = {
    today: {
      route: 'CX14',
      van: 'EDV 214',
      waveAt: ago(-70),
      stops: 168,
      stopsDone: null,
      pace: null,
      phone: null,
      phoneSince: null,
    },
    phone: '+12065550148',
    email: 'jamal.w@example.com',
    slack: workspace.name,
  };
  // What a message in it may carry, as a Slack conversation's read says.
  const limits: ComposeLimits = {
    text: 3000,
    photos: 10,
    largest: 20 * 1024 * 1024,
    kinds: ['image/png', 'image/jpeg', 'image/gif'],
  };
  return { inbox, open, messages, profile, workspace, limits };
}

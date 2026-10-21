import type {
  CommunicationsWorkspace,
  Contact,
  Correspondent,
  GroupChoice,
  GroupMember,
  PersonRole,
  Reachable,
} from '../api/index.js';
import { clock } from './format.js';

// Messaging a group: what's picked in To, whom that sends to, and what the group is called.
// Today's and the roles' groups cover every workspace where Communications works; the workspaces
// picked narrow them to those workspaces, Everyone to none; picked alone, a workspace is everyone
// in it. Someone picked by name always gets it. Each person gets it once.

/** Something picked in To: one of the groups to choose from, or someone by name. */
export type Pick =
  | { kind: 'group'; key: string; choice: GroupChoice }
  | { kind: 'person'; key: string; contact: Contact };

/** What a group of a role is called. */
const roles: Record<PersonRole, string> = {
  driver: 'Drivers',
  helper: 'Helpers',
  office: 'Office',
  former: 'Former',
};

/** The group's name, as its card and its chip say it: a wave by when it starts, in `timeZone`. */
export function choiceName(
  choice: GroupChoice,
  workspaces: readonly CommunicationsWorkspace[],
  timeZone: string,
) {
  switch (choice.kind) {
    case 'route':
      return 'On a route today';
    case 'wave':
      return `${clock(choice.waveAt ?? 0, timeZone)} wave`;
    case 'role':
      return choice.role ? roles[choice.role] : '';
    case 'everyone':
      return 'Everyone';
    case 'workspace':
      return (
        workspaces.find((workspace) => workspace.teamId === choice.teamId)?.name ??
        choice.teamId ??
        ''
      );
  }
}

/** Someone's key, as the groups to choose from name them: their person's code, else their
 * account. */
export const keyOf = (contact: Contact) =>
  contact.person.code ?? `${contact.teamId}:${contact.person.userId}`;
/** What picking `choice` is, to tell picks apart. */
export const choiceKey = (choice: GroupChoice) =>
  `${choice.kind}:${choice.waveAt ?? choice.role ?? choice.teamId ?? ''}`;

/** Whether a group narrows the others to its workspaces, as a workspace's and Everyone do. */
const narrows = (choice: GroupChoice) => choice.kind === 'workspace' || choice.kind === 'everyone';

/** Whom `picks` send to, of `people`: each person once, by the account picked by name, else their
 * first in a workspace picked. */
export function recipients(picks: readonly Pick[], people: readonly Reachable[]): GroupMember[] {
  const groups = picks.flatMap((pick) => (pick.kind === 'group' ? [pick.choice] : []));
  const places = groups.filter(narrows);
  const among = groups.filter((choice) => !narrows(choice));
  // Picked, Everyone means every workspace; a workspace, those picked.
  const within = places.some((choice) => choice.kind === 'everyone')
    ? undefined
    : new Set(places.map((choice) => choice.teamId));
  const fits = (member: GroupMember) => !within?.size || within.has(member.teamId);
  const keys = among.length
    ? new Set(among.flatMap((choice) => choice.people))
    : places.length
      ? new Set(people.map((person) => person.key))
      : new Set<string>();
  const chosen = new Map<string, GroupMember>();
  for (const person of people) {
    if (!keys.has(person.key)) continue;
    const member = person.members.find(fits);
    if (member) chosen.set(person.key, member);
  }
  for (const pick of picks)
    if (pick.kind === 'person')
      chosen.set(pick.key, { teamId: pick.contact.teamId, userId: pick.contact.person.userId });
  return [...chosen.values()];
}

/** The longest a group's name may be. */
const NAME_MOST = 100;
/** What a group of `picks` is called: one pick's name; several, joined, someone by their first
 * name, and past three, the first two and how many others. */
export function groupName(picks: readonly Pick[], named: (pick: Pick) => string) {
  if (picks.length === 1) return named(picks[0]!).slice(0, NAME_MOST);
  const names = picks.map((pick) =>
    pick.kind === 'person' ? (pick.contact.person.name.split(' ')[0] ?? '') : named(pick),
  );
  const joined =
    names.length > 3
      ? `${names.slice(0, 2).join(', ')} and ${names.length - 2} others`
      : `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;
  return joined.slice(0, NAME_MOST);
}

/** The first `most` of a group's people, whose faces show it. */
export function faces(
  choice: GroupChoice,
  people: ReadonlyMap<string, Reachable>,
  most = 3,
): Correspondent[] {
  return choice.people.flatMap((key) => people.get(key)?.person ?? []).slice(0, most);
}

/** How many people, as a group's card and header say it. */
export const peopleText = (count: number) => `${count} ${count === 1 ? 'person' : 'people'}`;

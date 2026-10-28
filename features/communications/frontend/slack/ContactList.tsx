import { useCallback } from 'react';
import type { CommunicationsWorkspace, Contact, Conversation } from '../../api/index.js';
import { getContacts } from '../../api/client.js';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ByInitial, ContactRow } from '../shared/ContactRow.js';
import { Face } from '../shared/Face.js';
import { ListEmpty, Rows } from '../shared/List.js';
import type { Target } from '../conversations/target.js';
import { roleText } from '../shared/format.js';
import { memberTarget } from './members.js';
import { useFound } from '../shared/useFound.js';

// Contacts: everyone the DSP can message, of every workspace or the one chosen, A to Z under
// each name's first letter; or those found by a name typed, misspelt or not, the closest first.
// Each shows how long they've waited, or when they last wrote, and opens their conversation, or
// starts one.

export function ContactList({
  team,
  typed,
  revision,
  workspaces,
  current,
  timeZone,
  now,
  reach,
}: {
  /** The workspace chosen, if one is. */
  team?: string;
  /** What's typed in Search, trimmed. */
  typed: string;
  /** The conversations' latest change, which reads them again. */
  revision: number;
  /** The workspaces Communications works in, which each says it's in while there are several. */
  workspaces: readonly CommunicationsWorkspace[];
  /** The conversation open. */
  current?: Conversation;
  timeZone: string;
  now: number;
  /** Opens the conversation with whoever was picked, or starts it. */
  reach: (target: Target) => Promise<void>;
}) {
  const read = useCallback(
    () => getContacts(typed, team).then((found) => found.contacts),
    [typed, team],
  );
  // Those found by a name are another list from all of them, as is each workspace's.
  const list = `${team ?? ''} ${typed ? 'found' : 'all'}`;
  const { found, failed } = useFound(read, list, revision, typed ? 200 : 0);
  const chosen = useAction(reach);
  if (failed || (typed && found?.length === 0))
    return <ListEmpty>{failed || 'No matches'}</ListEmpty>;
  if (!found) return <Loading />;
  // With every workspace shown, each says which it's in.
  const named = new Map(workspaces.map((workspace) => [workspace.teamId, workspace.name]));
  const where = !team && named.size > 1 ? named : undefined;
  const row = (contact: Contact) => (
    <ContactRow
      key={`${contact.teamId}:${contact.person.userId}`}
      face={<Face {...contact.person} size="contact" />}
      name={contact.person.name}
      matched={contact.matched}
      detail={[roleText(contact.person), where?.get(contact.teamId)].filter(Boolean).join(' · ')}
      waitingSince={contact.waitingSince}
      lastAt={contact.lastAt}
      selected={current?.teamId === contact.teamId && current.with.userId === contact.person.userId}
      timeZone={timeZone}
      now={now}
      open={() => {
        const target = memberTarget(contact);
        if (target && !chosen.busy) void chosen.run(target);
      }}
    />
  );
  if (typed) return <Rows>{found.map(row)}</Rows>;
  return <ByInitial contacts={found} nameOf={(contact) => contact.person.name} row={row} />;
}

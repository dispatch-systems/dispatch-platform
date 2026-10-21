import { useState } from 'react';
import type {
  Communications,
  CommunicationsWorkspace,
  Contact,
  Conversation,
  Writing,
} from '../../api/index.js';
import { SearchInput, Tabs } from '../../../../core/shell/frontend/ui/index.js';
import { ContactList } from './ContactList.js';
import { ConversationList } from './ConversationList.js';
import { WorkspacePicker } from './WorkspacePicker.js';

// The list beside the conversation: which workspace it shows, with two or more where
// Communications works; Search; and Messages, the conversations, or Contacts, everyone the DSP
// can message, whichever Search searches.

export function ListPane({
  inbox,
  working,
  team,
  choose,
  pinned,
  current,
  readOlder,
  pick,
  ...listing
}: {
  inbox: Communications;
  /** The workspaces Communications works in. */
  working: readonly CommunicationsWorkspace[];
  /** The workspace chosen, if one is. */
  team?: string;
  choose: (team?: string) => void;
  /** The conversation open, while the list doesn't hold it. */
  pinned?: Conversation;
  /** The conversation open, as last read. */
  current?: Conversation;
  readOlder: () => void;
  /** Opens the conversation with a contact, or starts one. */
  pick: (contact: Contact) => Promise<void>;
  open?: string;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  select: (id: string) => void;
  done: (conversation: Conversation) => void;
}) {
  const [contacts, setContacts] = useState(false);
  const [typed, setTyped] = useState('');
  const name = typed.trim();
  // How many wait, in all and in each workspace: every conversation waiting is listed.
  const waitingIn = new Map<string, number>();
  for (const conversation of inbox.conversations)
    if (conversation.state === 'waiting')
      waitingIn.set(conversation.teamId, (waitingIn.get(conversation.teamId) ?? 0) + 1);
  const waiting = team ? (waitingIn.get(team) ?? 0) : inbox.waiting;
  return (
    <section className="pane-list" aria-label={contacts ? 'Contacts' : 'Conversations'}>
      <div className="list-tools">
        {working.length > 1 && (
          <WorkspacePicker
            workspaces={working}
            team={team}
            waiting={{ all: inbox.waiting, in: waitingIn }}
            choose={choose}
          />
        )}
        <SearchInput
          label={contacts ? 'Search contacts' : 'Search'}
          placeholder={contacts ? 'Search contacts' : 'Search'}
          value={typed}
          onChange={setTyped}
        />
        <Tabs
          label="Show"
          value={contacts ? 'contacts' : 'messages'}
          onChange={(value) => setContacts(value === 'contacts')}
          items={[
            [
              'messages',
              <>
                Messages
                {waiting > 0 && <span className="count">{waiting}</span>}
              </>,
            ],
            ['contacts', 'Contacts'],
          ]}
        />
      </div>
      <div className="list-scroll">
        {contacts ? (
          <ContactList
            team={team}
            typed={name}
            revision={inbox.revision}
            workspaces={working}
            current={current}
            timeZone={listing.timeZone}
            now={listing.now}
            pick={pick}
          />
        ) : (
          <ConversationList
            inbox={inbox}
            team={team}
            typed={name}
            pinned={pinned}
            readOlder={readOlder}
            {...listing}
          />
        )}
      </div>
    </section>
  );
}

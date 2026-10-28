import { useState } from 'react';
import type {
  Communications,
  CommunicationsWorkspace,
  Conversation,
  Group,
  Writing,
} from '../../api/index.js';
import { SearchInput } from '../../../../core/shell/frontend/ui/index.js';
import type { Target } from '../conversations/target.js';
import { ConversationList } from '../inbox/ConversationList.js';
import { waitingIn, type Row } from '../inbox/inbox.js';
import { sectionNamed, type Section } from '../inbox/sections.js';
import { Count } from '../shared/Count.js';
import { Segments } from '../shared/Segments.js';
import type { Draft } from '../slack/groups/GroupDraft.js';
import { WorkspacePicker } from '../slack/WorkspacePicker.js';
import { VIEWS, type ListContext, type RoutesShown } from './section-views.js';
import { Sections } from './Sections.js';
import './list-pane.css';

// The list beside the conversation: its sections, while there are two or more; in a section of
// workspaces, which it shows, with two or more where Communications works; Search, and what the
// section shows beside it; and Messages, the conversations, or Contacts, whom the section can
// reach, whichever Search searches; or the section's own rows, as Today's Routes' (each
// section's model and view: inbox/sections.ts, section-views.tsx).

export function ListPane({
  inbox,
  sections,
  section,
  chooseSection,
  working,
  team,
  choose,
  draft,
  pinned,
  current,
  readOlder,
  reach,
  show,
  routes,
  ...listing
}: {
  inbox: Communications;
  /** Today's Routes as the page reads it. */
  routes: RoutesShown;
  /** The list's sections, and the one shown. */
  sections: readonly Section[];
  section: Section;
  chooseSection: (section: Section) => void;
  /** The workspaces Communications works in. */
  working: readonly CommunicationsWorkspace[];
  /** The workspace chosen, in a section of workspaces. */
  team?: string;
  choose: (team?: string) => void;
  /** The message being written to a group, while it is. */
  draft?: Draft;
  /** What's open, while the list doesn't show it. */
  pinned?: Row;
  /** The conversation open, as last read. */
  current?: Conversation;
  readOlder: (section: Section) => void;
  /** Opens the conversation with a contact, or starts one. */
  reach: (target: Target) => Promise<void>;
  /** Shows a conversation as the page's own writes left it. */
  show: (conversation: Conversation) => void;
  /** What's open: a conversation or a group, by its ID. */
  open?: string;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  select: (id: string) => void;
  openGroup: (id: string) => void;
  done: (conversation: Conversation) => void;
  doneGroup: (group: Group) => void;
}) {
  const [contacting, setContacts] = useState(false);
  const [typed, setTyped] = useState('');
  const name = typed.trim();
  const model = sectionNamed(section);
  const view = VIEWS[section];
  const contacts = contacting && view.contacts !== undefined;
  // How many wait, in each section and in each workspace: every conversation waiting is listed,
  // those under a group too.
  const waits = waitingIn(inbox);
  const inWorkspace = new Map<string, number>();
  for (const conversation of inbox.conversations)
    if (conversation.state === 'waiting' && conversation.teamId)
      inWorkspace.set(conversation.teamId, (inWorkspace.get(conversation.teamId) ?? 0) + 1);
  const waiting = model.workspaces && team ? (inWorkspace.get(team) ?? 0) : waits[section];
  const context: ListContext = {
    inbox,
    typed: name,
    team,
    working,
    current,
    reach,
    show,
    routes,
    ...listing,
  };
  return (
    <section className="pane-list" aria-label={contacts ? 'Contacts' : 'Conversations'}>
      <div className="list-tools">
        {sections.length > 1 && (
          <Sections sections={sections} section={section} waiting={waits} choose={chooseSection} />
        )}
        {model.workspaces && working.length > 1 && (
          <WorkspacePicker
            workspaces={working}
            team={team}
            waiting={{ all: waits[section], in: inWorkspace }}
            choose={choose}
          />
        )}
        <div className="tools-row">
          <SearchInput
            label={contacts ? 'Search contacts' : 'Search'}
            placeholder={contacts ? 'Search contacts' : 'Search'}
            value={typed}
            onChange={setTyped}
          />
          {view.tools?.(context)}
        </div>
        {view.contacts && (
          <Segments
            label="Show"
            value={contacts ? 'contacts' : 'messages'}
            onChange={(value) => setContacts(value === 'contacts')}
            items={[
              [
                'messages',
                <>
                  Messages
                  <Count waiting={waiting} />
                </>,
              ],
              ['contacts', 'Contacts'],
            ]}
          />
        )}
      </div>
      <div className="list-scroll">
        {contacts ? (
          view.contacts?.(context)
        ) : view.rows && !name ? (
          view.rows(context)
        ) : (
          <ConversationList
            inbox={inbox}
            section={section}
            team={team}
            typed={name}
            waiting={waiting}
            draft={model.groups ? draft : undefined}
            pinned={pinned}
            readOlder={() => readOlder(section)}
            {...listing}
          />
        )}
      </div>
    </section>
  );
}

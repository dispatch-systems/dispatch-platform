import { useState } from 'react';
import type {
  Communications,
  CommunicationsWorkspace,
  Contact,
  Conversation,
  Group,
  TextContact,
  Writing,
} from '../../api/index.js';
import { waitingIn, type Row, type Section } from '../inbox/inbox.js';
import type { RouteGroup, RouteRow } from '../todays-routes/todays-routes.js';
import { SearchInput, Tabs } from '../../../../core/shell/frontend/ui/index.js';
import { ContactList } from '../slack/ContactList.js';
import { ConversationList } from '../inbox/ConversationList.js';
import type { Draft } from '../slack/groups/GroupDraft.js';
import { RoutesList } from '../todays-routes/RoutesList.js';
import { Sections } from './Sections.js';
import { TextContactList } from '../texting/TextContactList.js';
import { ViewToggle, type RoutesView } from '../todays-routes/ViewToggle.js';
import { WorkspacePicker } from '../slack/WorkspacePicker.js';

// The list beside the conversation: its sections, while there are two or more; in Slack's,
// which workspace it shows, with two or more where Communications works; Search; and Messages,
// the conversations, or Contacts, everyone the DSP can message or text, whichever Search
// searches. In Today's Routes, Search finds drivers' work conversations, and the list or the board
// is chosen beside it.

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
  pick,
  pickNumber,
  show,
  routes,
  ...listing
}: {
  inbox: Communications;
  /** Today's Routes, in its groups, and choosing a row of it; and its view, while the board can
   * show. */
  routes: {
    groups: readonly RouteGroup[];
    choose: (row: RouteRow) => void;
    /** The driver whose conversation is open, if it's a driver's. */
    driver?: string | null;
    view?: { shown: RoutesView; choose: (view: RoutesView) => void };
  };
  /** The list's sections, and the one shown. */
  sections: readonly Section[];
  section: Section;
  chooseSection: (section: Section) => void;
  /** The workspaces Communications works in. */
  working: readonly CommunicationsWorkspace[];
  /** The workspace chosen, if one is. */
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
  pick: (contact: Contact) => Promise<void>;
  /** Opens the conversation with someone to text, or starts one. */
  pickNumber: (contact: TextContact) => Promise<void>;
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
  const routing = section === 'routes';
  const contacts = contacting && !routing;
  // How many wait, in each section and in each workspace: every conversation waiting is listed,
  // those under a group too.
  const waits = waitingIn(inbox);
  const inWorkspace = new Map<string, number>();
  for (const conversation of inbox.conversations)
    if (conversation.state === 'waiting' && conversation.teamId)
      inWorkspace.set(conversation.teamId, (inWorkspace.get(conversation.teamId) ?? 0) + 1);
  const slack = section === 'slack';
  const waiting = slack && team ? (inWorkspace.get(team) ?? 0) : waits[section];
  return (
    <section className="pane-list" aria-label={contacts ? 'Contacts' : 'Conversations'}>
      <div className="list-tools">
        {sections.length > 1 && (
          <Sections sections={sections} section={section} waiting={waits} choose={chooseSection} />
        )}
        {slack && working.length > 1 && (
          <WorkspacePicker
            workspaces={working}
            team={team}
            waiting={{ all: waits.slack, in: inWorkspace }}
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
          {routing && routes.view && (
            <ViewToggle view={routes.view.shown} choose={routes.view.choose} />
          )}
        </div>
        {!routing && (
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
        )}
      </div>
      <div className="list-scroll">
        {routing && !name ? (
          <RoutesList
            groups={routes.groups}
            open={listing.open}
            openDriver={routes.driver}
            typing={listing.typing}
            timeZone={listing.timeZone}
            now={listing.now}
            choose={routes.choose}
            done={listing.done}
          />
        ) : contacts && slack ? (
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
        ) : contacts ? (
          <TextContactList
            typed={name}
            revision={inbox.revision}
            current={current}
            timeZone={listing.timeZone}
            now={listing.now}
            pick={pickNumber}
            show={show}
          />
        ) : (
          <ConversationList
            inbox={inbox}
            section={section}
            team={slack ? team : undefined}
            typed={name}
            waiting={waiting}
            draft={slack ? draft : undefined}
            pinned={pinned}
            readOlder={() => readOlder(section)}
            {...listing}
          />
        )}
      </div>
    </section>
  );
}

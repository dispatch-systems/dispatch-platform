import type { ReactNode } from 'react';
import { Phone, Route } from 'lucide-react';
import type {
  Communications,
  CommunicationsWorkspace,
  Conversation,
  Writing,
} from '../../api/index.js';
import { SlackLogo } from '../../../slack/frontend/index.js';
import type { Target } from '../conversations/target.js';
import type { Section } from '../inbox/sections.js';
import { ContactList } from '../slack/ContactList.js';
import { TextContactList } from '../texting/TextContactList.js';
import { RoutesBoard } from '../todays-routes/RoutesBoard.js';
import { RoutesList } from '../todays-routes/RoutesList.js';
import type { RouteGroup, RouteRow } from '../todays-routes/todays-routes.js';
import { ViewToggle, type RoutesView } from '../todays-routes/ViewToggle.js';

// How each of the list's sections shows, defined once, keyed by the IDs inbox/sections.ts defines
// them by, so a section added there needs its view here: its label and icon on the switch; whom
// it can reach, under Contacts; its own rows in place of Messages, and what shows beside Search;
// and a board across the page, which the member can choose in place of the list. The page owns
// which section shows, what's open and how the page is laid out.

/** Today's Routes as the page reads it, in its groups, and choosing a row of it; the driver whose
 * conversation is open; and its view, while the board can show. */
export type RoutesShown = {
  groups: readonly RouteGroup[];
  choose: (row: RouteRow) => void;
  driver?: string | null;
  view?: { shown: RoutesView; choose: (view: RoutesView) => void };
};

/** What a section's list is drawn with. */
export type ListContext = {
  inbox: Communications;
  /** What's typed in Search, trimmed. */
  typed: string;
  /** The workspace chosen, in a section of workspaces. */
  team?: string;
  /** The workspaces Communications works in. */
  working: readonly CommunicationsWorkspace[];
  /** The conversation open, as last read. */
  current?: Conversation;
  /** What's open: a conversation or a group, by its ID. */
  open?: string;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  /** Opens a conversation, or starts one. */
  reach: (target: Target) => Promise<void>;
  /** Shows a conversation as the page's own writes left it. */
  show: (conversation: Conversation) => void;
  done: (conversation: Conversation) => void;
  routes: RoutesShown;
};

/** What a section's board is drawn with: the switch and the view above it. */
export type BoardContext = Pick<ListContext, 'typing' | 'timeZone' | 'now' | 'routes'> & {
  tools: ReactNode;
};

type SectionView = {
  label: ReactNode;
  icon: ReactNode;
  /** Whom it can reach, under Contacts beside Messages; none for a section without. */
  contacts?: (context: ListContext) => ReactNode;
  /** Its own rows while nothing is searched, in place of Messages. */
  rows?: (context: ListContext) => ReactNode;
  /** The first conversation its own rows list, which opens beside the list. */
  first?: (groups: readonly RouteGroup[]) => Conversation | undefined;
  /** What shows beside Search. */
  tools?: (context: ListContext) => ReactNode;
  /** A board across the page, which the member may choose in place of the list. */
  board?: (context: BoardContext) => ReactNode;
};

export const VIEWS: { [S in Section]: SectionView } = {
  // On a phone, or with all three in a narrow list, Today's Routes goes by Routes.
  routes: {
    label: (
      <>
        <span className="long">Today's Routes</span>
        <span className="short">Routes</span>
      </>
    ),
    icon: <Route size={14} aria-hidden="true" />,
    rows: ({ routes, open, typing, timeZone, now, done }) => (
      <RoutesList
        groups={routes.groups}
        open={open}
        openDriver={routes.driver}
        typing={typing}
        timeZone={timeZone}
        now={now}
        choose={routes.choose}
        done={done}
      />
    ),
    first: (groups) =>
      groups.flatMap((each) => each.rows).find((row) => row.conversation)?.conversation,
    tools: ({ routes }) =>
      routes.view && <ViewToggle view={routes.view.shown} choose={routes.view.choose} />,
    board: ({ tools, routes, typing, timeZone, now }) => (
      <RoutesBoard
        tools={tools}
        groups={routes.groups}
        typing={typing}
        timeZone={timeZone}
        now={now}
        choose={routes.choose}
      />
    ),
  },
  messaging: {
    label: 'Messaging',
    icon: <Phone size={14} aria-hidden="true" />,
    contacts: ({ typed, inbox, current, timeZone, now, reach, show }) => (
      <TextContactList
        typed={typed}
        revision={inbox.revision}
        current={current}
        timeZone={timeZone}
        now={now}
        reach={reach}
        show={show}
      />
    ),
  },
  slack: {
    label: 'Slack',
    icon: <SlackLogo size={14} />,
    contacts: ({ typed, team, inbox, working, current, timeZone, now, reach }) => (
      <ContactList
        team={team}
        typed={typed}
        revision={inbox.revision}
        workspaces={working}
        current={current}
        timeZone={timeZone}
        now={now}
        reach={reach}
      />
    ),
  },
};

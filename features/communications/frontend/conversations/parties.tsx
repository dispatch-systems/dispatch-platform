import type { ReactNode } from 'react';
import type { CommunicationsWorkspace, Conversation, RouteDriver, Today } from '../../api/index.js';
import { lastFour, numberBeside, roleText } from '../shared/format.js';
import { NumberActions } from '../texting/NumberActions.js';
import { PickDriver } from '../todays-routes/PickDriver.js';
import { RouteToday } from '../todays-routes/RouteFacts.js';
import type { Party } from './party.js';

// What the conversation open does for whom it's with, by who they are (`Party`), defined once
// each: whether they have a profile, what its head says of them and what else it does there, the
// number a message goes to and whether one reaches them now, whom the box says it goes to, and
// whether its days are a driver's workdays.

/** What the pane knows of the conversation open. */
export type Open = {
  conversation: Conversation;
  /** For a driver's, their route today and the work phone they hold now, while there's a live
   * day. */
  today: Today | null;
  /** The workspace it's in, while the DSP has two or more where Communications works. */
  where?: string;
  /** The workspaces Communications works in. */
  working: readonly CommunicationsWorkspace[];
  /** Whether the DSP texts. */
  texting: boolean;
  /** Today's drivers, whom a work phone nobody held can be picked for. */
  drivers: readonly RouteDriver[];
  timeZone: string;
  /** Shows a conversation as the page's own writes left it. */
  show: (conversation: Conversation) => void;
  /** Opens another conversation, as the page's own write left it. */
  follow: (conversation: Conversation) => void;
};

type PartyPane = {
  /** Whether they have a profile, opened from who it's with, and from their photo by their
   * messages too (`fromPhoto`); and whether it goes to each of their conversations (`ways`). */
  profile: false | { fromPhoto: boolean; ways: boolean };
  /** The number a message goes to: none in Slack. */
  number: (open: Open) => string | null;
  /** Whether a message reaches them now: one that can't shows No work phone in the box's place. */
  reachable?: (open: Open) => boolean;
  /** Whether a message can be written to go now. */
  writable: (open: Open) => boolean;
  /** What the head says of them. */
  about: (open: Open) => ReactNode;
  /** What else can be done with it, in the head. */
  actions?: (open: Open) => ReactNode;
  /** Whom the box says a message goes to. */
  whom: (open: Open) => string;
  /** Whether each day of it begins with the workday's line. */
  workdays: boolean;
};

/** Their first name, as a message to someone Driver Match knows goes. */
const firstName = ({ conversation }: Open) => conversation.with.name.split(' ')[0] ?? '';
/** Their first name, once Driver Match knows them; else the name saved, or the number, whole. */
const named = (open: Open) =>
  open.conversation.with.code ? firstName(open) : open.conversation.with.name;

export const PARTIES: { [P in Party]: PartyPane } = {
  member: {
    profile: { fromPhoto: true, ways: false },
    number: () => null,
    writable: ({ conversation, working }) =>
      working.some((workspace) => workspace.teamId === conversation.teamId),
    about: ({ conversation, where }) =>
      [roleText(conversation.with), where].filter(Boolean).join(' · '),
    whom: firstName,
    workdays: false,
  },
  number: {
    profile: false,
    number: ({ conversation }) => conversation.number,
    writable: ({ texting }) => texting,
    about: ({ conversation }) => numberBeside(conversation.with.name, conversation.number ?? ''),
    actions: ({ conversation, show }) => <NumberActions conversation={conversation} show={show} />,
    whom: named,
    workdays: false,
  },
  driver: {
    profile: { fromPhoto: false, ways: true },
    number: ({ today }) => today?.phone ?? null,
    reachable: ({ today }) => Boolean(today?.phone),
    writable: ({ texting }) => texting,
    about: ({ today }) => today && <RouteToday today={today} />,
    whom: named,
    workdays: true,
  },
  work_phone: {
    profile: false,
    number: ({ conversation }) => conversation.number,
    writable: ({ texting }) => texting,
    about: () => 'No driver',
    actions: ({ conversation, drivers, timeZone, follow }) =>
      drivers.length > 0 && (
        <PickDriver
          conversation={conversation}
          drivers={drivers}
          timeZone={timeZone}
          picked={follow}
        />
      ),
    whom: ({ conversation }) => lastFour(conversation.number ?? ''),
    workdays: false,
  },
};

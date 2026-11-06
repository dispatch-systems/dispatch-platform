import type { ReactNode } from 'react';
import { Phone, Smartphone } from 'lucide-react';
import type { CommunicationsWorkspace, Conversation, RouteDriver, Today } from '../../api/index.js';
import { phoneText } from '../../../../core/shell/frontend/lib/format.js';
import { SlackLogo } from '../../../slack/frontend/index.js';
import { lastFour, numberBeside, roleText } from '../shared/format.js';
import { NumberActions } from '../texting/NumberActions.js';
import { PickDriver } from '../todays-routes/PickDriver.js';
import { Reach, RouteCode } from '../todays-routes/RouteFacts.js';
import type { Party } from './party.js';

// What the conversation open does for whom it's with, by who they are (`Party`), defined once
// each: whether they have a profile, what its head says of them, alike on each of their
// conversations (`headLine`), and what else it does there, the
// number a message goes to and whether one reaches them now, whom the box says it goes to, and
// whether its days are a driver's workdays.

/** What the pane knows of the conversation open. */
export type Open = {
  conversation: Conversation;
  /** For a driver's, their route today and the work phone they hold now, while there's a live
   * day, as its head and profile both show them (`driverToday`); for anyone else's, the route
   * they drive today as the conversation was read, whose van its head shows. */
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
   * messages too (`fromPhoto`); whether it goes to each of their conversations (`ways`); and
   * whether its Today is the conversation's own, live, as its head's is (`today`), else read with
   * it. */
  profile: false | { fromPhoto: boolean; ways: boolean; today: boolean };
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

/** What the head says of whom it's with, alike on each of their conversations: the route they
 * drive today and its van, then how this conversation reaches them (`reach`), on one line, the van
 * and a workspace giving way first (`gives`); none while it would say nothing. */
function headLine({ today }: Open, reach: ReactNode): ReactNode {
  if (!today?.route && !today?.van && !reach) return null;
  return (
    <>
      {today?.route && <RouteCode code={today.route} />}
      {today?.van && <span className="gives">Van {today.van}</span>}
      {reach}
    </>
  );
}
/** Their first name, as a message to someone Driver Match knows goes. */
const firstName = ({ conversation }: Open) => conversation.with.name.split(' ')[0] ?? '';
/** Their first name, once Driver Match knows them; else the name saved, or the number, whole. */
const named = (open: Open) =>
  open.conversation.with.code ? firstName(open) : open.conversation.with.name;

export const PARTIES: { [P in Party]: PartyPane } = {
  member: {
    profile: { fromPhoto: true, ways: false, today: false },
    number: () => null,
    writable: ({ conversation, working }) =>
      working.some((workspace) => workspace.teamId === conversation.teamId),
    about: (open) => {
      const role = roleText(open.conversation.with);
      return headLine(
        open,
        (role || open.where) && (
          <>
            <Reach icon={<SlackLogo size={12} />}>{role || open.where}</Reach>
            {role && open.where && <span className="gives">{open.where}</span>}
          </>
        ),
      );
    },
    whom: firstName,
    workdays: false,
  },
  number: {
    profile: false,
    number: ({ conversation }) => conversation.number,
    writable: ({ texting }) => texting,
    about: (open) => {
      const number = numberBeside(open.conversation.with.name, open.conversation.number ?? '');
      return headLine(open, number && <Reach icon={<Phone aria-hidden="true" />}>{number}</Reach>);
    },
    actions: ({ conversation, drivers, show }) => (
      <NumberActions conversation={conversation} drivers={drivers} show={show} />
    ),
    whom: named,
    workdays: false,
  },
  driver: {
    profile: { fromPhoto: false, ways: true, today: true },
    number: ({ today }) => today?.phone ?? null,
    reachable: ({ today }) => Boolean(today?.phone),
    writable: ({ texting }) => texting,
    about: (open) =>
      headLine(
        open,
        open.today?.phone && (
          <Reach icon={<Smartphone aria-hidden="true" />}>{phoneText(open.today.phone)}</Reach>
        ),
      ),
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

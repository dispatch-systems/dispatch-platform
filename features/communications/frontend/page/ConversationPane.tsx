import { useEffect, type ReactNode } from 'react';
import type {
  CommunicationsWorkspace,
  Conversation,
  Elsewhere,
  Message,
  RouteDriver,
  SentMessage,
  Today,
  Writing,
} from '../../api/index.js';
import { markDone, retryText, sayTyping, sendMessage } from '../../api/client.js';
import { ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { lastFour, numberBeside } from '../format.js';
import { elsewhereNow } from '../inbox.js';
import { Composer } from './Composer.js';
import { ConversationHead } from './ConversationHead.js';
import { ElsewhereChips } from './Elsewhere.js';
import { NoWorkPhone } from './NoWorkPhone.js';
import { NumberActions } from './NumberActions.js';
import { roleText } from './parts.js';
import { PickDriver } from './PickDriver.js';
import { Profile } from './Profile.js';
import { RouteCode, WorkPhone } from './RouteFacts.js';
import { Thread } from './Thread.js';
import { useConversation } from './useConversation.js';
import { usePanel } from './usePanel.js';
import type { Changed } from './useInbox.js';

// The conversation open: who it's with, the workspace it's in while there are several, the number
// it texts, or a driver's route today and the work phone they hold, and, while it waits, how long
// and Done; what can be done with its number, or with a work phone nobody held, Pick driver; their
// conversations of other kinds, as chips; its messages; and writing in it, by text through the
// DSP's phones, or for a driver holding no work phone, their other conversations instead. On a
// phone it takes the page, with a way back to the list. In Slack, or a driver's, who it's with, or
// their photo by their messages, opens their profile over it, or beside it on a wide page
// (`docked`).

export function ConversationPane({
  id,
  listed,
  conversations,
  workspaces,
  texting,
  docked,
  kept,
  seen,
  changed,
  typing,
  timeZone,
  now,
  show,
  back,
  closed,
  openGroup,
  drivers,
  go,
}: {
  id: string;
  /** The conversation as the list has it, which the page's own writes and each update keep. */
  listed?: Conversation;
  /** The list's conversations, kept so, which its chips follow. */
  conversations: readonly Conversation[];
  /** The DSP's workspaces, as each update brings them. */
  workspaces: readonly CommunicationsWorkspace[];
  /** Whether the DSP texts. */
  texting: boolean;
  /** Whether the profile, open, is a column of its own beside the conversation. */
  docked: boolean;
  /** Whether the profile is open as the conversation opens, as the member may keep it. */
  kept: boolean;
  /** Tells the page of the conversation as read, which pins one the list doesn't hold. */
  seen: (conversation: Conversation) => void;
  changed: Changed;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  /** Shows the conversation in the list as the page's own writes left it. */
  show: (conversation: Conversation) => void;
  back: () => void;
  /** Leaves a conversation that's no longer there. */
  closed: () => void;
  /** Opens a group, as one a message went to. */
  openGroup: (id: string) => void;
  /** Today's drivers, whom a work phone nobody held can be picked for. */
  drivers: readonly RouteDriver[];
  /** Goes to whom it's with's conversation of another kind. */
  go: (elsewhere: Elsewhere) => void;
}) {
  const { opened, error, gone, readOlder, ...read } = useConversation(id, changed);
  useEffect(() => {
    if (gone) closed();
  }, [gone, closed]);
  useEffect(() => {
    if (opened) seen(opened.conversation);
  }, [opened, seen]);
  const conversation = listed ?? opened?.conversation;
  const texted = conversation !== undefined && conversation.kind !== 'slack';
  // A driver's work conversation, or a work phone's nobody held.
  const driver = conversation?.kind === 'work' && conversation.driverId !== null;
  const workPhone = conversation?.kind === 'work' && !driver;
  const today = opened?.today ?? null;
  const elsewhere = elsewhereNow(opened?.elsewhere ?? [], conversations);
  const number = (driver ? today?.phone : conversation?.number) ?? '';
  const working = workspaces.filter((workspace) => workspace.on);
  // A driver's goes to the work phone they hold now.
  const unreachable = driver && opened !== undefined && !today?.phone;
  const writable = texted
    ? texting && !unreachable
    : working.some((workspace) => workspace.teamId === conversation?.teamId);
  // With several workspaces to tell apart, the one it's in.
  const where =
    working.length > 1
      ? workspaces.find((workspace) => workspace.teamId === conversation?.teamId)?.name
      : undefined;
  const done = useAction(async () => {
    const after = await markDone(id);
    read.show(after);
    show(after);
  });
  const sent = ({ conversation: theirs, message }: SentMessage) => {
    if (!conversation) return;
    // Filed in another conversation, as its number changed hands as it went: follow it there.
    if (theirs.id !== id) {
      show(theirs);
      go(asElsewhere(theirs));
      return;
    }
    // Answered, it waits no more, unless it didn't go; the next update says so for everyone.
    const answered: Partial<Conversation> =
      message.status === 'failed' ? {} : { state: 'responded', waitingSince: null };
    const after: Conversation = { ...conversation, ...answered, lastAt: message.at, last: message };
    read.show(after, message);
    show(after);
  };
  const retry = useAction(async (failed: Message) => {
    if (conversation) read.show(conversation, await retryText(id, failed.id));
  });
  const writing = typing.filter((writer) => writer.conversationId === id);
  const person = conversation?.with;
  // What they do for the DSP, and where; a driver's route and work phone; a work phone that no
  // driver held; or, by text, their number, while it isn't their name.
  const about: ReactNode = driver
    ? today && <RouteToday today={today} />
    : workPhone
      ? 'No driver'
      : texted
        ? numberBeside(person?.name ?? '', number)
        : [person && roleText(person), where].filter(Boolean).join(' · ');
  // The profile, and what opened it, which focus goes back to as it closes: the photo by the
  // messages, or who it's with when that photo is gone. A number's or a work phone's has none.
  const profiled = !texted || driver;
  const profile = usePanel(kept && profiled);
  const opens = profiled ? profile.opens : undefined;
  // Whom it goes to, by name, or a work phone by its last digits.
  const whom = workPhone
    ? lastFour(number)
    : texted && !person?.code
      ? (person?.name ?? '')
      : (person?.name.split(' ')[0] ?? '');
  return (
    <section
      className={profile.open ? 'pane-thread profiled' : 'pane-thread'}
      aria-label={person?.name ?? 'Conversation'}
    >
      <ConversationHead
        conversation={conversation}
        about={about}
        now={now}
        who={profile.who}
        open={profile.open}
        opens={opens}
        back={back}
        done={{ busy: done.busy, run: () => void done.run() }}
        actions={
          conversation?.kind === 'number' ? (
            <NumberActions conversation={conversation} show={show} />
          ) : (
            workPhone &&
            drivers.length > 0 && (
              <PickDriver
                conversation={conversation}
                drivers={drivers}
                timeZone={timeZone}
                picked={(theirs) => {
                  show(theirs);
                  go(asElsewhere(theirs));
                }}
              />
            )
          )
        }
      />
      <ElsewhereChips elsewhere={elsewhere} current={id} go={go} />
      {profile.drawn && person && profiled && (
        <Profile
          id={profile.id}
          conversation={id}
          person={person}
          workspace={workspaces.find((workspace) => workspace.teamId === conversation?.teamId)}
          elsewhere={driver ? elsewhere : []}
          go={go}
          timeZone={timeZone}
          docked={docked}
          leaving={!profile.open}
          still={profile.still}
          close={profile.close}
          gone={profile.gone}
        />
      )}
      <ErrorBox message={error} />
      {opened && conversation ? (
        <Thread
          conversation={conversation}
          messages={opened.messages}
          more={opened.more}
          readOlder={readOlder}
          writing={writing}
          timeZone={timeZone}
          now={now}
          profile={opens ?? {}}
          openGroup={openGroup}
          retry={(failed) => void retry.run(failed)}
          workdays={driver ? opened.workdays : undefined}
        />
      ) : (
        <div className="thread">{!error && <Loading />}</div>
      )}
      {conversation &&
        person &&
        (unreachable ? (
          <NoWorkPhone elsewhere={elsewhere} go={go} />
        ) : (
          <Composer
            placeholder={`Message ${whom}`}
            writable={writable}
            texted={texted}
            to={texted && about && number ? lastFour(number) : undefined}
            send={async (text, photos) => sent(await sendMessage(id, text, photos))}
            typing={() => void sayTyping(id).catch(() => undefined)}
          />
        ))}
    </section>
  );
}

/** A driver's route today and the work phone they hold now, as their conversation's head says. */
function RouteToday({ today }: { today: Today }) {
  return (
    <>
      {today.route && <RouteCode code={today.route} />}
      {today.van && <span>Van {today.van}</span>}
      {today.phone && <WorkPhone number={today.phone} />}
    </>
  );
}

/** A conversation as one to go to. */
const asElsewhere = (conversation: Conversation): Elsewhere => ({
  kind: conversation.kind,
  conversationId: conversation.id,
  waiting: conversation.state === 'waiting',
  number: conversation.number,
  driverId: conversation.driverId,
  teamId: conversation.teamId,
  userId: conversation.with.userId,
});

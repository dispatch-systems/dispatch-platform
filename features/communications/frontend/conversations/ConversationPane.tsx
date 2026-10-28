import { useEffect, type ReactNode } from 'react';
import type {
  CommunicationsWorkspace,
  Conversation,
  Elsewhere,
  Message,
  SentMessage,
  Writing,
} from '../../api/index.js';
import { markDone, retryText, sayTyping, sendMessage } from '../../api/client.js';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { Changed } from '../inbox/useInbox.js';
import { lastFour } from '../shared/format.js';
import { PaneHead } from '../shared/PaneHead.js';
import { ThreadPending } from '../shared/ThreadFrame.js';
import { usePanel } from '../shared/usePanel.js';
import { NoWorkPhone } from '../todays-routes/NoWorkPhone.js';
import { driverToday, type TodaysRead } from '../todays-routes/todays-routes.js';
import { Composer } from './Composer.js';
import { ElsewhereChips } from './Elsewhere.js';
import { elsewhereNow, targetOf } from './elsewhere.js';
import { KINDS } from './kinds.js';
import { PARTIES, type Open } from './parties.js';
import { PartyFace, partyOf } from './party.js';
import { Profile } from './Profile.js';
import type { Target } from './target.js';
import { Thread } from './Thread.js';
import { useConversation } from './useConversation.js';

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
  todays,
  open,
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
  /** Today's Routes as the page last read it, while it shows: today's drivers, whom a work phone
   * nobody held can be picked for, and their routes today. */
  todays?: TodaysRead;
  /** Opens another conversation, as whom it's with's of another kind, or starts it. */
  open: (target: Target) => void;
}) {
  const { opened, error, gone, readOlder, ...read } = useConversation(id, changed);
  useEffect(() => {
    if (gone) closed();
  }, [gone, closed]);
  useEffect(() => {
    if (opened) seen(opened.conversation);
  }, [opened, seen]);
  const conversation = listed ?? opened?.conversation;
  const party = conversation && PARTIES[partyOf(conversation)];
  const elsewhere = elsewhereNow(opened?.elsewhere ?? [], conversations);
  const working = workspaces.filter((workspace) => workspace.on);
  const follow = (theirs: Conversation) => {
    show(theirs);
    open({ kind: 'conversation', id: theirs.id });
  };
  const go = (to: Elsewhere) => {
    const target = targetOf(to);
    if (target) open(target);
  };
  const shown: Open | undefined = conversation && {
    conversation,
    today: conversation.driverId
      ? driverToday(
          conversation.driverId,
          opened && { value: opened.today, at: opened.readAt },
          todays,
        )
      : null,
    // With several workspaces to tell apart, the one it's in.
    where:
      working.length > 1
        ? workspaces.find((workspace) => workspace.teamId === conversation.teamId)?.name
        : undefined,
    working,
    texting,
    drivers: todays?.value.drivers ?? [],
    timeZone,
    show,
    follow,
  };
  const number = (shown && party?.number(shown)) ?? '';
  // One that can't reach them now, as a driver holding no work phone, once it's read.
  const unreachable = shown && opened !== undefined && party?.reachable?.(shown) === false;
  const done = useAction(async () => {
    const after = await markDone(id);
    read.show(after);
    show(after);
  });
  const sent = ({ conversation: theirs, message }: SentMessage) => {
    if (!conversation) return;
    // Filed in another conversation, as its number changed hands as it went: follow it there.
    if (theirs.id !== id) return follow(theirs);
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
  const about: ReactNode = shown && party?.about(shown);
  // The profile, and what opened it, which focus goes back to as it closes: the photo by the
  // messages, or who it's with when that photo is gone.
  const profiled = party?.profile || undefined;
  const profile = usePanel(kept && Boolean(profiled));
  const opens = profiled ? profile.opens : undefined;
  return (
    <section
      className={profile.open ? 'pane-thread profiled' : 'pane-thread'}
      aria-label={person?.name ?? 'Conversation'}
    >
      <PaneHead
        face={conversation && <PartyFace conversation={conversation} size="head" />}
        name={person?.name}
        about={about}
        waits={conversation?.state === 'waiting'}
        waitingSince={conversation?.waitingSince ?? null}
        now={now}
        who={profile.who}
        open={profile.open}
        opens={opens}
        back={back}
        done={{ busy: done.busy, run: () => void done.run() }}
        actions={shown && party?.actions?.(shown)}
      />
      <ElsewhereChips elsewhere={elsewhere} current={id} go={go} />
      {profile.drawn && person && profiled && (
        <Profile
          id={profile.id}
          conversation={id}
          person={person}
          workspace={workspaces.find((workspace) => workspace.teamId === conversation?.teamId)}
          elsewhere={profiled.ways ? elsewhere : []}
          today={profiled.today ? (shown?.today ?? null) : undefined}
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
          profile={profiled?.fromPhoto ? opens : undefined}
          openGroup={openGroup}
          retry={(failed) => void retry.run(failed)}
          workdays={party?.workdays ? opened.workdays : undefined}
        />
      ) : (
        <ThreadPending failed={Boolean(error)} />
      )}
      {shown && party && (
        <Composer
          placeholder={`Message ${party.whom(shown)}`}
          writable={party.writable(shown) && !unreachable}
          instead={unreachable && <NoWorkPhone elsewhere={elsewhere} go={go} />}
          limits={opened?.limits ?? undefined}
          attachments={KINDS[shown.conversation.kind].attachments}
          to={about && number ? lastFour(number) : undefined}
          send={async (text, photos) => sent(await sendMessage(id, text, photos))}
          typing={() => void sayTyping(id).catch(() => undefined)}
        />
      )}
    </section>
  );
}

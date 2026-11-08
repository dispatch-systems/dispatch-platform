import { useEffect, useMemo, useState, type ReactNode } from 'react';
import type {
  CommunicationsWorkspace,
  Conversation,
  Elsewhere,
  Message,
  SentMessage,
  Writing,
} from '../../api/index.js';
import { getConversation, markDone, retryText, sayTyping, sendMessage } from '../../api/client.js';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { lastFour } from '../shared/format.js';
import { PaneHead } from '../shared/PaneHead.js';
import { ThreadPending } from '../shared/ThreadFrame.js';
import { usePanel } from '../shared/usePanel.js';
import { NoWorkPhone } from '../todays-routes/NoWorkPhone.js';
import { dayOf, type TodaysRead } from '../todays-routes/todays-routes.js';
import { Composer } from './Composer.js';
import { ElsewhereChips } from './ElsewhereChips.js';
import { elsewhereNow, targetOf } from './elsewhere.js';
import { KINDS } from './kinds.js';
import { PARTIES, type Open } from './parties.js';
import { PartyFace, partyOf } from './party.js';
import { Profile } from './Profile.js';
import type { Target } from './target.js';
import { Thread } from './Thread.js';
import { TypingLine } from './TypingLine.js';
import { useReplied } from './replied.js';
import type { ConversationReads } from './reads.js';
import type { Drafts } from './drafts.js';
import { useConversation } from './useConversation.js';

// The conversation open: who it's with, the workspace it's in while there are several, the number
// it texts, or a driver's route today and the work phone they hold, and, while it waits, how long
// and Done, else Next waiting; what can be done with its number, or with a work phone nobody held,
// Pick driver; their conversations of other kinds, as chips; its messages; and writing in it, by
// text through the DSP's phones, or for a driver holding no work phone, their other conversations
// instead; above the box, who else at the DSP is writing in it, and before sending, whether one of
// them replied since the member started writing. On a phone it takes the page, with a way back to
// the list. While whom it's with is working, who it's with, or their photo by their messages,
// opens their profile over it, or beside it on a wide page (`docked`).

export function ConversationPane({
  id,
  listed,
  conversations,
  workspaces,
  texting,
  docked,
  kept,
  drafts,
  seen,
  reads,
  typing,
  me,
  timeZone,
  now,
  show,
  back,
  closed,
  openGroup,
  todays,
  open,
  next,
  gifs,
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
  /** What's written in each conversation, kept by the page. */
  drafts: Drafts;
  /** Tells the page of the conversation as read, which pins one the list doesn't hold. */
  seen: (conversation: Conversation) => void;
  /** The conversations the page has read, which it shows from as it opens. */
  reads: ConversationReads;
  typing: readonly Writing[];
  /** The member's own account, whose messages are no teammate's reply. */
  me: string;
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
  /** Next waiting: opens the one waiting longest of the section shown, while another waits. */
  next?: () => void;
  /** Whether a GIF of KLIPY's may be chosen for a message, as the DSP has GIFs. */
  gifs: boolean;
}) {
  const {
    opened,
    error,
    gone,
    readOlder,
    olderFailed,
    retry: readAgain,
    ...read
  } = useConversation(reads, id);
  useEffect(() => {
    if (gone) closed();
  }, [gone, closed]);
  useEffect(() => {
    if (opened) seen(opened.conversation);
  }, [opened, seen]);
  const conversation = listed ?? opened?.conversation;
  const party = conversation && PARTIES[partyOf(conversation)];
  // Those shown: a work one of a driver off today is read only to follow its updates (`reads.ts`).
  const elsewhere = elsewhereNow(opened?.elsewhere ?? [], conversations).filter(
    (each) => !each.hidden,
  );
  const working = workspaces.filter((workspace) => workspace.on);
  const follow = (theirs: Conversation) => {
    show(theirs);
    open({ kind: 'conversation', id: theirs.id });
  };
  const go = (to: Elsewhere) => {
    const target = targetOf(to);
    if (target) open(target);
  };
  // Whom it's with today, as the conversation or Today's Routes read it last.
  const day =
    conversation && dayOf(conversation, opened && { value: opened, at: opened.readAt }, todays);
  const shown: Open | undefined = conversation && {
    conversation,
    today: day?.today ?? null,
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
  // Done shows it done at once, and as the server had it once that's saved. One that fails reads
  // the conversation again and shows it as the server has it, whatever came since.
  const marking = useAction(async (seen: Message | null | undefined) => {
    const after = await markDone(id, seen);
    read.show(after);
    show(after);
  });
  const done = () => {
    if (!conversation) return;
    const marked = { ...conversation, state: 'done' as const, waitingSince: null };
    read.show(marked);
    show(marked);
    void marking.run(opened?.messages.at(-1) ?? conversation.last).then(async (saved) => {
      if (saved) return;
      const now = await getConversation(id).catch(() => undefined);
      if (!now) return;
      read.show(now.conversation);
      show(now.conversation);
    });
  };
  const sent = ({ conversation: theirs, message }: SentMessage) => {
    // Filed in another conversation, as its number changed hands as it went: follow it there.
    if (theirs.id !== id) return follow(theirs);
    // As the server had it once sent; the next update says so for everyone.
    read.show(theirs, message);
    show(theirs);
  };
  const retry = useAction(async (failed: Message) => {
    if (conversation) read.show(conversation, await retryText(id, failed.id));
  });
  const writing = typing.filter((writer) => writer.conversationId === id);
  // Its latest read's messages, and its last as the latest update brings it, which may be newer:
  // never older ones read since, which are no teammate's reply.
  const last = conversation?.last;
  const known = useMemo(
    () => opened && (last ? [...opened.lastRead, last] : opened.lastRead),
    [opened, last],
  );
  const replied = useReplied(known, me);
  // Asked to show a teammate's reply, the thread goes to its latest.
  const [latest, setLatest] = useState(0);
  const person = conversation?.with;
  const about: ReactNode = shown && party?.about(shown);
  // Their profile, while they're working, and what opened it, which focus goes back to as it
  // closes: the photo by the messages, or who it's with when that photo is gone. Once they're not,
  // it's gone, as is its room.
  const profiled = Boolean(day?.working);
  const profile = usePanel(kept && profiled);
  const profileOpen = profiled && profile.open;
  const opens = profiled ? profile.opens : undefined;
  return (
    <section
      className={profileOpen ? 'pane-thread profiled' : 'pane-thread'}
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
        open={profileOpen}
        opens={opens}
        back={back}
        done={done}
        next={next}
        actions={shown && party?.actions?.(shown)}
      />
      <ElsewhereChips elsewhere={elsewhere} current={id} go={go} />
      {profile.drawn && person && profiled && (
        <Profile
          id={profile.id}
          conversation={id}
          person={person}
          workspace={workspaces.find((workspace) => workspace.teamId === conversation?.teamId)}
          elsewhere={elsewhere}
          today={shown?.today ?? null}
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
          olderFailed={olderFailed}
          latest={latest}
          timeZone={timeZone}
          now={now}
          profile={opens}
          openGroup={openGroup}
          retry={(failed) => void retry.run(failed)}
          workdays={party?.workdays ? opened.workdays : undefined}
        />
      ) : (
        <ThreadPending failed={Boolean(error)} retry={readAgain} />
      )}
      {shown && party && (
        <Composer
          drafts={drafts}
          draft={id}
          placeholder={`Message ${party.whom(shown)}`}
          writable={party.writable(shown) && !unreachable}
          instead={unreachable && <NoWorkPhone elsewhere={elsewhere} go={go} />}
          limits={opened?.limits}
          attachments={KINDS[shown.conversation.kind].attachments}
          to={about && number ? lastFour(number) : undefined}
          send={async (text, photos, gif) => sent(await sendMessage(id, text, photos, gif))}
          gifs={gifs}
          typing={() => void sayTyping(id).catch(() => undefined)}
          above={<TypingLine writing={writing} />}
          replied={{
            check: replied.check,
            seen: replied.seen,
            show: () => setLatest((shown) => shown + 1),
          }}
        />
      )}
    </section>
  );
}

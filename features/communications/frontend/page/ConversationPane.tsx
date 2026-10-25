import { useEffect } from 'react';
import type { CommunicationsWorkspace, Conversation, Message, Writing } from '../../api/index.js';
import { markDone, sayTyping, sendMessage } from '../../api/client.js';
import { ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Composer } from './Composer.js';
import { ConversationHead } from './ConversationHead.js';
import { roleText } from './parts.js';
import { Profile } from './Profile.js';
import { Thread } from './Thread.js';
import { useConversation } from './useConversation.js';
import { usePanel } from './usePanel.js';
import type { Changed } from './useInbox.js';

// The conversation open: who it's with, the workspace it's in while there are several, and,
// while it waits, how long and Done; its messages; and writing in it. On a phone it takes the
// page, with a way back to the list. Who it's with, or their photo by their messages, opens
// their profile over it, or beside it on a wide page (`docked`).

export function ConversationPane({
  id,
  listed,
  workspaces,
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
}: {
  id: string;
  /** The conversation as the list has it, which the page's own writes and each update keep. */
  listed?: Conversation;
  /** The DSP's workspaces, as each update brings them. */
  workspaces: readonly CommunicationsWorkspace[];
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
}) {
  const { opened, error, gone, readOlder, ...read } = useConversation(id, changed);
  useEffect(() => {
    if (gone) closed();
  }, [gone, closed]);
  useEffect(() => {
    if (opened) seen(opened.conversation);
  }, [opened, seen]);
  const conversation = listed ?? opened?.conversation;
  const working = workspaces.filter((workspace) => workspace.on);
  const writable = working.some((workspace) => workspace.teamId === conversation?.teamId);
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
  const sent = (message: Message) => {
    if (!conversation) return;
    // Answered, it waits no more; the next update says so for everyone.
    const after: Conversation = {
      ...conversation,
      state: 'responded',
      waitingSince: null,
      lastAt: message.at,
      last: message,
    };
    read.show(after, message);
    show(after);
  };
  const writing = typing.filter((writer) => writer.conversationId === id);
  const person = conversation?.with;
  // What they do for the DSP, and where.
  const about = [person && roleText(person), where].filter(Boolean).join(' · ');
  // The profile, and what opened it, which focus goes back to as it closes: the photo by the
  // messages, or who it's with when that photo is gone.
  const profile = usePanel(kept);
  const opens = profile.opens;
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
      />
      {profile.drawn && person && (
        <Profile
          id={profile.id}
          conversation={id}
          person={person}
          workspace={workspaces.find((workspace) => workspace.teamId === conversation?.teamId)}
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
          profile={opens}
          openGroup={openGroup}
        />
      ) : (
        <div className="thread">{!error && <Loading />}</div>
      )}
      {conversation && person && (
        <Composer
          placeholder={`Message ${person.name.split(' ')[0] ?? person.name}`}
          writable={writable}
          send={async (text, photos) => sent(await sendMessage(id, text, photos))}
          typing={() => void sayTyping(id).catch(() => undefined)}
        />
      )}
    </section>
  );
}

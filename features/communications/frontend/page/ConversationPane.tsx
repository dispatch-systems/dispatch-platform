import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { ArrowLeft, Check } from 'lucide-react';
import type { CommunicationsWorkspace, Conversation, Message, Writing } from '../../api/index.js';
import { markDone } from '../../api/client.js';
import { ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Composer } from './Composer.js';
import { Face } from './Face.js';
import { opensProfile, roles, Wait } from './parts.js';
import { Profile } from './Profile.js';
import { Thread } from './Thread.js';
import { useConversation } from './useConversation.js';
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
  seen,
  changed,
  typing,
  timeZone,
  now,
  show,
  back,
  closed,
}: {
  id: string;
  /** The conversation as the list has it, which the page's own writes and each update keep. */
  listed?: Conversation;
  /** The DSP's workspaces, as each update brings them. */
  workspaces: readonly CommunicationsWorkspace[];
  /** Whether the profile, open, is a column of its own beside the conversation. */
  docked: boolean;
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
  const about = [person?.role && roles[person.role], where].filter(Boolean).join(' · ');
  // The profile, and what opened it, which focus goes back to as it closes: the photo by the
  // messages, or who it's with when that photo is gone.
  const [profile, setProfile] = useState(false);
  const profileId = useId();
  const opener = useRef<HTMLElement | null>(null);
  const who = useRef<HTMLButtonElement>(null);
  const toggleProfile = useCallback((from: HTMLElement) => {
    opener.current = from;
    setProfile((open) => !open);
  }, []);
  const closeProfile = useCallback((refocus: boolean) => {
    setProfile(false);
    if (refocus) (opener.current?.isConnected ? opener.current : who.current)?.focus();
  }, []);
  const opens = opensProfile(profileId, profile, toggleProfile);
  return (
    <section
      className={profile ? 'pane-thread profiled' : 'pane-thread'}
      aria-label={person?.name ?? 'Conversation'}
    >
      <header className="thread-head">
        <button className="icon-button back" aria-label="Back" onClick={back}>
          <ArrowLeft size={18} />
        </button>
        {person && conversation && (
          <>
            <button ref={who} className={profile ? 'who open' : 'who'} {...opens}>
              <Face {...person} size="head" />
              <span>
                <strong>{person.name}</strong>
                {(about || conversation.waitingSince !== null) && (
                  <small>
                    {about}
                    {about && conversation.waitingSince !== null && ' · '}
                    {conversation.waitingSince !== null && (
                      <Wait since={conversation.waitingSince} now={now} />
                    )}
                  </small>
                )}
              </span>
            </button>
            {conversation.state === 'waiting' && (
              <div className="actions">
                <button disabled={done.busy} onClick={() => void done.run()}>
                  <Check size={15} />
                  Done
                </button>
              </div>
            )}
          </>
        )}
      </header>
      {profile && person && (
        <Profile
          id={profileId}
          conversation={id}
          person={person}
          workspace={workspaces.find((workspace) => workspace.teamId === conversation?.teamId)}
          timeZone={timeZone}
          docked={docked}
          close={closeProfile}
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
        />
      ) : (
        <div className="thread">{!error && <Loading />}</div>
      )}
      {conversation && <Composer conversation={conversation} writable={writable} sent={sent} />}
    </section>
  );
}

import { useCallback, useEffect, useState } from 'react';
import { SquarePen } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import type { Contact, Conversation } from '../../api/index.js';
import { markDone, openConversation } from '../../api/client.js';
import { finishSlack } from '../../../slack/api/client.js';
import { useSlackReturn } from '../../../slack/frontend/index.js';
import { ErrorBox, Header, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { hashQuery, replaceHashQuery } from '../../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ConversationList } from './ConversationList.js';
import { ConversationPane } from './ConversationPane.js';
import { CopyChatLink } from './CopyChatLink.js';
import { NewMessage } from './NewMessage.js';
import { ConnectSlack, UpdateSlack } from './Setup.js';
import { useInbox } from './useInbox.js';

// The Communications page: the DSP's conversations with the members of its Slack workspaces,
// those waiting on an answer first, and the one open beside them, all kept up to date as they
// change. Those who manage the DSP's connections connect Slack here while it isn't, and update
// it while a workspace hasn't approved what Communications uses. The conversation open is kept
// in the address (`?open=`).

/** How often the times waited move on. */
const TICK_MS = 30_000;
/** Wide enough to show the list and a conversation side by side, as communications.css says. */
const SIDE_BY_SIDE = '(width > 760px)';

/** The time now, moving on every `every` milliseconds. */
function useNow(every: number) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), every);
    return () => clearInterval(timer);
  }, [every]);
  return now;
}

export function CommunicationsPage({ view }: { view: DspView }) {
  const timeZone = view.dsp.timezone;
  const manages = can(view, 'connections.manage');
  const { inbox, changed, error, refresh, show, readOlder } = useInbox();
  // Slack sends those who connected or updated it from here back here, to finish.
  const returned = useSlackReturn(async (state, code) => {
    try {
      await finishSlack(state, code);
    } finally {
      refresh();
    }
  });
  const [open, setOpen] = useState(() => hashQuery().get('open') ?? undefined);
  const [pinned, setPinned] = useState<Conversation>();
  const [composing, setComposing] = useState(false);
  const now = useNow(TICK_MS);
  const select = useCallback((id?: string) => {
    setOpen(id);
    // One pinned to the top of the list goes once another is open.
    setPinned((kept) => (kept?.id === id ? kept : undefined));
    replaceHashQuery(id ? { open: id } : {});
  }, []);
  const closed = useCallback(() => select(undefined), [select]);
  // Side by side, a conversation is always open: the first, until another is chosen, and as the
  // window widens to show both.
  const first = inbox?.conversations[0]?.id;
  useEffect(() => {
    const wide = matchMedia(SIDE_BY_SIDE);
    const openFirst = () => {
      if (!open && first && wide.matches) select(first);
    };
    openFirst();
    wide.addEventListener('change', openFirst);
    return () => wide.removeEventListener('change', openFirst);
  }, [open, first, select]);
  // Each update brings the workspaces as they are, so it goes once Slack is updated.
  const updating = manages ? (inbox?.workspaces.filter((each) => each.needsUpdate) ?? []) : [];
  const done = useAction(async (conversation: Conversation) =>
    show(await markDone(conversation.id)),
  );
  // The conversation open, as last read, is pinned to the top of the list while the list doesn't
  // hold it: one holding no message yet, as New message opens it, or one older than those listed.
  const seen = useCallback((conversation: Conversation) => setPinned(conversation), []);
  const pick = useCallback(
    async (contact: Contact) => {
      let id = contact.conversationId;
      if (!id) {
        const opened = await openConversation(contact.teamId, contact.person.userId);
        id = opened.id;
        setPinned(opened);
      }
      select(id);
      setComposing(false);
    },
    [select],
  );

  const working = inbox?.workspaces.filter((workspace) => workspace.on) ?? [];
  const connect = manages && inbox?.workspaces.length === 0;
  const listed = inbox?.conversations.find((conversation) => conversation.id === open);
  const unlisted = pinned && !inbox?.conversations.some((each) => each.id === pinned.id);
  return (
    <div className={open ? 'communications reading' : 'communications'}>
      <div className="communications-page" inert={updating.length > 0}>
        <Header title="Communications">
          {working.length > 0 && (
            <>
              <CopyChatLink workspaces={working} />
              <button
                className="primary"
                aria-label="New message"
                onClick={() => setComposing(true)}
              >
                <SquarePen size={16} />
                <span>New message</span>
              </button>
            </>
          )}
        </Header>
        <ErrorBox message={error} />
        {!inbox ? (
          !error && <Loading />
        ) : connect ? (
          <ConnectSlack busy={returned.busy} />
        ) : (
          <div className="chat-frame">
            <ConversationList
              inbox={inbox}
              pinned={unlisted ? pinned : undefined}
              readOlder={readOlder}
              open={open}
              typing={inbox.typing.writing}
              timeZone={timeZone}
              now={now}
              select={select}
              done={(conversation) => void done.run(conversation)}
            />
            {open ? (
              <ConversationPane
                key={open}
                id={open}
                listed={listed ?? (pinned?.id === open ? pinned : undefined)}
                working={working}
                seen={seen}
                changed={changed}
                typing={inbox.typing.writing}
                timeZone={timeZone}
                now={now}
                show={show}
                back={closed}
                closed={closed}
              />
            ) : (
              <section className="pane-thread" aria-label="Conversation" />
            )}
          </div>
        )}
      </div>
      {updating.length > 0 && <UpdateSlack workspaces={updating} busy={returned.busy} />}
      {composing && (
        <NewMessage workspaces={working} pick={pick} close={() => setComposing(false)} />
      )}
    </div>
  );
}

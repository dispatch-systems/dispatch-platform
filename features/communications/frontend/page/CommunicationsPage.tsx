import { useCallback, useEffect, useRef, useState } from 'react';
import { SlidersHorizontal, SquarePen } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import type { Contact, Conversation, Group } from '../../api/index.js';
import { markDone, markGroupDone, openConversation } from '../../api/client.js';
import { finishSlack } from '../../../slack/api/client.js';
import { useSlackReturn } from '../../../slack/frontend/index.js';
import { ErrorBox, Header, Loading } from '../../../../core/shell/frontend/ui/index.js';
import {
  hashQuery,
  navigate,
  replaceHashQuery,
  settingsHash,
} from '../../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import { prefetchRouteData } from '../../../../core/shell/frontend/runtime/route-prefetch.js';
import { settingsPage } from '../../../../core/shell/frontend/runtime/slots.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { pageLook, Shown, useDisplay } from '../display/display.js';
import { rows, type Row } from '../inbox.js';
import { ConversationPane } from './ConversationPane.js';
import { CopyChatLink } from './CopyChatLink.js';
import { GroupDraft, type Draft } from './GroupDraft.js';
import { GroupPane } from './GroupPane.js';
import { ListPane } from './ListPane.js';
import { NewMessage } from './NewMessage.js';
import { ConnectSlack, UpdateSlack } from './Setup.js';
import { useInbox } from './useInbox.js';
import { useLayout } from './useLayout.js';
import { useWorkspaceChoice } from './useWorkspaceChoice.js';

// The Communications page: the DSP's conversations with the members of its Slack workspaces, and
// the groups it messaged, those waiting on an answer first, or everyone it can message, of every
// workspace or one; and the conversation or group open beside them, or a message to a group being
// written, all kept up to date as they change. Those who manage the DSP's connections connect
// Slack here while it isn't, and update it while a workspace hasn't approved what Communications
// uses. The conversation or group open is kept in the address (`?open=`, `?group=`). It looks as
// the member chose in Settings' Message display, which Display opens.

/** How often the times waited move on. */
const TICK_MS = 30_000;

/** The time now, moving on every `every` milliseconds. */
function useNow(every: number) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), every);
    return () => clearInterval(timer);
  }, [every]);
  return now;
}

/** What the pane shows: a conversation or a group, by its ID, or a message being written to a
 * group. */
type Opened = { conversation?: string; group?: string; drafting?: true };
/** What the address says is open. */
function addressed(): Opened {
  const query = hashQuery();
  return {
    conversation: query.get('open') ?? undefined,
    group: query.get('group') ?? undefined,
  };
}

export function CommunicationsPage({ view, user }: { view: DspView; user: string }) {
  const timeZone = view.dsp.timezone;
  const page = useRef<HTMLDivElement>(null);
  const layout = useLayout(page);
  const display = useDisplay();
  // The profile, kept open or shut from one conversation to the next, beside them, as the member
  // may choose: on a phone it would cover each one opened.
  const [profileOpen, keepProfile] = useState(true);
  const kept =
    display.keep_profile_open && layout !== 'phone'
      ? { open: profileOpen, keep: keepProfile }
      : undefined;
  // Beside the conversation with room for it, unless the member has it over the conversation.
  const docked = layout === 'docked' && display.profile_opens === 'beside';
  const displaySettings = settingsHash(view.dsp.id, 'display', view);
  const warmSettings = () => {
    const host = settingsPage();
    if (host) prefetchRouteData(host, view);
  };
  const manages = can(view, 'connections.manage');
  const [chosen, choose] = useWorkspaceChoice(view.dsp.id, user);
  const { inbox, team, changed, error, refresh, show, showGroup, readOlder } = useInbox(chosen);
  // Slack sends those who connected or updated it from here back here, to finish.
  const returned = useSlackReturn(async (state, code) => {
    try {
      await finishSlack(state, code);
    } finally {
      refresh();
    }
  });
  const [opened, setOpened] = useState<Opened>(addressed);
  const { conversation: open, group, drafting } = opened;
  const [pinned, setPinned] = useState<Row>();
  const [composing, setComposing] = useState(false);
  const [draft, setDraft] = useState<Draft>({ name: '', people: 0 });
  const now = useNow(TICK_MS);
  const opening = useCallback((next: Opened) => {
    setOpened(next);
    // One pinned to the top of the list goes once another is open.
    setPinned((kept) => (kept?.row.id === (next.conversation ?? next.group) ? kept : undefined));
    replaceHashQuery(
      next.conversation ? { open: next.conversation } : next.group ? { group: next.group } : {},
    );
  }, []);
  const select = useCallback((id?: string) => opening({ conversation: id }), [opening]);
  const openGroup = useCallback((id: string) => opening({ group: id }), [opening]);
  const closed = useCallback(() => opening({}), [opening]);
  const working = inbox?.workspaces.filter((workspace) => workspace.on) ?? [];
  const shownRows = inbox ? rows(inbox, team) : [];
  // Side by side, something is always open: the first listed, until another is chosen, and as
  // the page widens to show both.
  const first = shownRows[0];
  useEffect(() => {
    if (open || group || drafting || !first || !layout || layout === 'phone') return;
    if (first.kind === 'group') openGroup(first.row.id);
    else select(first.row.id);
  }, [open, group, drafting, first, layout, select, openGroup]);
  // Each update brings the workspaces as they are, so it goes once Slack is updated.
  const updating = manages ? (inbox?.workspaces.filter((each) => each.needsUpdate) ?? []) : [];
  const done = useAction(async (conversation: Conversation) =>
    show(await markDone(conversation.id)),
  );
  const doneGroup = useAction(async (listed: Group) => showGroup(await markGroupDone(listed.id)));
  // What's open, as last read, is pinned to the top of the list while the list doesn't show it:
  // a conversation holding no message yet, as New message opens it, one listed under a group, or
  // one older than those listed.
  const seen = useCallback(
    (conversation: Conversation) => setPinned({ kind: 'conversation', row: conversation }),
    [],
  );
  const seenGroup = useCallback((listed: Group) => setPinned({ kind: 'group', row: listed }), []);
  const reach = useCallback(
    async (teamId: string, userId: string, conversationId: string | null) => {
      let id = conversationId;
      if (!id) {
        const started = await openConversation(teamId, userId);
        id = started.id;
        setPinned({ kind: 'conversation', row: started });
      }
      select(id);
      setComposing(false);
    },
    [select],
  );
  const pick = useCallback(
    (contact: Contact) => reach(contact.teamId, contact.person.userId, contact.conversationId),
    [reach],
  );
  const reaching = useAction(reach);

  const connect = manages && inbox?.workspaces.length === 0;
  const listed = inbox?.conversations.find((conversation) => conversation.id === open);
  const listedGroup = inbox?.groups.find((each) => each.id === group);
  // What's pinned as the list has it, while it holds it, as each update keeps it.
  const pinnedNow: Row | undefined =
    pinned?.kind === 'conversation'
      ? { kind: 'conversation', row: listed?.id === pinned.row.id ? listed : pinned.row }
      : pinned && {
          kind: 'group',
          row: listedGroup?.id === pinned.row.id ? listedGroup : pinned.row,
        };
  const unlisted =
    pinnedNow && !shownRows.some((each) => each.row.id === pinnedNow.row.id)
      ? pinnedNow
      : undefined;
  const current =
    listed ??
    (pinnedNow?.kind === 'conversation' && pinnedNow.row.id === open ? pinnedNow.row : undefined);
  const currentGroup =
    listedGroup ??
    (pinnedNow?.kind === 'group' && pinnedNow.row.id === group ? pinnedNow.row : undefined);
  // Choosing a workspace leaves a conversation of another open, or a group with nobody there, for
  // the first of the one chosen.
  const chooseWorkspace = (team?: string) => {
    choose(team);
    const elsewhere =
      (current && current.teamId !== team) ||
      (currentGroup && !currentGroup.teamIds.includes(team ?? ''));
    if (team && elsewhere && !drafting) closed();
  };
  const reading = open || group || drafting;
  return (
    <div
      ref={page}
      className={reading ? 'communications reading' : 'communications'}
      {...pageLook(display, layout === 'phone')}
    >
      <Shown value={display}>
        <div className="communications-page" inert={updating.length > 0}>
          <Header title="Communications">
            {working.length > 0 && (
              <>
                {displaySettings && (
                  <button
                    aria-label="Display"
                    onPointerEnter={warmSettings}
                    onFocus={warmSettings}
                    onClick={() => navigate(displaySettings)}
                  >
                    <SlidersHorizontal size={16} />
                    <span>Display</span>
                  </button>
                )}
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
              <ListPane
                inbox={inbox}
                working={working}
                team={team}
                choose={chooseWorkspace}
                draft={drafting ? draft : undefined}
                pinned={unlisted}
                current={current}
                readOlder={readOlder}
                pick={pick}
                open={open ?? group}
                typing={inbox.typing.writing}
                timeZone={timeZone}
                now={now}
                select={select}
                openGroup={openGroup}
                done={(conversation) => void done.run(conversation)}
                doneGroup={(listed) => void doneGroup.run(listed)}
              />
              {drafting ? (
                <GroupDraft
                  workspaces={working}
                  timeZone={timeZone}
                  back={closed}
                  drafted={setDraft}
                  sent={(sent) => {
                    showGroup(sent);
                    openGroup(sent.id);
                  }}
                />
              ) : group ? (
                <GroupPane
                  key={group}
                  id={group}
                  listed={listedGroup}
                  workspaces={inbox.workspaces}
                  docked={docked}
                  seen={seenGroup}
                  changed={changed}
                  timeZone={timeZone}
                  now={now}
                  show={showGroup}
                  back={closed}
                  closed={closed}
                  reply={(entry) => {
                    if (entry.conversationId) select(entry.conversationId);
                  }}
                  open={(person) =>
                    void reaching.run(person.teamId, person.person.userId, person.conversationId)
                  }
                />
              ) : open ? (
                <ConversationPane
                  key={open}
                  id={open}
                  listed={current}
                  workspaces={inbox.workspaces}
                  docked={docked}
                  kept={kept}
                  seen={seen}
                  changed={changed}
                  typing={inbox.typing.writing}
                  timeZone={timeZone}
                  now={now}
                  show={show}
                  back={closed}
                  closed={closed}
                  openGroup={openGroup}
                />
              ) : (
                <section className="pane-thread" aria-label="Conversation" />
              )}
            </div>
          )}
        </div>
        {updating.length > 0 && <UpdateSlack workspaces={updating} busy={returned.busy} />}
        {composing && (
          <NewMessage
            workspaces={working}
            pick={pick}
            group={() => {
              setComposing(false);
              setDraft({ name: '', people: 0 });
              opening({ drafting: true });
            }}
            close={() => setComposing(false)}
          />
        )}
      </Shown>
    </div>
  );
}

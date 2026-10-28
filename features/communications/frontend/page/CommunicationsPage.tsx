import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { SlidersHorizontal, SquarePen } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import type { Conversation, Group } from '../../api/index.js';
import { markDone, markGroupDone } from '../../api/client.js';
import { finishSlack } from '../../../slack/api/client.js';
import { useSlackReturn } from '../../../slack/frontend/index.js';
import { ErrorBox, Header, Loading } from '../../../../core/shell/frontend/ui/index.js';
import {
  hashQuery,
  navigate,
  parseHash,
  replaceHashQuery,
  settingsHash,
} from '../../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import { prefetchRouteData } from '../../../../core/shell/frontend/runtime/route-prefetch.js';
import { settingsPage } from '../../../../core/shell/frontend/runtime/slots.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { dockAt, pageLook, Shown, useDisplay } from '../display/display.js';
import { rows, waitingIn, type Row } from '../inbox/inbox.js';
import {
  groupsSection,
  sectionNamed,
  sectionOf,
  SECTIONS,
  type Section,
} from '../inbox/sections.js';
import { routeGroups, routeRows, rowTarget } from '../todays-routes/todays-routes.js';
import { started, type Target } from '../conversations/target.js';
import { memberTarget } from '../slack/members.js';
import { ConversationPane } from '../conversations/ConversationPane.js';
import { CopyChatLink } from '../slack/CopyChatLink.js';
import { GroupDraft, type Draft } from '../slack/groups/GroupDraft.js';
import { GroupPane } from '../slack/groups/GroupPane.js';
import { ListPane } from './ListPane.js';
import { NewMessage } from '../slack/NewMessage.js';
import { PhonesOffline } from '../texting/PhonesOffline.js';
import { VIEWS, type RoutesShown } from './section-views.js';
import { Sections } from './Sections.js';
import { ConnectSlack, UpdateSlack } from '../slack/Setup.js';
import { useInbox } from '../inbox/useInbox.js';
import { useLayout } from './useLayout.js';
import { useNow } from './useNow.js';
import { useRemembered } from './useRemembered.js';
import { useTodaysRoutes } from '../todays-routes/useTodaysRoutes.js';
import { ViewToggle, type RoutesView } from '../todays-routes/ViewToggle.js';

// The Communications page: the DSP's conversations, in sections: Today's Routes, the drivers on a
// route today and their work phones, while it texts and has live route data; Messaging, the
// numbers that text its phones, while it texts; and Slack, with the members of its Slack
// workspaces, and the groups it messaged, while it has one. In each, those waiting on an answer
// first, or everyone it can message, in Slack of every workspace or one; and the conversation or
// group open beside them, or a message to a group being written, all kept up to date as they
// change; or Today's Routes as a board across the page. A chip in the heading for each of its
// phones offline. Those who manage the DSP's connections connect Slack here while it has neither,
// and, while Slack shows, update it while a workspace hasn't approved what Communications uses.
// The section shown, and Today's Routes' view, are remembered for each member, and the
// conversation or group open is kept in the address (`?open=`, `?group=`): a link to one opens it,
// in its section, the page open or not. It looks as the member chose in Settings' Message display,
// which Display opens.

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
  const display = useDisplay();
  const layout = useLayout(page, dockAt(display));
  // The profile open as each conversation opens, as the member may keep it: not with one pane at a
  // time, where it would cover the conversation.
  const kept = display.keep_profile_open && layout !== 'phone';
  // Beside the conversation with room for it, unless the member has it over the conversation.
  const docked = layout === 'docked' && display.profile_opens === 'beside';
  const displaySettings = settingsHash(view.dsp.id, 'display', view);
  const warmSettings = () => {
    const host = settingsPage();
    if (host) prefetchRouteData(host, view);
  };
  const manages = can(view, 'connections.manage');
  const [chosen, choose] = useRemembered('workspace', view.dsp.id, user);
  const [chosenSection, chooseSection] = useRemembered('section', view.dsp.id, user);
  const [chosenView, chooseView] = useRemembered('routes-view', view.dsp.id, user);
  const { inbox, team, changed, error, refresh, show, showGroup, readOlder } = useInbox(chosen);
  // The sections the DSP has, the one chosen shown, else the first; with none, Slack's, empty
  // until it's connected.
  const texting = Boolean(inbox?.texting);
  const sections: Section[] = SECTIONS.filter((each) => inbox && each.offered(inbox)).map(
    (each) => each.id,
  );
  const section = sections.find((each) => each === chosenSection) ?? sections[0] ?? 'slack';
  const model = sectionNamed(section);
  const shows = VIEWS[section];
  const todays = useTodaysRoutes(section === 'routes', changed, inbox);
  const routeList = useMemo(
    () =>
      todays.read && inbox ? routeGroups(routeRows(todays.read.routes, inbox.conversations)) : [],
    [todays.read, inbox],
  );
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
  const reading = open || group || drafting;
  const [pinned, setPinned] = useState<Row>();
  const [composing, setComposing] = useState(false);
  const [draft, setDraft] = useState<Draft>({ name: '', people: 0 });
  const now = useNow();
  const opening = useCallback((next: Opened) => {
    setOpened(next);
    // One pinned to the top of the list goes once another is open.
    setPinned((kept) => (kept?.row.id === (next.conversation ?? next.group) ? kept : undefined));
    replaceHashQuery(
      next.conversation ? { open: next.conversation } : next.group ? { group: next.group } : {},
    );
  }, []);
  // A link to a conversation or a group, as a new message's notification is, opens it here.
  useEffect(() => {
    const followed = () => {
      const next = addressed();
      if (parseHash(location.hash).page === 'communications' && (next.conversation || next.group))
        opening(next);
    };
    addEventListener('hashchange', followed);
    return () => removeEventListener('hashchange', followed);
  }, [opening]);
  const select = useCallback((id?: string) => opening({ conversation: id }), [opening]);
  const openGroup = useCallback((id: string) => opening({ group: id }), [opening]);
  const closed = useCallback(() => opening({}), [opening]);
  // Choosing the board leaves what's open for it.
  const chooseRoutesView = (next: RoutesView) => {
    chooseView(next);
    if (next === 'board') closed();
  };
  const working = inbox?.workspaces.filter((workspace) => workspace.on) ?? [];
  const listedTeam = model.workspaces ? team : undefined;
  const shownRows = inbox ? rows(inbox, section, listedTeam) : [];
  // Today's Routes as a board across the page, as the member chose, but one pane at a time, and
  // while nothing is open: what opens, from a row, a link or a chip, shows in the list, and the
  // board comes back once nothing is.
  const routesView: RoutesView = chosenView === 'board' ? 'board' : 'list';
  const boardable = layout !== undefined && layout !== 'phone';
  const board = shows.board !== undefined && routesView === 'board' && boardable && !reading;
  // Side by side, something is always open: the first listed with a conversation, until another
  // is chosen, and as the page widens to show both; on the board, nothing.
  const first = shows.first ? shows.first(routeList) : shownRows[0]?.row;
  const firstGroup = !shows.first && shownRows[0]?.kind === 'group';
  useEffect(() => {
    if (open || group || drafting || board || !first || !layout || layout === 'phone') return;
    if (firstGroup) openGroup(first.id);
    else select(first.id);
  }, [open, group, drafting, board, first, firstGroup, layout, select, openGroup]);
  // While Slack shows: each update brings the workspaces as they are, so it goes once Slack is
  // updated.
  const updating =
    manages && model.workspaces ? (inbox?.workspaces.filter((each) => each.needsUpdate) ?? []) : [];
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
  // What a target names, opened: a conversation there is, or one started, pinned until it holds
  // a message.
  const reach = async (target: Target) => {
    if (target.kind === 'conversation') select(target.id);
    else {
      const begun = await started(target);
      setPinned({ kind: 'conversation', row: begun });
      select(begun.id);
    }
    setComposing(false);
  };
  const reaching = useAction(reach);
  const go = (target: Target) => void reaching.run(target);

  const connect = manages && inbox?.workspaces.length === 0 && !inbox.texting;
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
  // Choosing a section leaves what's open in another, for the first of the one chosen.
  const showSection = (next: Section) => {
    chooseSection(next);
    const elsewhere = current ? sectionOf(current) !== next : next !== groupsSection;
    if (elsewhere && (current || currentGroup || drafting)) closed();
  };
  // What opens shows its section, once as it opens, as by a link: a group Slack's, a conversation
  // its own. What was open last is forgotten once nothing is, so it shows its section again.
  const placed = useRef<string>(undefined);
  const shown = currentGroup ?? current;
  const shownIn = currentGroup ? groupsSection : current && sectionOf(current);
  useEffect(() => {
    if (placed.current === shown?.id) return;
    placed.current = shown?.id;
    if (shownIn && shownIn !== section && sections.includes(shownIn)) chooseSection(shownIn);
  }, [shown, shownIn, section, sections, chooseSection]);
  // Today's Routes as the page reads it, which its list and its board show.
  const routes: RoutesShown = {
    groups: routeList,
    read: { routes: todays.read?.routes, error: todays.error, retry: todays.refresh },
    choose: (row) => go(rowTarget(row)),
    driver: current?.driverId,
    view: boardable ? { shown: 'list', choose: chooseRoutesView } : undefined,
  };
  // The DSP's phones offline: in the heading, or on a phone, a line of their own under it.
  const offline = <PhonesOffline phones={inbox?.phones ?? []} now={now} />;
  return (
    <div
      ref={page}
      className={reading ? 'communications reading' : 'communications'}
      {...pageLook(display)}
      data-docked={docked || undefined}
    >
      <Shown value={display}>
        <div className="communications-page" inert={updating.length > 0}>
          <Header title="Communications">
            {layout !== 'phone' && offline}
            {sections.length > 0 && displaySettings && (
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
          {layout === 'phone' && offline}
          <ErrorBox message={error} />
          {!inbox ? (
            !error && <Loading />
          ) : connect ? (
            <ConnectSlack busy={returned.busy} />
          ) : board ? (
            shows.board?.({
              tools: (
                <>
                  {sections.length > 1 && (
                    <Sections
                      sections={sections}
                      section={section}
                      waiting={waitingIn(inbox)}
                      choose={showSection}
                    />
                  )}
                  <ViewToggle view="board" choose={chooseRoutesView} />
                </>
              ),
              routes,
              typing: inbox.typing.writing,
              timeZone,
              now,
            })
          ) : (
            <div className="chat-frame">
              <ListPane
                inbox={inbox}
                routes={routes}
                sections={sections}
                section={section}
                chooseSection={showSection}
                working={working}
                team={listedTeam}
                choose={chooseWorkspace}
                draft={drafting ? draft : undefined}
                pinned={unlisted}
                current={current}
                readOlder={readOlder}
                reach={reach}
                show={show}
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
                  open={(person) => {
                    const target = memberTarget(person);
                    if (target) go(target);
                  }}
                />
              ) : open ? (
                <ConversationPane
                  key={open}
                  id={open}
                  listed={current}
                  conversations={inbox.conversations}
                  workspaces={inbox.workspaces}
                  texting={inbox.texting}
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
                  todays={todays.read}
                  open={go}
                />
              ) : (
                <section className="pane-thread" aria-label="Conversation" />
              )}
            </div>
          )}
        </div>
        {updating.length > 0 && (
          <UpdateSlack
            workspaces={updating}
            busy={returned.busy}
            close={texting ? () => showSection('messaging') : undefined}
          />
        )}
        {composing && (
          <NewMessage
            workspaces={working}
            reach={reach}
            group={() => {
              setComposing(false);
              setDraft({ name: '', people: 0 });
              chooseSection(groupsSection);
              opening({ drafting: true });
            }}
            close={() => setComposing(false)}
          />
        )}
      </Shown>
    </div>
  );
}

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
  pushHashQuery,
  replaceHashQuery,
  settingsHash,
} from '../../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import { cacheLimits } from '../../../../core/shell/frontend/runtime/data-cache.js';
import { prefetchRouteData } from '../../../../core/shell/frontend/runtime/route-prefetch.js';
import { settingsPage } from '../../../../core/shell/frontend/runtime/slots.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { dockAt, pageLook, Shown, useDisplay } from '../display/display.js';
import { rows, waitingIn, type Row } from '../inbox/inbox.js';
import {
  groupsSection,
  lists,
  openingSection,
  sectionNamed,
  sectionOf,
  SECTIONS,
  type Section,
} from '../inbox/sections.js';
import { routeGroups, routeRows, rowTarget } from '../todays-routes/todays-routes.js';
import { started, type Target } from '../conversations/target.js';
import { Drafts } from '../conversations/drafts.js';
import { memberTarget } from '../slack/members.js';
import { ConversationPane } from '../conversations/ConversationPane.js';
import { useConversationReads } from '../conversations/useConversation.js';
import { CopyChatLink } from '../slack/CopyChatLink.js';
import { GroupDraft, type Draft } from '../slack/groups/GroupDraft.js';
import type { Pick } from '../slack/groups/groups.js';
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

// The Communications page: the DSP's conversations, in sections: Recents, every section's, the
// latest written in first, and the numbers that text its phones, always; Today's Routes, the
// drivers on a route today and their work phones, while it has live route data; and Slack, with
// the members of its Slack workspaces, and the groups it messaged, while it has one. In each but
// Recents, those waiting on an answer first, or everyone it can message, in Slack of every
// workspace or one; and the conversation or group open beside them, or a message to a group being
// written, all kept up to date as they change; or Today's Routes as a board across the page. A chip in the heading for
// each of its phones offline. Those who manage the DSP's connections connect Slack here while it
// has neither a workspace nor a phone, and, while Slack shows, update it while a workspace hasn't
// approved what Communications uses.
// The section shown, and Today's Routes' view, are remembered for each member; until they choose
// one, the page opens on the first section with conversations. The conversation or group open is
// kept in the address (`?open=`, `?group=`): a link to one opens it, in its section, the page open
// or not; one pane at a time, one opened from the list is a step of the browser's history, so Back
// goes back to the list. What's written in each and not sent is kept while the page is open, and
// in the browser's tab (`drafts`). It looks as the member chose in Settings' Message display, which
// Display opens.

/** What the pane shows: a conversation or a group, by its ID, or a message being written to a
 * group. */
type Opened = { conversation?: string; group?: string; drafting?: true };
/** How many of the first waiting are read before they're opened. */
const WARMED = 3;
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
  // Of the workspace chosen while a section of workspaces is chosen, as Slack is; else of every one,
  // as Recents lists them.
  const ofWorkspace = SECTIONS.some((each) => each.id === chosenSection && each.workspaces);
  const { inbox, team, changed, error, refresh, show, showGroup, readOlder, olderFailed } =
    useInbox(ofWorkspace ? chosen : undefined);
  // The conversations read, kept so each opens at once once read, and read before they're opened.
  const reads = useConversationReads(view.dsp.id, changed);
  const warm = useCallback((id: string) => reads.warm(id), [reads]);
  // The sections the DSP has, the one chosen shown, else the one the page opened on; with none,
  // Slack's, empty until it's connected.
  const texting = Boolean(inbox?.texting);
  const sections: Section[] = SECTIONS.filter((each) => inbox && each.offered(inbox)).map(
    (each) => each.id,
  );
  // With none chosen, the page opens on the first section with conversations, as the DSP's list
  // first reads, and stays there while it's open, though another gains some.
  const openedOn = useRef<{ dsp: string; section?: Section }>(undefined);
  if (inbox && openedOn.current?.dsp !== view.dsp.id)
    openedOn.current = { dsp: view.dsp.id, section: openingSection(sections, inbox) };
  const section =
    sections.find((each) => each === chosenSection) ??
    sections.find((each) => each === openedOn.current?.section) ??
    sections[0] ??
    'slack';
  const model = sectionNamed(section);
  const shows = VIEWS[section];
  // Read while Today's Routes or Recents, which lists its conversations, shows.
  const todays = useTodaysRoutes(
    sections.includes('routes') && lists(section, 'routes'),
    changed,
    inbox,
  );
  // Grouped again only as Today's Routes or the list answers anew, not as the page draws again.
  const todaysRoutes = todays.read?.value;
  const routeList = useMemo(
    () => (todaysRoutes && inbox ? routeGroups(routeRows(todaysRoutes, inbox.conversations)) : []),
    [todaysRoutes, inbox],
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
  const drafts = useMemo(() => new Drafts(view.dsp.id, user), [view.dsp.id, user]);
  // Whom a message to a group is to, kept as another opens, and what the list's row says of it.
  const [picks, setPicks] = useState<Pick[]>([]);
  const [draft, setDraft] = useState<Draft>({ name: '', people: 0 });
  const now = useNow();
  // How many times what's open changed, as one started opens only if nothing else did meanwhile.
  const turns = useRef(0);
  // Whether what's open is a step of the browser's history after the list, as one pane at a time.
  const stepped = useRef(false);
  const phone = layout === 'phone';
  /** Shows what `next` names. */
  const adopt = useCallback((next: Opened) => {
    turns.current++;
    setOpened(next);
    // One pinned to the top of the list goes once another is open.
    setPinned((kept) => (kept?.row.id === (next.conversation ?? next.group) ? kept : undefined));
  }, []);
  /** Shows what `next` names, and keeps it in the address: one pane at a time, opened from the
   * list, as a step of the browser's history, and closed, by going back that step. */
  const opening = useCallback(
    (next: Opened) => {
      const query: Record<string, string> = next.conversation
        ? { open: next.conversation }
        : next.group
          ? { group: next.group }
          : {};
      const shows = Boolean(next.conversation || next.group);
      if (phone && shows && !reading) {
        stepped.current = true;
        pushHashQuery(query);
      } else if (phone && !shows && !next.drafting && stepped.current) {
        stepped.current = false;
        history.back();
      } else replaceHashQuery(query);
      adopt(next);
    },
    [adopt, phone, reading],
  );
  // The address moved, by Back or Forward, or a link to a conversation or a group, as a new
  // message's notification is: what it names opens here, and one pane at a time, with nothing
  // named, the list.
  useEffect(() => {
    const followed = () => {
      const next = addressed();
      if (parseHash(location.hash).page !== 'communications') return;
      const shows = Boolean(next.conversation || next.group);
      stepped.current = phone && shows;
      if (shows || phone) adopt(next);
    };
    addEventListener('hashchange', followed);
    return () => removeEventListener('hashchange', followed);
  }, [adopt, phone]);
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
  const shownRows = inbox ? rows(inbox, section, listedTeam, shows.read?.(routeList)) : [];
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
  // The first few waiting of those shown, the likeliest opened next, read once the page has read
  // what it opened, and again once an update changes them.
  const waitingFirst = shownRows
    .filter((each) => each.kind === 'conversation' && each.row.state === 'waiting')
    .slice(0, WARMED)
    .map((each) => each.row.id)
    .join(' ');
  useEffect(() => {
    if (!waitingFirst) return;
    const later = setTimeout(() => {
      for (const id of waitingFirst.split(' ')) reads.warm(id);
    }, cacheLimits.preloadDelayMs);
    return () => clearTimeout(later);
  }, [reads, waitingFirst, changed]);
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
  // a message, unless something else was opened while it started.
  const reach = async (target: Target) => {
    if (target.kind === 'conversation') select(target.id);
    else {
      const turn = turns.current;
      const begun = await started(target);
      if (turns.current === turn) {
        setPinned({ kind: 'conversation', row: begun });
        select(begun.id);
      }
    }
    setComposing(false);
  };
  const reaching = useAction(reach);
  const go = (target: Target) => void reaching.run(target);

  const connect = manages && inbox?.workspaces.length === 0 && inbox.phones.length === 0;
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
  // Choosing a section leaves what's open that it doesn't list, for the first of the one chosen.
  const showSection = (next: Section) => {
    chooseSection(next);
    const elsewhere = !lists(next, current ? sectionOf(current) : groupsSection);
    if (elsewhere && (current || currentGroup || drafting)) closed();
  };
  // What opens shows its section, once as it opens, as by a link, unless the one shown lists it
  // too, as Recents does: a group Slack's, a conversation its own. What was open last is forgotten
  // once nothing is, so it shows its section again.
  const placed = useRef<string>(undefined);
  const shown = currentGroup ?? current;
  const shownIn = currentGroup ? groupsSection : current && sectionOf(current);
  useEffect(() => {
    if (placed.current === shown?.id) return;
    placed.current = shown?.id;
    if (shownIn && !lists(section, shownIn) && sections.includes(shownIn)) chooseSection(shownIn);
  }, [shown, shownIn, section, sections, chooseSection]);
  // Today's Routes as the page reads it, which its list and its board show.
  const routes: RoutesShown = {
    groups: routeList,
    read: { routes: todaysRoutes, error: todays.error, retry: todays.refresh },
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
                olderFailed={olderFailed}
                reach={reach}
                show={show}
                open={open ?? group}
                typing={inbox.typing.writing}
                timeZone={timeZone}
                now={now}
                select={select}
                warm={warm}
                openGroup={openGroup}
                done={(conversation) => void done.run(conversation)}
                doneGroup={(listed) => void doneGroup.run(listed)}
              />
              {drafting ? (
                <GroupDraft
                  workspaces={working}
                  timeZone={timeZone}
                  drafts={drafts}
                  picks={picks}
                  pick={setPicks}
                  back={closed}
                  drafted={setDraft}
                  sent={(sent) => {
                    setPicks([]);
                    setDraft({ name: '', people: 0 });
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
                  drafts={drafts}
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
                  drafts={drafts}
                  seen={seen}
                  reads={reads}
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

import { useCallback, useEffect, useMemo, useRef } from 'react';
import { SlidersHorizontal, SquarePen } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import type { Conversation, Group } from '../../api/index.js';
import { markDone, markGroupDone } from '../../api/client.js';
import { finishSlack } from '../../../slack/api/client.js';
import { useSlackReturn } from '../../../slack/frontend/index.js';
import { ErrorBox, Header, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { navigate, settingsHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import { hasFeature } from '../../../../core/shell/frontend/runtime/features.js';
import { cacheLimits } from '../../../../core/shell/frontend/runtime/data-cache.js';
import { prefetchRouteData } from '../../../../core/shell/frontend/runtime/route-prefetch.js';
import { settingsPage } from '../../../../core/shell/frontend/runtime/slots.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { dockAt, pageLook, Shown, useDisplay } from '../display/display.js';
import { nextWaiting, rows, waitingIn } from '../inbox/inbox.js';
import { lists, openingSection, sectionNamed, SECTIONS, type Section } from '../inbox/sections.js';
import { routeGroups, routeRows, rowTarget } from '../todays-routes/todays-routes.js';
import { draftsOf } from '../conversations/drafts.js';
import { memberTarget } from '../slack/members.js';
import { ConversationPane } from '../conversations/ConversationPane.js';
import { useConversationReads } from '../conversations/useConversation.js';
import { CopyChatLink } from '../slack/CopyChatLink.js';
import { GroupDraft } from '../slack/groups/GroupDraft.js';
import { GroupPane } from '../slack/groups/GroupPane.js';
import { ListPane } from './ListPane.js';
import { NewMessage } from './NewMessage.js';
import { PhonesOffline } from '../texting/PhonesOffline.js';
import { VIEWS, type RoutesShown } from './section-views.js';
import { Sections } from './Sections.js';
import { ConnectSlack, UpdateSlack } from '../slack/Setup.js';
import { beforeToday } from '../inbox/headings.js';
import { useInbox } from '../inbox/useInbox.js';
import { dayOf } from '../shared/format.js';
import { useLayout } from './useLayout.js';
import { useOpenedPane } from './useOpenedPane.js';
import { useNow } from './useNow.js';
import { useRemembered } from './useRemembered.js';
import { useShortcuts } from './useShortcuts.js';
import { useTodaysRoutes } from '../todays-routes/useTodaysRoutes.js';
import { ViewToggle, type RoutesView } from '../todays-routes/ViewToggle.js';

// Composes inbox and conversation reads, page-lived text drafts, and the selected pane.

/** How many of the first waiting are read before they're opened. */
const WARMED = 3;

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
  // The day Recents' Previous days were shown on, as they stay shown until it ends.
  const [previousOn, showPreviousOn] = useRemembered('previous-days', view.dsp.id, user);
  // Of the workspace chosen while a section of workspaces is chosen, as Slack is; else of every one,
  // as Recents lists them.
  const ofWorkspace = SECTIONS.some((each) => each.id === chosenSection && each.workspaces);
  const { inbox, team, changed, error, refresh, show, showGroup, readOlder, olderFailed } =
    useInbox(ofWorkspace ? chosen : undefined);
  // The conversations read, kept so each opens at once once read, and read before they're opened.
  const reads = useConversationReads(view.dsp.id, changed, timeZone);
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
  const {
    pane,
    open,
    group,
    drafting,
    reading,
    pinnedNow,
    composing,
    setComposing,
    picks,
    setPicks,
    draft,
    setDraft,
    listedGroup,
    current,
    select,
    openGroup,
    closed,
    seen,
    seenGroup,
    reach,
    go,
    chooseWorkspace,
    showSection,
    startGroup,
    sentGroup,
  } = useOpenedPane({
    phone: layout === 'phone',
    inbox,
    section,
    sections,
    choose,
    chooseSection,
    showGroup,
  });
  const drafts = useMemo(() => draftsOf(view.dsp.id, user), [view.dsp.id, user]);
  const now = useNow();
  const today = dayOf(now, timeZone);
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
  // is chosen, and as the page widens to show both, but none Previous days hides; on the board,
  // nothing.
  const firstShown =
    model.everything && previousOn !== today
      ? shownRows.find((each) => !beforeToday(each.row, { timeZone, now }))
      : shownRows[0];
  const first = shows.first ? shows.first(routeList) : firstShown?.row;
  const firstGroup = !shows.first && firstShown?.kind === 'group';
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
  // Once what's open is answered, Next waiting opens the one waiting longest of those shown.
  const waitingNext = nextWaiting(shownRows, open ?? group);
  const next =
    waitingNext &&
    (() =>
      waitingNext.kind === 'group' ? openGroup(waitingNext.row.id) : select(waitingNext.row.id));
  useShortcuts(page, layout !== undefined && layout !== 'phone');
  // While Slack shows: each update brings the workspaces as they are, so it goes once Slack is
  // updated.
  const updating =
    manages && model.workspaces ? (inbox?.workspaces.filter((each) => each.needsUpdate) ?? []) : [];
  const done = useAction(async (conversation: Conversation) =>
    show(await markDone(conversation.id, conversation.last)),
  );
  const doneGroup = useAction(async (listed: Group) =>
    showGroup(await markGroupDone(listed.id, listed.last)),
  );
  const connect = manages && inbox?.workspaces.length === 0 && inbox.phones.length === 0;
  const unlisted =
    pinnedNow && !shownRows.some((each) => each.row.id === pinnedNow.row.id)
      ? pinnedNow
      : undefined;
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
          <Header title="Dispatch Phone">
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
            {working.length > 0 && <CopyChatLink workspaces={working} />}
            {(working.length > 0 || texting) && (
              <button
                className="primary"
                aria-label="New message"
                onClick={() => setComposing(true)}
              >
                <SquarePen size={16} />
                <span>New message</span>
              </button>
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
                previousDays={previousOn === today}
                showPreviousDays={(shown) => showPreviousOn(shown ? today : undefined)}
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
              {pane.kind === 'draft' ? (
                <GroupDraft
                  workspaces={working}
                  timeZone={timeZone}
                  drafts={drafts}
                  picks={picks}
                  pick={setPicks}
                  back={closed}
                  drafted={setDraft}
                  sent={sentGroup}
                />
              ) : pane.kind === 'group' ? (
                <GroupPane
                  key={pane.id}
                  id={pane.id}
                  listed={listedGroup}
                  workspaces={inbox.workspaces}
                  docked={docked}
                  drafts={drafts}
                  seen={seenGroup}
                  changed={changed}
                  timeZone={timeZone}
                  now={now}
                  show={showGroup}
                  readAfresh={refresh}
                  back={closed}
                  closed={closed}
                  next={next}
                  reply={(entry) => {
                    if (entry.conversationId) select(entry.conversationId);
                  }}
                  open={(person) => {
                    const target = memberTarget(person);
                    if (target) go(target);
                  }}
                />
              ) : pane.kind === 'conversation' ? (
                <ConversationPane
                  key={pane.id}
                  gifs={hasFeature(view, 'communications.gifs')}
                  id={pane.id}
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
                  me={user}
                  timeZone={timeZone}
                  now={now}
                  show={show}
                  readAfresh={refresh}
                  back={closed}
                  closed={closed}
                  openGroup={openGroup}
                  todays={todays.read}
                  open={go}
                  next={next}
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
            texting={texting}
            reach={reach}
            group={startGroup}
            close={() => setComposing(false)}
          />
        )}
      </Shown>
    </div>
  );
}

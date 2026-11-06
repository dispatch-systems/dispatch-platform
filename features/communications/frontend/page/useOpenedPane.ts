import { useCallback, useEffect, useRef, useState } from 'react';
import type { Communications, Conversation, Group } from '../../api/index.js';
import {
  hashQuery,
  parseHash,
  pushHashQuery,
  replaceHashQuery,
} from '../../../../core/shell/frontend/runtime/navigation.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { Row } from '../inbox/inbox.js';
import { groupsSection, lists, sectionOf, type Section } from '../inbox/sections.js';
import { started, type Target } from '../conversations/target.js';
import type { Draft } from '../slack/groups/GroupDraft.js';
import type { Pick } from '../slack/groups/groups.js';

// Pane selection, its browser-history step and the turn allowed to finish opening a target.
type Address = { conversation?: string; group?: string };
type Opened = (
  | { kind: 'none' }
  | { kind: 'conversation'; id: string }
  | { kind: 'group'; id: string }
  | { kind: 'draft' }
) & { address: Address };

/** Raw query values also drive list/pin bookkeeping, distinguishing empty from absent. A group
 * takes pane priority when both keys name an ID, while the list still reads the conversation. */
function atAddress(address: Address): Opened {
  if (address.group) return { kind: 'group', id: address.group, address };
  if (address.conversation) return { kind: 'conversation', id: address.conversation, address };
  return { kind: 'none', address };
}

/** What a step of the browser's history holds when it opened from the list, one pane at a time,
 * so closing it goes back to the list, not to whatever came before, as a notification's link. */
const FROM_LIST = 'communicationsFromList';
/** Whether the step of the browser's history now opened from the list. */
const fromList = () => (history.state as Record<string, unknown> | null)?.[FROM_LIST] === true;
/** What the address says is open. */
function addressed(): Opened {
  const query = hashQuery();
  return atAddress({
    conversation: query.get('open') ?? undefined,
    group: query.get('group') ?? undefined,
  });
}

export function useOpenedPane({
  phone,
  inbox,
  section,
  sections,
  choose,
  chooseSection,
  showGroup,
}: {
  phone: boolean;
  inbox: Communications | undefined;
  section: Section;
  sections: Section[];
  choose: (team?: string) => void;
  chooseSection: (section: string) => void;
  showGroup: (group: Group) => void;
}) {
  const [opened, setOpened] = useState<Opened>(addressed);
  const { conversation: open, group } = opened.address;
  const drafting = opened.kind === 'draft' ? true : undefined;
  const reading = open || group || drafting;
  const [pinned, setPinned] = useState<Row>();
  const [composing, setComposing] = useState(false);
  // Whom a message to a group is to, kept as another opens, and what the list's row says of it.
  const [picks, setPicks] = useState<Pick[]>([]);
  const [draft, setDraft] = useState<Draft>({ name: '', people: 0 });
  // How many times what's open changed, or one to start was chosen, as one started opens only if
  // nothing else did meanwhile.
  const turns = useRef(0);
  /** Shows what `next` names. */
  const adopt = useCallback((next: Opened) => {
    turns.current++;
    setOpened(next);
    // One pinned to the top of the list goes once another is open.
    setPinned((kept) =>
      kept?.row.id === (next.address.conversation ?? next.address.group) ? kept : undefined,
    );
  }, []);
  /** Shows what `next` names, and keeps it in the address: one pane at a time, opened from the
   * list, as a step of the browser's history, and closed, by going back that step. */
  const opening = useCallback(
    (next: Opened) => {
      const { conversation, group } = next.address;
      const query: Record<string, string> = conversation
        ? { open: conversation }
        : group
          ? { group }
          : {};
      const shows = Boolean(next.address.conversation || next.address.group);
      if (phone && shows && !reading) pushHashQuery(query, { [FROM_LIST]: true });
      else if (phone && !shows && next.kind !== 'draft' && fromList()) history.back();
      else replaceHashQuery(query);
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
      const shows = Boolean(next.address.conversation || next.address.group);
      if (shows || phone) adopt(next);
    };
    addEventListener('hashchange', followed);
    return () => removeEventListener('hashchange', followed);
  }, [adopt, phone]);
  const select = useCallback((id?: string) => opening(atAddress({ conversation: id })), [opening]);
  const openGroup = useCallback((id: string) => opening(atAddress({ group: id })), [opening]);
  const closed = useCallback(() => opening(atAddress({})), [opening]);
  // What's open, as last read, is pinned to the top of the list while the list doesn't show it:
  // a conversation holding no message yet, as New message opens it, one listed under a group, or
  // one older than those listed.
  const seen = useCallback(
    (conversation: Conversation) => setPinned({ kind: 'conversation', row: conversation }),
    [],
  );
  const seenGroup = useCallback((listed: Group) => setPinned({ kind: 'group', row: listed }), []);
  // What a target names, opened: a conversation there is, or one started, pinned until it holds
  // a message, unless something else was opened, or another chosen, while it started.
  const reach = async (target: Target) => {
    if (target.kind === 'conversation') select(target.id);
    else {
      const turn = ++turns.current;
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
  const startGroup = () => {
    setComposing(false);
    chooseSection(groupsSection);
    opening({ kind: 'draft', address: {} });
  };
  const sentGroup = (sent: Group) => {
    setPicks([]);
    setDraft({ name: '', people: 0 });
    showGroup(sent);
    openGroup(sent.id);
  };
  return {
    pane: opened,
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
  };
}

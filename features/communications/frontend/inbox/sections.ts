import type { Communications, Conversation } from '../../api/index.js';
import type { Listing } from '../../api/client.js';

// The list's sections, each defined once, in the order the switch shows them: its stable ID, kept
// in each member's browser as the section they chose; whether the DSP has it; which conversations
// are its own, and whether the groups are; whether it lists every section's too; where it says
// older answered ones of its own are left to read, as it pages; where its own are read and
// searched; and whether it shows one workspace's or every one's. How each shows is
// page/section-views.tsx's, keyed by the same IDs.

export type SectionModel = {
  /** Whether the DSP has it, as the overview and each update say. */
  offered: (inbox: Pick<Communications, 'routes' | 'texting' | 'workspaces'>) => boolean;
  /** Whether `conversation` is its own: every conversation is one section's, but a blocked
   * number's. */
  holds: (conversation: Conversation) => boolean;
  /** Whether the groups are its own. */
  groups: boolean;
  /** Whether it lists every section's conversations and the groups besides its own, the latest
   * written in first, as Recents does; else its own, those waiting first. */
  everything: boolean;
  /** Where the overview says whether older answered rows of its own are left to read; none for
   * one read whole, by its own read. */
  more?: 'more' | 'moreTexts';
  /** Where its own conversations are read from and searched: in a section of workspaces, every
   * one's or `team`'s. */
  listing: (team?: string) => Listing;
  /** Whether it shows one workspace's conversations, as chosen, or every one's. */
  workspaces: boolean;
};

export const SECTIONS = [
  {
    // Recents: every section's conversations and the groups, always; its own, the numbers that
    // text the DSP's phones, but those blocked. By the ID it shipped with as Messaging, which
    // members' browsers keep.
    id: 'messaging',
    offered: () => true,
    holds: (conversation) => conversation.kind === 'number' && !conversation.blocked,
    groups: false,
    everything: true,
    more: 'moreTexts',
    listing: () => ({ kind: 'number' }),
    workspaces: false,
  },
  {
    // Today's Routes: the drivers on a route today and their work phones, while the DSP has live
    // route data, listed by its own read (todays-routes/).
    id: 'routes',
    offered: (inbox) => inbox.routes,
    holds: (conversation) => conversation.kind === 'work',
    groups: false,
    everything: false,
    listing: () => ({ kind: 'work' }),
    workspaces: false,
  },
  {
    // Slack: its workspaces' direct messages and the groups it messaged, while it has one.
    id: 'slack',
    offered: (inbox) => inbox.workspaces.length > 0,
    holds: (conversation) => conversation.kind === 'slack',
    groups: true,
    everything: false,
    more: 'more',
    listing: (team) => ({ team }),
    workspaces: true,
  },
] as const satisfies readonly ({ id: string } & SectionModel)[];

export type Section = (typeof SECTIONS)[number]['id'];
type Defined = { id: Section } & SectionModel;

/** The section `id`. */
export const sectionNamed = (id: Section): Defined => SECTIONS.find((each) => each.id === id)!;
/** The section a conversation is its own of, if any: none for a number blocked. */
export const sectionOf = (conversation: Conversation): Section | undefined =>
  SECTIONS.find((each) => each.holds(conversation))?.id;
/** The section the groups are its own of. */
export const groupsSection = SECTIONS.find((each: Defined) => each.groups)!.id;
/** Whether `section` lists what's `of` a section, its own or every one's. */
export const lists = (section: Section, of: Section | undefined) =>
  of !== undefined && (of === section || sectionNamed(section).everything);
/** The section a page opens on with none chosen: the first of those `offered` listing a
 * conversation, or a group, in `inbox`; with none, the first offered. */
export function openingSection(
  offered: readonly Section[],
  inbox: Pick<Communications, 'conversations' | 'groups'>,
): Section | undefined {
  const holding = (section: Section) =>
    inbox.conversations.some((conversation) => lists(section, sectionOf(conversation))) ||
    (inbox.groups.length > 0 && lists(section, groupsSection));
  return offered.find(holding) ?? offered[0];
}

import type { Communications, Conversation } from '../../api/index.js';
import type { Listing } from '../../api/client.js';

// The list's sections, each defined once, in the order the switch shows them: its stable ID, kept
// in each member's browser as the section they chose; whether the DSP has it; which conversations
// it lists, and whether the groups too; where it says older answered ones are left to read, as it
// pages; where its conversations are read and searched; and whether it shows one workspace's or
// every one's. How each shows is page/sections.tsx's, keyed by the same IDs.

export type SectionModel = {
  /** Whether the DSP has it, as the overview and each update say. */
  offered: (inbox: Pick<Communications, 'routes' | 'texting' | 'workspaces'>) => boolean;
  /** Whether it lists `conversation`. */
  holds: (conversation: Conversation) => boolean;
  /** Whether it lists the groups too. */
  groups: boolean;
  /** Where the overview says whether older answered rows of it are left to read; none for one
   * read whole, by its own read. */
  more?: 'more' | 'moreTexts';
  /** Where its conversations are read from and searched: in a section of workspaces, every one's or
   * `team`'s. */
  listing: (team?: string) => Listing;
  /** Whether it shows one workspace's conversations, as chosen, or every one's. */
  workspaces: boolean;
};

export const SECTIONS = [
  {
    // Today's Routes: the drivers on a route today and their work phones, while the DSP has live
    // route data, listed by its own read (todays-routes/).
    id: 'routes',
    offered: (inbox) => inbox.routes,
    holds: (conversation) => conversation.kind === 'work',
    groups: false,
    listing: () => ({ kind: 'work' }),
    workspaces: false,
  },
  {
    // Messaging: the numbers that text the DSP's phones, but those blocked, always.
    id: 'messaging',
    offered: (inbox) => inbox.texting,
    holds: (conversation) => conversation.kind === 'number' && !conversation.blocked,
    groups: false,
    more: 'moreTexts',
    listing: () => ({ kind: 'number' }),
    workspaces: false,
  },
  {
    // Slack: its workspaces' direct messages and the groups it messaged, while it has one.
    id: 'slack',
    offered: (inbox) => inbox.workspaces.length > 0,
    holds: (conversation) => conversation.kind === 'slack',
    groups: true,
    more: 'more',
    listing: (team) => ({ team }),
    workspaces: true,
  },
] as const satisfies readonly ({ id: string } & SectionModel)[];

export type Section = (typeof SECTIONS)[number]['id'];
type Defined = { id: Section } & SectionModel;

/** The section `id`. */
export const sectionNamed = (id: Section): Defined => SECTIONS.find((each) => each.id === id)!;
/** The section a conversation is listed in, if any: none for a number blocked. */
export const sectionOf = (conversation: Conversation): Section | undefined =>
  SECTIONS.find((each) => each.holds(conversation))?.id;
/** The section the groups are listed in. */
export const groupsSection = SECTIONS.find((each: Defined) => each.groups)!.id;

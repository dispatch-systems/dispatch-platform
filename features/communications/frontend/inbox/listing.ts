import type { Conversation, Group, Writing } from '../../api/index.js';

/** What each row of a list of conversations and groups is handed. */
export type Listing = {
  /** What's open: a conversation or a group, by its ID. */
  open?: string;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  select: (id: string) => void;
  openGroup: (id: string) => void;
  done: (conversation: Conversation) => void;
  doneGroup: (group: Group) => void;
};

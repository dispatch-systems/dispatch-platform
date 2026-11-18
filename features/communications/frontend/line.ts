import { createContext, useContext } from 'react';
import type { Permission } from '../../../core/accounts/api/index.js';
import { dispatchTexting, type LineTexting } from '../api/client.js';

/**
 * A line of the DSP's phones, as a page texts on it: Dispatch Phone's own, with Slack, Today's
 * Routes, groups, profiles, quick replies, GIFs and adding a number to a driver; or another
 * feature's, its numbers' texts alone, with its own page, routes, count, alerts, banner of phones
 * down and Display section, which that feature's manifest names (`lines.ts`). Each reads and
 * answers only its own conversations, and what a browser keeps of it is its own.
 */
export type TextingLine = {
  /** Its feature's name: none for Dispatch Phone's own. */
  id?: string;
  /** What its page's heading says, and its alerts and pushes say before who wrote, but Dispatch
   * Phone's own, whose say who wrote alone. */
  name: string;
  /** Its page, by its ID, where its conversations open. */
  page: string;
  /** What a member holds to use it. */
  permission: Permission;
  /** Its routes. */
  texting: LineTexting;
  /** Its section of Settings' Display, each member's own: how its page looks and alerts them. */
  display: string;
  /** What a member holds to manage its phones, as managing all the DSP's connections allows too:
   * its banner of phones down links to their card for them. */
  phones: Permission;
  /** What its banner calls its phone, while it has one alone. */
  phone: string;
};

export const DISPATCH_PHONE: TextingLine = {
  name: 'Dispatch Phone',
  page: 'communications',
  permission: 'communications.use',
  texting: dispatchTexting,
  display: 'communications.display',
  phones: 'phones.manage',
  phone: 'Dispatch phone',
};

/** The line the page draws: Dispatch Phone's own, unless another's page says so. */
export const Line = createContext(DISPATCH_PHONE);
export const useLine = () => useContext(Line);

/** Whether `line` is Dispatch Phone's own. */
export const own = (line: TextingLine) => line.id === undefined;
/** What the browser keeps of `line` at `where` is named by: Dispatch Phone's own as it shipped,
 * another's apart from it. */
export const keptAt = (line: TextingLine, where: string) =>
  line.id === undefined ? where : `${where}:${line.id}`;

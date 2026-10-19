import { useSyncExternalStore } from 'react';
import { leaving, type Guards } from '../../../core/shell/frontend/runtime/leave.js';
import { hashQuery, replaceHashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import { forgetDrafts } from './drafts.js';

// Settings' address: the page open (`?tab=<page>`, its id, which never changes) and a section of
// it to point out (`&find=<section>`). Its sidebar moves it in place, as a link followed or Back
// moves it, and its page follows.

const moves = new Set<() => void>();
/** Calls `follow` each time the address moves; stops when the answer is called. */
export function followAddress(follow: () => void) {
  moves.add(follow);
  window.addEventListener('hashchange', follow);
  return () => {
    moves.delete(follow);
    window.removeEventListener('hashchange', follow);
  };
}
/** The page an address, a hash, names, if it names one. */
export const pageIn = (hash: string) => new URLSearchParams(hash.split('?')[1]).get('tab');
/** The page the address names, if it names one. */
export const addressedPage = () => pageIn(location.hash);
/** The page the address names, as it moves. */
export const useAddressedPage = () => useSyncExternalStore(followAddress, addressedPage);
/** The section the address asks to point out, if it asks. */
export const addressedSection = () => hashQuery().get('find') ?? undefined;
/** Drops the section to point out from the address, so opening it again doesn't. */
export function forgetSection() {
  const query = hashQuery();
  if (!query.has('find')) return;
  query.delete('find');
  replaceHashQuery(Object.fromEntries(query));
}

/** The editors open on the page, as a schedule's, each asked before the page moves on. */
export const guards: Guards = new Set();
/**
 * Asks the editors open on the page in turn, and once every one has let it, saved or discarded,
 * forgets the sections' edits and moves on: `then`.
 */
export const leavePage = (then: () => void) =>
  leaving(guards, () => {
    forgetDrafts();
    then();
  });
/**
 * Moves Settings from the page open, `from`, to `page`, once every editor open on it has let it,
 * pointing out `section` there.
 */
export function moveTo(page: string, from: string | undefined, section?: string) {
  const move = () => {
    replaceHashQuery(section ? { tab: page, find: section } : { tab: page });
    for (const follow of moves) follow();
  };
  if (page === from) move();
  else leavePage(move);
}

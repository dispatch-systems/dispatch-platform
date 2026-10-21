// What stands in for a workspace's logo: the first letters of its name, in one of the
// dashboard's identity tones, chosen by the name so it's the same wherever the workspace shows.

/** A workspace's initials: the first letters of its first two words, or of its one word. */
export function initials(name: string) {
  const words = (name.toLocaleUpperCase('en-US').match(/[\p{L}\p{N}]+/gu) ?? []).map((word) => [
    ...word,
  ]);
  const [first, second] = words;
  if (!first) return '?';
  return second ? `${first[0]}${second[0]}` : first.slice(0, 2).join('');
}
/** The identity tone a workspace's initials are drawn in. */
export const tone = (name: string) =>
  ['violet', 'sage', 'slate', 'steel'][
    [...name].reduce((hash, ch) => (hash * 31 + ch.codePointAt(0)!) >>> 0, 0) % 4
  ]!;

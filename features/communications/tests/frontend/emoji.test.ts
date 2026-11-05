import assert from 'node:assert/strict';
import test from 'node:test';
import data from 'emojibase-data/en/data.json' with { type: 'json' };
import { categorize, inTone, search } from '../../frontend/conversations/emoji.js';

// What the picker offers, from Emojibase's own data: only emoji an iPhone on iOS 14.5 or an
// Android phone on Android 12 draws (Emoji 13.1), so a text never reaches a driver as an empty box.

test('the picker offers only emoji and skin tones that iOS 14.5 and Android 12 draw', () => {
  const version = new Map(
    data.flatMap((entry) => [
      [entry.emoji, entry.version],
      ...(entry.skins ?? []).map((skin) => [skin.emoji, skin.version] as const),
    ]),
  );
  const offered = categorize(data).flatMap((category) => category.emoji);
  const newer = offered.flatMap((emoji) =>
    [emoji.emoji, ...Object.values(emoji.tones ?? {})].filter(
      (each) => (version.get(each) ?? Infinity) > 13.1,
    ),
  );
  assert.deepEqual(newer, []);
  const shown = new Set(offered.map((emoji) => emoji.emoji));
  assert.ok(!shown.has('🫠'), 'Emoji 14’s melting face is left out');
  // Skin tones come with what has them, newer tones left out of what's older.
  const thumbsUp = offered.find((emoji) => emoji.emoji === '👍')!;
  assert.equal(inTone(thumbsUp, 4), '👍🏾');
  const handshake = offered.find((emoji) => emoji.emoji === '🤝')!;
  assert.equal(inTone(handshake, 4), '🤝', 'its tones came with Emoji 14');
});

test('search finds an emoji by the start of each word of its name or tags, whatever their case', () => {
  const categories = categorize(data);
  const found = (typed: string) => search(categories, typed).map((emoji) => emoji.emoji);
  assert.equal(found('thumbs u')[0], '👍');
  // Emojibase capitalizes some tags, as 👌's "OK".
  assert.ok(found('ok').includes('👌'));
  assert.ok(found('Shaka').includes('🤙'));
});

import type { LucideIcon } from 'lucide-react';
import { Apple, Car, Clock, Flag, Hand, Heart, Lamp, Leaf, Smile, Volleyball } from 'lucide-react';

// The emoji a message may carry, from Emojibase's English data, loaded the first time the picker
// opens: those every phone a text reaches draws, up to Emoji `SUPPORTED` (iOS 14.5, Android 12),
// so none shows as an empty box on an older phone. Each is in its category, in Unicode's order,
// with the words it's found by and its skin tones, as the device's own emoji font draws them.

/** The newest Emoji version offered: iOS 14.5's and Android 12's. */
const SUPPORTED = 13.1;

/** A skin tone: none, then light to dark, as Unicode numbers them. */
export type Tone = 0 | 1 | 2 | 3 | 4 | 5;
export const TONES: readonly Tone[] = [0, 1, 2, 3, 4, 5];

export type Emoji = {
  emoji: string;
  label: string;
  /** Its label's words and tags, lowercased, as it's searched by. */
  words: readonly string[];
  /** It in each skin tone it comes in, by the tone, up to `SUPPORTED`. */
  tones?: Partial<Record<Tone, string>>;
};
export type Category = { key: string; label: string; icon: LucideIcon; emoji: Emoji[] };

/** Emojibase's groups, by its number for each, as the picker names them; skin tones aren't one. */
const GROUPS: { [group: number]: Omit<Category, 'emoji'> } = {
  0: { key: 'smileys', label: 'Smileys', icon: Smile },
  1: { key: 'people', label: 'People', icon: Hand },
  3: { key: 'nature', label: 'Nature', icon: Leaf },
  4: { key: 'food', label: 'Food', icon: Apple },
  5: { key: 'travel', label: 'Travel', icon: Car },
  6: { key: 'activities', label: 'Activities', icon: Volleyball },
  7: { key: 'objects', label: 'Objects', icon: Lamp },
  8: { key: 'symbols', label: 'Symbols', icon: Heart },
  9: { key: 'flags', label: 'Flags', icon: Flag },
};
/** Where the emoji used last show, before the categories. */
export const RECENT = { key: 'recent', label: 'Recent', icon: Clock };

/** One of Emojibase's emoji, as much of it as the picker reads. */
type Entry = {
  emoji: string;
  label: string;
  group?: number;
  order?: number;
  version: number;
  tags?: string[];
  skins?: { emoji: string; tone?: number | number[]; version: number }[];
};

/** Emojibase's emoji in the categories they show in, each holding those up to `supported`. */
export function categorize(entries: readonly Entry[], supported = SUPPORTED): Category[] {
  const sorted = entries
    .filter((entry) => entry.version <= supported && entry.group !== undefined)
    .sort((one, other) => (one.order ?? 0) - (other.order ?? 0));
  return Object.entries(GROUPS).map(([group, category]) => ({
    ...category,
    emoji: sorted.filter((entry) => entry.group === Number(group)).map(shown(supported)),
  }));
}

const shown =
  (supported: number) =>
  ({ emoji, label, tags = [], skins = [] }: Entry): Emoji => {
    // A tone of its own, not the pairs of two people's.
    const tones = skins.flatMap(({ tone, version, emoji: toned }) =>
      typeof tone === 'number' && version <= supported ? [[tone, toned] as const] : [],
    );
    return {
      emoji,
      label,
      words: [...label.split(/[\s:,]+/), ...tags].map((word) => word.toLowerCase()),
      tones: tones.length ? Object.fromEntries([[0, emoji], ...tones]) : undefined,
    };
  };

/** It as `tone` has it, or as it is when it comes in no tones. */
export const inTone = (emoji: Emoji, tone: Tone) => emoji.tones?.[tone] ?? emoji.emoji;

/** Those of `categories` a word of which starts with each word `typed` has. */
export function search(categories: readonly Category[], typed: string): Emoji[] {
  const wanted = typed.toLowerCase().split(/\s+/).filter(Boolean);
  return categories
    .flatMap((category) => category.emoji)
    .filter((emoji) => wanted.every((word) => emoji.words.some((each) => each.startsWith(word))));
}

let loading: Promise<Category[]> | undefined;
/** The categories, loaded once; loaded again after a load that failed. */
export function loadEmoji(): Promise<Category[]> {
  loading ??= import('emojibase-data/en/data.json').then(
    (data) => categorize(data.default),
    (cause: unknown) => {
      loading = undefined;
      throw cause;
    },
  );
  return loading;
}

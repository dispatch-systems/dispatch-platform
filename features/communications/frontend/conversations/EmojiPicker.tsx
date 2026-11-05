import { useEffect, useMemo, useRef, useState } from 'react';
import { LoaderCircle, Search, Smile } from 'lucide-react';
import { Popover } from '../../../../core/shell/frontend/ui/index.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import { ListEmpty } from '../shared/List.js';
import {
  inTone,
  loadEmoji,
  RECENT,
  search,
  TONES,
  type Category,
  type Emoji,
  type Tone,
} from './emoji.js';
import './emoji-picker.css';

// Adding an emoji to what's written: a smiley beside the paperclip opens every emoji a phone
// draws (emoji.ts), found by search or by category, the tabs over them going to each, the
// emoji used last first, in the skin tone chosen under them. Each picked goes in where the
// writer was and the picker stays open for another; a press outside it, Escape, or the smiley
// again closes it. The emoji used last and the tone are remembered in this browser.

const RECENT_KEY = 'dispatch-emoji-recent';
const TONE_KEY = 'dispatch-emoji-tone';
/** How many emoji used last show: two rows. */
const RECENT_COUNT = 16;
const TONE_NAMES = ['Default', 'Light', 'Medium-light', 'Medium', 'Medium-dark', 'Dark'];

export function EmojiPicker({
  disabled,
  pick,
}: {
  disabled: boolean;
  /** Puts `emoji` in what's written. */
  pick: (emoji: string) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover
      className="emoji-picker"
      triggerClassName="round"
      panelClassName="emoji-panel"
      label="Add emoji"
      trigger={<Smile size={18} />}
      open={open}
      disabled={disabled}
      onRequestOpen={() => setOpen(true)}
      onRequestClose={() => setOpen(false)}
    >
      <Emojis pick={pick} />
    </Popover>
  );
}

function Emojis({ pick }: { pick: (emoji: string) => void }) {
  const { fail } = useFeedback();
  const [categories, setCategories] = useState<Category[]>();
  const [typed, setTyped] = useState('');
  const [tone, setTone] = useState(readTone);
  // As they were when it opened, so what shows doesn't move under the pointer while picking.
  const [recent] = useState(readRecent);
  const [at, setAt] = useState<string>();
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let live = true;
    loadEmoji().then(
      (loaded) => live && setCategories(loaded),
      (cause: unknown) => live && fail(messageOf(cause)),
    );
    return () => {
      live = false;
    };
  }, [fail]);

  const byEmoji = useMemo(
    () =>
      new Map(categories?.flatMap((category) => category.emoji.map((each) => [each.emoji, each]))),
    [categories],
  );
  const sections = useMemo(() => {
    const used = recent.flatMap((emoji) => byEmoji.get(emoji) ?? []);
    return [...(used.length ? [{ ...RECENT, emoji: used }] : []), ...(categories ?? [])];
  }, [recent, byEmoji, categories]);
  const found = typed.trim() && categories ? search(categories, typed) : undefined;
  const shown = at ?? sections[0]?.key;

  const choose = (emoji: Emoji) => {
    pick(inTone(emoji, tone));
    const used = readRecent().filter((each) => each !== emoji.emoji);
    remember(RECENT_KEY, [emoji.emoji, ...used].slice(0, RECENT_COUNT).join(' '));
  };
  const chooseTone = (next: Tone) => {
    setTone(next);
    remember(TONE_KEY, String(next));
  };
  const go = (key: string) => {
    setTyped('');
    setAt(key);
    // Once the categories show again in place of what was found.
    requestAnimationFrame(() => {
      const section = list.current?.querySelector<HTMLElement>(`[data-category="${key}"]`);
      list.current?.scrollTo({ top: section?.offsetTop ?? 0 });
    });
  };
  // The tab of the category at the top of the list, as it scrolls.
  const scrolled = () => {
    const element = list.current!;
    const top = element.scrollTop + 8;
    const sections = [...element.querySelectorAll<HTMLElement>('[data-category]')];
    const current = sections.findLast((section) => section.offsetTop <= top) ?? sections[0];
    if (current) setAt(current.dataset.category);
  };
  const grid = (emoji: readonly Emoji[]) => (
    <div className="emoji-grid">
      {emoji.map((each) => (
        <button
          key={each.emoji}
          type="button"
          title={each.label}
          aria-label={each.label}
          onClick={() => choose(each)}
        >
          {inTone(each, tone)}
        </button>
      ))}
    </div>
  );

  return (
    <>
      <label className="emoji-search">
        <Search size={14} aria-hidden="true" />
        <input
          type="search"
          aria-label="Search emoji"
          placeholder="Search"
          value={typed}
          autoFocus={finePointer()}
          onChange={(event) => setTyped(event.target.value)}
          onKeyDown={(event) => {
            // Within what's written's form, Enter picks the first found rather than sending.
            if (event.key !== 'Enter') return;
            event.preventDefault();
            if (found?.[0]) choose(found[0]);
          }}
        />
      </label>
      <div className="emoji-tabs" role="tablist" aria-label="Categories">
        {sections.map(({ key, label, icon: Icon }) => (
          <button
            key={key}
            type="button"
            role="tab"
            title={label}
            aria-label={label}
            aria-selected={!found && key === shown}
            disabled={!categories}
            onClick={() => go(key)}
          >
            <Icon size={16} />
          </button>
        ))}
      </div>
      <div ref={list} className="emoji-list" onScroll={found ? undefined : scrolled}>
        {!categories ? (
          <LoaderCircle className="spin" size={20} aria-label="Loading" />
        ) : found ? (
          found.length ? (
            grid(found)
          ) : (
            <ListEmpty>No matches</ListEmpty>
          )
        ) : (
          sections.map((section) => (
            <section key={section.key} data-category={section.key} aria-label={section.label}>
              <h4>{section.label}</h4>
              {grid(section.emoji)}
            </section>
          ))
        )}
      </div>
      <div className="emoji-tones" role="radiogroup" aria-label="Skin tone">
        {TONES.map((each) => (
          <button
            key={each}
            type="button"
            role="radio"
            title={TONE_NAMES[each]}
            aria-label={TONE_NAMES[each]}
            aria-checked={each === tone}
            onClick={() => chooseTone(each)}
          >
            {inTone(THUMBS_UP, each)}
          </button>
        ))}
      </div>
    </>
  );
}

/** What each skin tone shows as. */
const THUMBS_UP: Emoji = {
  emoji: '👍',
  label: 'thumbs up',
  words: [],
  tones: { 0: '👍', 1: '👍🏻', 2: '👍🏼', 3: '👍🏽', 4: '👍🏾', 5: '👍🏿' },
};

/** Whether the pointer is a mouse's, so a field taking focus brings up no phone's keyboard. */
export const finePointer = () => matchMedia('(pointer: fine)').matches;

function readStored(key: string) {
  try {
    return localStorage.getItem(key) ?? '';
  } catch {
    return '';
  }
}
function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not kept: the picker starts as it did.
  }
}
function readRecent() {
  return readStored(RECENT_KEY).split(' ').filter(Boolean);
}
function readTone(): Tone {
  const stored = Number(readStored(TONE_KEY));
  return TONES.find((each) => each === stored) ?? 0;
}

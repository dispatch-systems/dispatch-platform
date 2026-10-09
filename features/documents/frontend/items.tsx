import {
  File,
  FileImage,
  FileText,
  FileType,
  Folder,
  Presentation,
  Sheet,
  type LucideIcon,
} from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { thumbnail, type DocumentsItem, type ItemKind } from '../api/client.js';

// What every view of Documents draws a file with: its kind's name, tile and first page, or
// Google's picture of it.

const KINDS: Record<ItemKind, { label: string; icon: LucideIcon }> = {
  folder: { label: 'Folder', icon: Folder },
  doc: { label: 'Google Doc', icon: FileText },
  sheet: { label: 'Google Sheet', icon: Sheet },
  slides: { label: 'Google Slides', icon: Presentation },
  pdf: { label: 'PDF', icon: FileType },
  image: { label: 'Image', icon: FileImage },
  file: { label: 'File', icon: File },
};
export const kindLabel = (kind: ItemKind) => KINDS[kind].label;

/** A file's kind as a small tinted tile. */
export function Tile({ kind, size = 16 }: { kind: ItemKind; size?: number }) {
  const Icon = KINDS[kind].icon;
  return (
    <span className={`documents-tile documents-tile-${kind}`} aria-hidden="true">
      <Icon size={size} />
    </span>
  );
}

/**
 * A file on a grid card: Google's picture of what it holds once the card nears the screen, and
 * a drawing of its first page until then, or when Google has none.
 */
export function Thumb({ kind, picture }: { kind: ItemKind; picture?: string | null }) {
  const [box, shown, lost] = usePicture(picture);
  if (shown)
    return (
      <div
        ref={box}
        className={`documents-thumb documents-thumb-picture${kind === 'image' ? ' documents-thumb-photo' : ''}`}
        aria-hidden="true"
      >
        <img src={shown} alt="" onError={lost} />
      </div>
    );
  return (
    <div ref={box} className={`documents-thumb documents-thumb-${kind}`} aria-hidden="true">
      <Drawing kind={kind} />
    </div>
  );
}

function Drawing({ kind }: { kind: ItemKind }) {
  if (kind === 'sheet')
    return (
      <div className="documents-page documents-grid">
        {Array.from({ length: 40 }, (_, i) => (
          <i key={i} className={i < 5 ? 'head' : i % 5 === 0 ? 'key' : undefined} />
        ))}
      </div>
    );
  if (kind === 'slides')
    return (
      <div className="documents-slide">
        <b />
        <i />
        <i className="short" />
      </div>
    );
  if (kind === 'image')
    return (
      <>
        <span />
        <span />
        <span />
      </>
    );
  return (
    <div className="documents-page">
      <b />
      <i />
      <i />
      <i className="short" />
      <i />
      <i />
      <i className="short" />
    </div>
  );
}

/**
 * The picture at `url`, fetched once its box nears the screen, as an address the page shows;
 * none until it comes, and none if it can't be fetched or shown, so the card keeps its
 * drawing. `lost` gives up on one the browser couldn't show.
 */
function usePicture(url: string | null | undefined) {
  const box = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState<{ url: string; src?: string }>();
  useEffect(() => {
    const at = box.current;
    if (!url || !at) return;
    const stop = new AbortController();
    let src: string | undefined;
    const near = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        near.disconnect();
        thumbnail(url, stop.signal).then(
          (blob) => {
            if (stop.signal.aborted) return;
            src = URL.createObjectURL(blob);
            setShown({ url, src });
          },
          () => {},
        );
      },
      { rootMargin: '200px' },
    );
    near.observe(at);
    return () => {
      near.disconnect();
      stop.abort();
      if (src) URL.revokeObjectURL(src);
    };
  }, [url]);
  const lost = () => {
    if (shown?.src) URL.revokeObjectURL(shown.src);
    setShown(shown && { url: shown.url });
  };
  return [box, shown && shown.url === url ? shown.src : undefined, lost] as const;
}

/** A file's size, for one anyone uploaded. */
export function size(bytes: number) {
  if (bytes < 1000) return `${bytes} B`;
  if (bytes < 1_000_000) return `${Math.round(bytes / 1000)} KB`;
  if (bytes < 1_000_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
}
/** What the list's type column says of an item. */
export function typeOf(item: DocumentsItem) {
  if (item.kind === 'folder') return item.items === 1 ? '1 item' : `${item.items ?? 0} items`;
  return item.size === null ? kindLabel(item.kind) : size(item.size);
}

const day = (value: Date, timeZone: string) =>
  new Intl.DateTimeFormat('en-CA', { timeZone, dateStyle: 'short' }).format(value);
/**
 * When something changed, as people say it: "40 minutes ago", "3 hours ago", "Yesterday",
 * "Oct 3", or with its year when it isn't this one.
 */
export function edited(at: string, timeZone: string, now = new Date()) {
  const then = new Date(at);
  const minutes = Math.max(0, Math.round((now.getTime() - then.getTime()) / 60_000));
  if (minutes < 1) return 'Just now';
  if (minutes < 60) return minutes === 1 ? '1 minute ago' : `${minutes} minutes ago`;
  if (day(then, timeZone) === day(now, timeZone)) {
    const hours = Math.round(minutes / 60);
    return hours === 1 ? '1 hour ago' : `${hours} hours ago`;
  }
  const yesterday = new Date(now.getTime() - 86_400_000);
  if (day(then, timeZone) === day(yesterday, timeZone)) return 'Yesterday';
  const sameYear =
    new Intl.DateTimeFormat('en-US', { timeZone, year: 'numeric' }).format(then) ===
    new Intl.DateTimeFormat('en-US', { timeZone, year: 'numeric' }).format(now);
  return new Intl.DateTimeFormat('en-US', {
    timeZone,
    month: 'short',
    day: 'numeric',
    ...(sameYear ? {} : { year: 'numeric' }),
  }).format(then);
}
/** `edited` in the middle of a sentence: "Edited yesterday", "Edited Oct 3". */
export const editedInline = (at: string, timeZone: string) => {
  const said = edited(at, timeZone);
  return /^[A-Z][a-z]{2} \d/.test(said) ? said : said.toLowerCase();
};

const TONES = ['slate', 'sage', 'violet', 'steel'] as const;
/** Someone's initials, in a tone of their name, as the team's avatars are drawn. */
export function Initials({ name }: { name: string }) {
  const words = name.split(/\s+/).filter(Boolean);
  const initials = words
    .slice(0, 2)
    .map((word) => word[0]!.toUpperCase())
    .join('');
  const tone = TONES[[...name].reduce((sum, char) => sum + char.charCodeAt(0), 0) % TONES.length];
  return (
    <span className={`documents-initials documents-tone-${tone}`} aria-hidden="true">
      {initials || '?'}
    </span>
  );
}

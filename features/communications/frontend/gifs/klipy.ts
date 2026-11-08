import type { GifSearch, GifVersion, MessageGif } from '../../api/index.js';

// KLIPY's GIFs, searched straight from the browser as KLIPY's terms have it: at the address the
// server gives (`GifSearch`), with its content filter and the member's anonymous ID there.

/** A GIF as KLIPY's search and trending lists answer it, by the sizes it comes in. */
type Found = {
  slug: string;
  title?: string;
  type?: string;
  file?: Partial<Record<'hd' | 'md' | 'sm' | 'xs', { gif?: GifVersion }>>;
};

/** How many a page of results holds. */
const PAGE = 24;

/** KLIPY's trending GIFs while `q` is empty, else those it finds for `q`, in KLIPY's order. */
export async function findGifs(search: GifSearch, q: string, signal: AbortSignal) {
  const typed = q.trim();
  const url = new URL(
    `${search.address}/gifs/${typed ? 'search' : 'trending'}`,
    window.location.origin,
  );
  if (typed) url.searchParams.set('q', typed);
  url.searchParams.set('per_page', String(PAGE));
  url.searchParams.set('customer_id', search.customer);
  url.searchParams.set('content_filter', search.filter);
  const answer = await fetch(url, { signal });
  if (!answer.ok) throw new Error(`KLIPY answered ${answer.status}`);
  const found = (await answer.json()) as { data?: { data?: Found[] } };
  return (found.data?.data ?? []).map(asGif).filter((gif): gif is MessageGif => gif !== null);
}

/** Tells KLIPY the member shared `gif`, as its terms ask; whether it heard isn't waited on. */
export function shareGif(search: GifSearch, gif: MessageGif) {
  const url = new URL(`${search.address}/gifs/share/${gif.slug}`, window.location.origin);
  void fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ customer_id: search.customer }),
    keepalive: true,
  }).catch(() => undefined);
}

/** The size of `gif` shown `width` pixels wide: the smallest at least as wide, else the largest. */
export function shownAt(gif: MessageGif, width: number) {
  const bySize = [...gif.versions].sort((a, b) => a.width - b.width);
  return bySize.find((version) => version.width >= width) ?? bySize[bySize.length - 1]!;
}

/** A GIF KLIPY found, by its GIF sizes alone; none when it has none. */
function asGif(found: Found): MessageGif | null {
  const versions = (['hd', 'md', 'sm', 'xs'] as const)
    .map((size) => found.file?.[size]?.gif)
    .filter((version): version is GifVersion => Boolean(version?.url));
  if (!found.slug || !versions.length) return null;
  return { slug: found.slug, title: found.title ?? '', versions };
}

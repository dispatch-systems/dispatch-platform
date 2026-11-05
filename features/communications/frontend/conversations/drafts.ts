// What's written in each conversation's box, a group's, and a message to a group, and not sent
// yet: kept by the page, outside the box, so it's there again as it opens again, after another was
// open, Back or a notification followed; and in the browser's tab for each member at each DSP, so
// a reload brings it back too. Only the text: photos go with the box. Gone once it's sent, but for
// what was written while it went.

const key = (dsp: string, user: string, draft: string) =>
  `dispatch-communications-draft:${dsp}:${user}:${draft}`;

/** What `now` holds still to send once `sent` went: all of it, unless it begins with what went. */
const unsent = (now: string, sent: string) =>
  now.startsWith(sent) ? now.slice(sent.length).trimStart() : now;

export class Drafts {
  private readonly texts = new Map<string, string>();
  private readonly watchers = new Map<string, Set<() => void>>();
  constructor(
    private readonly dsp: string,
    private readonly user: string,
  ) {}

  /** What's written in `draft`, as kept. */
  text(draft: string) {
    let text = this.texts.get(draft);
    if (text === undefined) {
      try {
        text = sessionStorage.getItem(key(this.dsp, this.user, draft)) ?? '';
      } catch {
        text = '';
      }
      this.texts.set(draft, text);
    }
    return text;
  }
  /** Keeps `text` as what's written in `draft`. */
  write(draft: string, text: string) {
    if (text === this.text(draft)) return;
    this.texts.set(draft, text);
    try {
      if (text) sessionStorage.setItem(key(this.dsp, this.user, draft), text);
      else sessionStorage.removeItem(key(this.dsp, this.user, draft));
    } catch {
      // Not kept in the tab, it's still kept while the page is open.
    }
    for (const watch of this.watchers.get(draft) ?? []) watch();
  }
  /** Takes what went, `text`, out of `draft`: what was written while it went stays. */
  sent(draft: string, text: string) {
    this.write(draft, unsent(this.text(draft), text));
  }
  /** Calls `watch` as `draft` changes, until what this answers is called. */
  watch(draft: string, watch: () => void) {
    const watching = this.watchers.get(draft) ?? new Set();
    watching.add(watch);
    this.watchers.set(draft, watching);
    return () => {
      watching.delete(watch);
      if (!watching.size) this.watchers.delete(draft);
    };
  }
}

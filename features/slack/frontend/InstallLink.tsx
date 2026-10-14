import { useState } from 'react';
import { useFeedback } from '../../../core/shell/frontend/runtime/feedback.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { cancelSlackLink, createSlackLink, type SlackLink } from '../api/client.js';
import { day } from './day.js';

// The DSP's install link on its Slack card, for whoever runs its Slack.

/**
 * Puts the new install link on the clipboard. The link is still being made, so the clipboard
 * is given it to come: Safari lets a page write there only while answering the click.
 */
async function toClipboard(link: Promise<string>) {
  if (typeof ClipboardItem !== 'undefined' && navigator.clipboard?.write) {
    const text = link.then((url) => new Blob([url], { type: 'text/plain' }));
    await navigator.clipboard.write([new ClipboardItem({ 'text/plain': text })]);
  } else await navigator.clipboard.writeText(await link);
}

/** Making the link and cancelling it, after which the card reads its account again. */
export function useInstallLink(refresh: () => void) {
  const { notify } = useFeedback();
  // The link made here, shown once: Dispatch keeps only its hash.
  const [made, setMade] = useState('');
  const copy = useAction(async () => {
    const link = createSlackLink().then((created) => created.url);
    const copied = await toClipboard(link).then(
      () => true,
      () => false,
    );
    // Shown on the card too, for a browser that kept it from the clipboard.
    setMade(await link);
    refresh();
    if (copied) notify('Install link copied');
  });
  const cancel = useAction(
    async () => {
      await cancelSlackLink();
      setMade('');
      refresh();
    },
    { success: 'Install link cancelled' },
  );
  return { made, copy, cancel };
}

/** The link while it works: the one just made, when expires, and cancelling it. */
export function InstallLink({
  link,
  made,
  timeZone,
  cancel,
}: {
  link: SlackLink | null;
  made: string;
  timeZone: string;
  cancel: ReturnType<typeof useInstallLink>['cancel'];
}) {
  if (!link && !made) return null;
  return (
    <div className="slack-link">
      {made && (
        <input
          readOnly
          aria-label="Install link"
          value={made}
          onFocus={(event) => event.currentTarget.select()}
        />
      )}
      <div className="slack-link-row">
        {link && <small>Install link · expires {day(link.expiresAt, timeZone)}</small>}
        <button className="text-button" disabled={cancel.busy} onClick={() => void cancel.run()}>
          Cancel link
        </button>
      </div>
    </div>
  );
}

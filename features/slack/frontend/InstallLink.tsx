import { useState } from 'react';
import { useFeedback } from '../../../core/shell/frontend/runtime/feedback.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { Badge } from '../../../core/shell/frontend/ui/index.js';
import {
  cancelSlackLink,
  createSlackLink,
  slackAccountChanged,
  type SlackLink,
} from '../api/client.js';
import { day } from './day.js';
import { SlackCard } from './SlackCard.js';

// The DSP's install link, for whoever runs its Slack: made from New, and on a card of its own
// while it works.

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

/**
 * Making the link, in place of any before, for a workspace to have the name it's given: `copied`
 * runs once it's on the clipboard.
 */
export function useInstallLink(copied: () => void) {
  const { notify } = useFeedback();
  // The link made here, for a browser that kept it from the clipboard: shown this once, as
  // Dispatch keeps only its hash.
  const [made, setMade] = useState('');
  const copy = useAction(async (label: string) => {
    const link = createSlackLink(label).then((created) => created.url);
    const onClipboard = await toClipboard(link).then(
      () => true,
      () => false,
    );
    const url = await link;
    slackAccountChanged();
    if (onClipboard) {
      notify('Install link copied');
      copied();
    } else setMade(url);
  });
  return { made, copy };
}

/** The link's card while it works: when it expires, and cancelling it. */
export function LinkCard({ link, timeZone }: { link: SlackLink; timeZone: string }) {
  const cancel = useAction(
    async () => {
      await cancelSlackLink();
      slackAccountChanged();
    },
    { success: 'Install link cancelled' },
  );
  return (
    <SlackCard
      title={link.label ?? 'Slack'}
      actions={
        <button className="text-button" disabled={cancel.busy} onClick={() => void cancel.run()}>
          Cancel link
        </button>
      }
    >
      <div role="status">
        <Badge value="invited">Pending</Badge>
      </div>
      <dl className="slack-details">
        <dt>Install link</dt>
        <dd>Expires {day(link.expiresAt, timeZone)}</dd>
        {link.createdBy && (
          <>
            <dt>Made by</dt>
            <dd>{link.createdBy}</dd>
          </>
        )}
      </dl>
    </SlackCard>
  );
}

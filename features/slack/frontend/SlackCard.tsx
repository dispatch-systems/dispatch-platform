import type { ReactNode } from 'react';
import { SlackLogo } from './SlackLogo.js';

/**
 * One of Slack's cards on Settings' DSP Connections, under Slack's mark and its `title`, with
 * its actions below.
 */
export function SlackCard({
  title,
  children,
  actions,
}: {
  title: string;
  children: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <article className="archived-connection-card slack-connection-card">
      <header>
        <h3>
          <SlackLogo size={20} />
          {title}
        </h3>
      </header>
      <div className="archived-connection-content">{children}</div>
      {actions && <footer>{actions}</footer>}
    </article>
  );
}

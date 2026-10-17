import { useState } from 'react';
import { ArrowRight, ChevronDown } from 'lucide-react';
import type {
  ListedIntegration,
  SettingsContext,
} from '../../../core/shell/frontend/runtime/slots.js';
import { Badge } from '../../../core/shell/frontend/ui/index.js';
import { IntegrationNotes, IntegrationSteps } from './IntegrationNotes.js';

// One integration through Slack, on its card in a workspace's settings, for members who may not
// know what it is: what it does, how it's used and what Dispatch sees, beside its switch. On, the
// card says what it's doing instead, with how it works a click away, and opens its settings.

export function IntegrationCard({
  integration,
  context,
  on,
  locked,
  settings,
  switchTo,
  open,
}: {
  integration: ListedIntegration;
  /** What its summary is drawn with, for the workspace. */
  context: SettingsContext & { account: string };
  on: boolean;
  /** Whether it can't be switched now, as while the workspace needs reconnecting. */
  locked: boolean;
  /** Its own section of the workspace's settings, if it has one. */
  settings?: { id: string; label: string };
  /** Asks to switch it on or off. */
  switchTo: (on: boolean) => void;
  open: (section: string) => void;
}) {
  const [explained, setExplained] = useState(false);
  const { label, icon: Icon } = integration;
  const byline =
    (context.view && integration.byline?.(context.view)) ?? `Part of ${integration.from}`;
  return (
    <article className="slack-integration" aria-label={label}>
      <header>
        <span className="slack-integration-icon">
          <Icon size={19} aria-hidden="true" />
        </span>
        <div className="slack-integration-name">
          <h3>{label}</h3>
          <span>{byline}</span>
        </div>
        {on && <Badge value="ready">On</Badge>}
        <input
          type="checkbox"
          role="switch"
          aria-label={label}
          checked={on}
          disabled={locked}
          onChange={(event) => switchTo(event.target.checked)}
        />
      </header>
      <p>{integration.description}</p>
      {on && (
        <footer>
          <div className="slack-integration-summary">{integration.summary?.(context)}</div>
          <button
            className="text-button"
            aria-expanded={explained}
            onClick={() => setExplained((shown) => !shown)}
          >
            <ChevronDown size={15} aria-hidden="true" />
            How it works
          </button>
          {settings && (
            <button onClick={() => open(settings.id)}>
              Open {settings.label}
              <ArrowRight size={15} aria-hidden="true" />
            </button>
          )}
        </footer>
      )}
      {(!on || explained) && (
        <div className="slack-integration-details">
          <section>
            <h4>How it works</h4>
            <IntegrationSteps steps={integration.steps} />
          </section>
          <section>
            <h4>What Dispatch sees</h4>
            <IntegrationNotes notes={integration.sees} />
          </section>
        </div>
      )}
    </article>
  );
}

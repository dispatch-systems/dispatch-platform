import { Fragment } from 'react';
import { ShieldCheck } from 'lucide-react';
import type { Connection } from '../api/index.js';
import {
  connectionCard,
  type ConnectionPiece,
  type SettingsContext,
} from '../../shell/frontend/runtime/slots.js';

/**
 * A DSP's Connections: its own accounts, for those who manage them, which are the collectors'
 * connections and any a feature adds; then the member's own accounts, that features add.
 */
export function ConnectionsPage({
  development,
  timezone,
  providers,
  dsp,
  personal,
  context,
}: {
  development: boolean;
  timezone: string;
  /** The collectors' connections the member manages, in the order shown. */
  providers: readonly Connection['provider'][];
  /** The DSP's accounts features add, after the collectors'. */
  dsp: readonly ConnectionPiece[];
  /** The member's own accounts. */
  personal: readonly ConnectionPiece[];
  context: SettingsContext;
}) {
  return (
    <>
      {providers.length + dsp.length > 0 && (
        <section className="connections-view" aria-labelledby="dsp-connections-heading">
          <header>
            <h2 id="dsp-connections-heading">DSP Connections</h2>
            <p className="muted">Accounts Dispatch uses for the whole DSP.</p>
          </header>
          <div className="connection-cards">
            {providers.map((provider) => (
              <Fragment key={provider}>
                {connectionCard(provider)?.render({ development, timezone })}
              </Fragment>
            ))}
            {dsp.map((piece) => (
              <Fragment key={piece.id}>{piece.render(context)}</Fragment>
            ))}
          </div>
          <p className="connection-permissions muted">
            <ShieldCheck size={16} />
            People with Manage DSP Connections can change these.
          </p>
        </section>
      )}
      {personal.length > 0 && (
        <section className="connections-view" aria-labelledby="personal-connections-heading">
          <header>
            <h2 id="personal-connections-heading">Personal Connections</h2>
            <p className="muted">Your own accounts. Only you can see and change these.</p>
          </header>
          <div className="connection-cards">
            {personal.map((piece) => (
              <Fragment key={piece.id}>{piece.render(context)}</Fragment>
            ))}
          </div>
        </section>
      )}
    </>
  );
}

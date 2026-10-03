import { Fragment } from 'react';
import { ShieldCheck } from 'lucide-react';
import type { Connection } from '../../../shared/contracts/index.js';
import { connectionCard } from '../../shell/frontend/runtime/slots.js';

export function ConnectionsPage({
  development,
  timezone,
  providers,
}: {
  development: boolean;
  timezone: string;
  /** The connections the DSP has, in the order shown. */
  providers: readonly Connection['provider'][];
}) {
  return (
    <section className="connections-view" aria-labelledby="connections-heading">
      <div>
        <h2 id="connections-heading">Connections</h2>
      </div>
      <div className="connection-cards">
        {providers.map((provider) => (
          <Fragment key={provider}>
            {connectionCard(provider)?.render({ development, timezone })}
          </Fragment>
        ))}
      </div>
      <p className="connection-permissions muted">
        <ShieldCheck size={16} />
        DSP owners and platform owners can manage these credentials.
      </p>
    </section>
  );
}

import { useMemo, useSyncExternalStore } from 'react';
import { Wrench } from 'lucide-react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import { widgets } from '../../../core/shell/frontend/runtime/slots.js';
import { WidgetHome } from './WidgetHome.js';

/** As narrow as a window whose sidebar is a drawer, where Home has no grid yet. */
const NARROW = '(width <= 980px)';
const narrow = () => matchMedia(NARROW).matches;
const listen = (changed: () => void) => {
  const query = matchMedia(NARROW);
  query.addEventListener('change', changed);
  return () => query.removeEventListener('change', changed);
};

/** A DSP's home page: the member's widgets, on a window wide enough for its grid. */
export function HomePage({ session, view }: { session: SessionView; view: DspView }) {
  const small = useSyncExternalStore(listen, narrow);
  const available = useMemo(() => widgets({ session, view }), [session, view]);
  if (small || !available.length) return <UnderDevelopment />;
  return <WidgetHome session={session} view={view} available={available} />;
}

function UnderDevelopment() {
  return (
    <section className="dsp-home" aria-labelledby="dsp-home-heading">
      <div className="dsp-home-notice">
        <div className="dsp-home-icon" aria-hidden="true">
          <Wrench />
        </div>
        <h1 id="dsp-home-heading">Currently under development</h1>
        <p>We’re building your DSP home page.</p>
      </div>
    </section>
  );
}

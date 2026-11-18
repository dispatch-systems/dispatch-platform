import { useMemo, useSyncExternalStore } from 'react';
import { Wrench } from 'lucide-react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import { useHashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import { widgets } from '../../../core/shell/frontend/runtime/slots.js';
import { widgetOf } from './layout.js';
import { WidgetHome } from './WidgetHome.js';
import { WidgetSizes } from './WidgetSizes.js';

/** As narrow as a window whose sidebar is a drawer, where Home has no grid yet. */
const NARROW = '(width <= 980px)';
const narrow = () => matchMedia(NARROW).matches;
const listen = (changed: () => void) => {
  const query = matchMedia(NARROW);
  query.addEventListener('change', changed);
  return () => query.removeEventListener('change', changed);
};

/** The widget `?sizes=` shows at every size, and the grid of them shown, from 0. */
const readSizes = (query: URLSearchParams) => ({
  id: query.get('sizes'),
  page: Math.max(0, Math.trunc(Number(query.get('page') ?? 1)) - 1 || 0),
});

/**
 * A DSP's home page: the member's widgets, on a window wide enough for its grid; or, away from
 * Production, one of them at every size it declares, as `?sizes=<id>` asks.
 */
export function HomePage({ session, view }: { session: SessionView; view: DspView }) {
  const small = useSyncExternalStore(listen, narrow);
  const available = useMemo(() => widgets({ session, view }), [session, view]);
  const [sizes, setSizes] = useHashQuery(readSizes);
  const sized =
    session.environment !== 'production' && sizes.id ? widgetOf(available, sizes.id) : undefined;
  if (small || !available.length) return <UnderDevelopment />;
  if (sized)
    return (
      <WidgetSizes
        widget={sized}
        view={view}
        page={sizes.page}
        turn={(page) => setSizes({ id: sized.id, page })}
      />
    );
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

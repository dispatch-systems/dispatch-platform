import {
  Component,
  Suspense,
  lazy,
  type ComponentType,
  type LazyExoticComponent,
  type ReactNode,
} from 'react';
import type { DspView } from '../../../core/accounts/api/index.js';
import type {
  Widget,
  WidgetProps,
  WidgetSize,
} from '../../../core/shell/frontend/runtime/slots.js';

// What draws each widget, loaded once, the first time it's shown.
const drawn = new Map<string, LazyExoticComponent<ComponentType<WidgetProps>>>();
function drawing(widget: Widget) {
  let found = drawn.get(widget.id);
  if (!found) {
    found = lazy(() => widget.load().then((draw) => ({ default: draw })));
    drawn.set(widget.id, found);
  }
  return found;
}

/** A widget in its frame: loading on its own, and failing alone. */
export function WidgetFrame({
  widget,
  size,
  view,
}: {
  widget: Widget;
  size: WidgetSize;
  view: DspView;
}) {
  const Drawn = drawing(widget);
  return (
    <article className="home-widget" aria-label={widget.label}>
      <Failing id={widget.id}>
        <Suspense fallback={<div className="home-widget-loading" />}>
          <Drawn size={size} view={view} />
        </Suspense>
      </Failing>
    </article>
  );
}

/** Keeps a widget that fails from taking the page with it. */
class Failing extends Component<{ id: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(error: unknown) {
    console.error(`The widget ${this.props.id} failed`, error);
  }
  render() {
    return this.state.failed ? (
      <p className="home-widget-failed">This widget couldn’t load.</p>
    ) : (
      this.props.children
    );
  }
}

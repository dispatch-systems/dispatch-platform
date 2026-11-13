import { Component, memo, useEffect, useState, type ComponentType, type ReactNode } from 'react';
import type { DspView } from '../../../core/accounts/api/index.js';
import type { Widget, WidgetProps } from '../../../core/shell/frontend/runtime/slots.js';

// What draws each widget, loaded once, the first time it's shown.
const loaded = new WeakMap<Widget, ComponentType<WidgetProps>>();

/** A widget in its frame: loading on its own, failing alone, and drawn again only as it changes. */
export const WidgetFrame = memo(function WidgetFrame({
  widget,
  columns,
  rows,
  view,
}: {
  widget: Widget;
  columns: number;
  rows: number;
  view: DspView;
}) {
  return (
    <article className="home-widget" aria-label={widget.label}>
      <Failing id={widget.id}>
        <Drawn widget={widget} columns={columns} rows={rows} view={view} />
      </Failing>
    </article>
  );
});

/** The widget once its code has loaded, its frame empty until then. */
function Drawn({ widget, ...drawn }: WidgetProps & { widget: Widget }) {
  const [Loaded, setLoaded] = useState(() => loaded.get(widget));
  const [failed, setFailed] = useState<Error>();
  useEffect(() => {
    if (Loaded) return;
    let live = true;
    widget.load().then(
      (component) => {
        loaded.set(widget, component);
        if (live) setLoaded(() => component);
      },
      (error: unknown) =>
        live && setFailed(error instanceof Error ? error : new Error(String(error))),
    );
    return () => {
      live = false;
    };
  }, [widget, Loaded]);
  // A failed download fails as drawing it would, for the boundary around it.
  if (failed) throw failed;
  return Loaded ? <Loaded {...drawn} /> : <div className="home-widget-loading" />;
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

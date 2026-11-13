import type { DspView } from '../../../core/accounts/api/index.js';
import type { Widget, WidgetSize } from '../../../core/shell/frontend/runtime/slots.js';
import type { PlacedWidget } from '../api/index.js';
import { WidgetFrame } from './WidgetFrame.js';

/** The widgets on the page's grid, each in the cells it was placed in. */
export function WidgetGrid({
  placed,
  view,
}: {
  placed: readonly { widget: Widget; size: WidgetSize; at: PlacedWidget }[];
  view: DspView;
}) {
  return (
    <div className="home-grid">
      {placed.map(({ widget, size, at }) => (
        <div
          key={widget.id}
          className="home-cell"
          style={{
            gridColumn: `${at.x + 1} / span ${at.w}`,
            gridRow: `${at.y + 1} / span ${at.h}`,
          }}
        >
          <WidgetFrame widget={widget} size={size} view={view} />
        </div>
      ))}
    </div>
  );
}

import { useMemo } from 'react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { useFillsWindow } from '../../../core/shell/frontend/runtime/fills.js';
import { replaceHashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import type { Widget } from '../../../core/shell/frontend/runtime/slots.js';
import { Header, Pagination } from '../../../core/shell/frontend/ui/index.js';
import { useFit } from './fit.js';
import { sizePages } from './layout.js';
import { WidgetFrame } from './WidgetFrame.js';

/**
 * One widget at every size it declares, on as few of the page's grids as hold them, a grid at a
 * time: what its author looks at, away from Production, as `?sizes=<id>` opens it.
 */
export function WidgetSizes({
  widget,
  view,
  page,
  turn,
}: {
  widget: Widget;
  view: DspView;
  /** The grid shown, from 0. */
  page: number;
  turn: (page: number) => void;
}) {
  useFillsWindow();
  const { area, fit } = useFit();
  const pages = useMemo(() => sizePages(widget.sizes), [widget]);
  const shown = Math.min(page, pages.length - 1);
  return (
    <section className="home-page">
      <Header title={widget.label}>
        <Pagination
          variant="pages"
          page={shown}
          pageSize={1}
          total={pages.length}
          onChange={(next) => {
            replaceHashQuery({ sizes: widget.id, page: String(next + 1) });
            turn(next);
          }}
        />
      </Header>
      <div ref={area} className="home-area">
        <div className="home-grid" style={fit}>
          {pages[shown]!.map((at) => (
            <div
              // Each widget's own frames: another's never shows under its heading.
              key={`${widget.id}:${at.w}x${at.h}`}
              className="home-cell"
              data-columns={at.w}
              data-rows={at.h}
              style={{
                gridColumn: `${at.x + 1} / span ${at.w}`,
                gridRow: `${at.y + 1} / span ${at.h}`,
              }}
            >
              <WidgetFrame widget={widget} columns={at.w} rows={at.h} view={view} />
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

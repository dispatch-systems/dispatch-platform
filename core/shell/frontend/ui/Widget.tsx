import { CircleCheck, type LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';
import { dateFormatter } from '../lib/date-format.js';
import './widget.css';

// What a widget on a member's Home page is made of, so each looks and fits its size as the others
// do: a link to its own page filling its cell, its header, what it says while it can't show
// anything, a count, a day with none, a list of as many rows as fit, and its footer. A widget
// lays these out by its size, as `widgetFit` names the steps every widget keeps.

/**
 * What a widget shows at its size, as every widget does: `narrow`, one column, a count with its
 * names under it once `tall`, two rows or more, which also has room for a footer; a row's second
 * line and initials in `detail`, three columns by two; and more at a row's end once `wide`, four
 * columns.
 */
export const widgetFit = ({ columns, rows }: { columns: number; rows: number }) => ({
  narrow: columns === 1,
  tall: rows > 1,
  detail: columns >= 3 && rows >= 2,
  wide: columns >= 4,
});

/** A day, `YYYY-MM-DD`, as a widget's header shows it: Nov 18, with its weekday from three columns. */
export const widgetDay = (day: string, weekday = false) =>
  dateFormatter('en-US', {
    timeZone: 'UTC',
    month: 'short',
    day: 'numeric',
    ...(weekday ? { weekday: 'short' } : {}),
  }).format(new Date(day + 'T12:00:00Z'));

/** A name's initials, from its first two words. */
export const initials = (name: string) =>
  name
    .split(' ')
    .slice(0, 2)
    .map((word) => word[0])
    .join('');

/**
 * A widget: a link to what it shows on its owner's page, filling its cell, under its header of
 * its icon, its label (`short` in one column) and the `day` it shows; with no `href`, a widget
 * that opens nothing.
 */
export function WidgetLink({
  href,
  columns,
  rows,
  icon: Icon,
  label,
  short,
  day,
  className,
  children,
}: {
  href?: string;
  columns: number;
  rows: number;
  icon: LucideIcon;
  label: string;
  /** Its label in one column, where its day leaves room for a word or so. */
  short?: string;
  day?: string;
  /** Its own class, for what its own CSS draws. */
  className?: string;
  children?: ReactNode;
}) {
  const Frame = href ? 'a' : 'div';
  return (
    <Frame
      className={className ? `widget ${className}` : 'widget'}
      href={href}
      data-columns={columns}
      data-rows={rows}
    >
      <header>
        <Icon aria-hidden="true" />
        <strong>{columns === 1 && short ? short : label}</strong>
        {day && <span>{widgetDay(day, columns >= 3)}</span>}
      </header>
      {children}
    </Frame>
  );
}

/** What a widget says in place of what it shows: why it can't, or that there's nothing yet. */
export const WidgetNote = ({ children }: { children: ReactNode }) => (
  <p className="widget-note">{children}</p>
);

/** A count, large, over what it counts. */
export const WidgetCount = ({ count, label }: { count: number; label: string }) => (
  <div className="widget-count">
    <strong>{count}</strong>
    <span>{label}</span>
  </div>
);

/** None to show, as good news: a check and its 0, with a line under it. */
export const WidgetZero = ({ label, children }: { label: string; children?: ReactNode }) => (
  <div className="widget-zero">
    <CircleCheck aria-hidden="true" />
    <span>
      <strong>0</strong> {label}
    </span>
    {children && <small>{children}</small>}
  </div>
);

/** Its rows, as many as fit whole, in order: one that doesn't fit wraps out of sight. */
export const WidgetList = ({ children }: { children: ReactNode }) => (
  <ol className="widget-list">{children}</ol>
);

/** How a row is marked at its edge: well, a warning, or bad. */
export type WidgetTone = 'good' | 'warn' | 'bad';

/**
 * A row: its `title`, with a `badge` beside it and its `detail` under it, after its `initials`,
 * with what `children` adds at its end, as a `WidgetValue`, `WidgetPill` or `WidgetBar`.
 */
export function WidgetRow({
  title,
  badge,
  detail,
  initials,
  tone = 'warn',
  children,
}: {
  title: ReactNode;
  badge?: ReactNode;
  detail?: ReactNode;
  initials?: string;
  tone?: WidgetTone;
  children?: ReactNode;
}) {
  return (
    <li data-tone={tone}>
      {initials && (
        <span className="widget-initials" aria-hidden="true">
          {initials}
        </span>
      )}
      <span className="widget-row-main">
        <strong>
          <span className="widget-title">{title}</span>
          {badge && <span className="widget-badge">{badge}</span>}
        </strong>
        {detail && <small>{detail}</small>}
      </span>
      {children}
    </li>
  );
}

/** A row's value at its end, bold, with what it's out of after it. */
export const WidgetValue = ({ children, of }: { children: ReactNode; of?: ReactNode }) => (
  <span className="widget-value">
    {children}
    {of && <small> {of}</small>}
  </span>
);

/** A short mark on a row, in its tone. */
export const WidgetPill = ({ children }: { children: ReactNode }) => (
  <span className="widget-pill">{children}</span>
);

/** How much of a whole a row is, 0 to 1, as a bar in its tone. */
export const WidgetBar = ({ share }: { share: number }) => (
  <span className="widget-bar" aria-hidden="true">
    <span style={{ width: `${Math.max(0, Math.min(1, share)) * 100}%` }} />
  </span>
);

/** The line at a widget's foot, from two rows. */
export const WidgetFooter = ({ children }: { children: ReactNode }) => <footer>{children}</footer>;

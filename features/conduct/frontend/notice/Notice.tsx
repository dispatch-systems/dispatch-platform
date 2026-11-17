import { Fragment, useLayoutEffect, useRef, type CSSProperties, type ReactNode } from 'react';
import type { ConductFinalKind, ConductTier } from '../../api/index.js';
import { blank, finalNote, tierName, tiers } from '../tiers.js';
import * as wording from './wording.js';

/** What a notice prints: a writeup, saved or being written. */
export type NoticeContent = {
  personName: string;
  supervisorName: string;
  writtenOn: string;
  tier: ConductTier;
  finalKind: ConductFinalKind | null;
  suspension: string | null;
  statement: string;
  prior: string;
  plan: string;
};

// The notice is laid out as the DSP's own form is: every place below is in points from the
// page's top left (a letter page is 612 × 792), measured off that form, so the notice prints on
// one page exactly as it does.

/** Where something sits on the page: its top, its left, and how wide it may run. */
const at = (top: number, left: number, width?: number): CSSProperties => ({
  top: `${top}pt`,
  left: `${left}pt`,
  ...(width === undefined ? {} : { width: `${width}pt` }),
});

/** A ruled line, from `left` to `right`. */
const Rule = ({ top, left, right }: { top: number; left: number; right: number }) => (
  <span className="conduct-notice-rule" style={at(top, left, right - left)} />
);

/** Text placed on the page in the form's own lettering. */
const Text = ({
  top,
  left,
  width,
  className,
  children,
}: {
  top: number;
  left: number;
  width?: number;
  className?: string;
  children: ReactNode;
}) => (
  <span className={`conduct-notice-text ${className ?? ''}`} style={at(top, left, width)}>
    {children}
  </span>
);

/** Each tier's box and name, its top. */
const tierRows = [107.7, 120.5, 133.2];
/** Each answered section: its number's top, and where its four ruled lines are below it. */
const sections = [206.1, 306.5, 406.8] as const;
const lines = [30.4, 49.2, 68.0, 86.8];
/** The smallest lettering shrinks to, as a share of its size: small enough that a section of
 * the most a writeup holds, even a line for each of its characters, still fits its four lines. */
const smallest = 0.001;

/**
 * Shrinks `element`'s lettering (`--fit`, a share of its size) to the largest that fits its box,
 * across (`width`) or down, and marks it `data-overflow` if even the smallest doesn't.
 */
function fit(element: HTMLElement, across: boolean) {
  if (!element.clientWidth || !element.clientHeight) return;
  const fits = () =>
    across
      ? element.scrollWidth <= element.clientWidth + 1
      : element.scrollHeight <= element.clientHeight + 1;
  const scale = (share: number) => element.style.setProperty('--fit', String(share));
  scale(1);
  if (!fits()) {
    let [low, high] = [smallest, 1];
    for (let step = 0; step < 20; step++) {
      const middle = (low + high) / 2;
      scale(middle);
      if (fits()) low = middle;
      else high = middle;
    }
    scale(low);
  }
  element.toggleAttribute('data-overflow', !fits());
  const base = element.classList.contains('conduct-notice-answer')
    ? 10.3
    : element.classList.contains('conduct-notice-signature')
      ? 22.4
      : 12;
  element.toggleAttribute(
    'data-unreadable',
    Number(element.style.getPropertyValue('--fit')) * base < 7,
  );
}

/** Printing fits synchronously after fonts load, independently of queued preview work. */
export function fitNotice(notice: HTMLElement) {
  for (const element of notice.querySelectorAll<HTMLElement>('[data-conduct-fit]'))
    fit(element, element.dataset.conductFit === 'across');
}

const pending = new Map<HTMLElement, boolean>();
let frame: number | undefined;
function schedule(element: HTMLElement, across: boolean) {
  pending.set(element, across);
  frame ??= requestAnimationFrame(() => {
    frame = undefined;
    for (const [element, across] of pending) if (element.isConnected) fit(element, across);
    pending.clear();
  });
}
function useFit<T extends HTMLElement>(across: boolean, shown: string) {
  const ref = useRef<T>(null);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    let active = true;
    schedule(element, across);
    if (document.fonts.status !== 'loaded')
      void document.fonts.ready.then(() => {
        if (active) schedule(element, across);
      });
    return () => {
      active = false;
      pending.delete(element);
    };
  }, [across, shown]);
  return ref;
}

/** A name or date written on its line, shrinking to fit the line's width when it's longer. */
function Filled({
  top,
  left,
  width,
  className = 'conduct-notice-filled',
  children,
}: {
  top: number;
  left: number;
  width: number;
  className?: string;
  children: string;
}) {
  const ref = useFit<HTMLSpanElement>(true, children);
  return (
    <span
      ref={ref}
      data-conduct-fit="across"
      className={`conduct-notice-text ${className}`}
      style={at(top, left, width)}
    >
      {children}
    </span>
  );
}

/** "Casey Supervisor" as the form's top line names a supervisor: "Casey S.". */
const shortName = (name: string) => {
  const parts = name.trim().split(/\s+/);
  const last = parts.length > 1 ? parts.at(-1) : undefined;
  return last ? `${parts[0]} ${last[0]}.` : name.trim();
};

/** A day, YYYY-MM-DD, as the form writes it: 08/23/2026. */
const formDay = (day: string) => {
  const [year, month, date] = day.split('-');
  return `${month}/${date}/${year}`;
};

/**
 * A section's words on its four lines, shrinking to fit them when they run longer, so the
 * notice never runs onto a second page. Each blank still to fill is marked while it's being
 * written.
 */
function Answer({ top, text, marked }: { top: number; text: string; marked: boolean }) {
  const box = useFit<HTMLDivElement>(false, `${marked}:${text}`);
  const parts = text.split(blank);
  const blanks = marked ? (text.match(blank) ?? []) : [];
  return (
    <div
      ref={box}
      data-conduct-fit="down"
      className="conduct-notice-answer"
      style={at(top + 14.6, 92, 448)}
    >
      {marked
        ? parts.map((part, index) => (
            <Fragment key={index}>
              {part}
              {blanks[index] && <mark>{blanks[index]}</mark>}
            </Fragment>
          ))
        : text}
    </div>
  );
}

/**
 * The Corrective Action Notice as it prints, on a letter page: the driver signs it there.
 * `marked` marks the blanks left to fill, on screen while it's being written.
 */
export function Notice({ content, marked = false }: { content: NoticeContent; marked?: boolean }) {
  const note = content.tier === 'final' ? finalNote(content.finalKind, content.suspension) : '';
  const day = formDay(content.writtenOn);
  const answers = [content.statement, content.prior, content.plan];
  const headings = [wording.sections.statement, wording.sections.prior, wording.sections.plan];
  return (
    <article className="conduct-notice" aria-label={wording.title}>
      <h2 className="conduct-notice-text conduct-notice-bold" style={at(43.7, 0, 612)}>
        {wording.title}
      </h2>

      <Text top={70.3} left={72.5} className="conduct-notice-bold">
        Employee:
      </Text>
      <Rule top={81.8} left={120} right={259.9} />
      <Filled top={70.1} left={125.9} width={133}>
        {content.personName}
      </Filled>
      <Text top={70.3} left={288.2} className="conduct-notice-bold">
        Supervisor:
      </Text>
      <Rule top={81.8} left={340.6} right={425.9} />
      <Filled top={70.1} left={345.6} width={79}>
        {shortName(content.supervisorName)}
      </Filled>
      <Text top={70.3} left={432.5} className="conduct-notice-bold">
        Date:
      </Text>
      <Rule top={81.8} left={455} right={517.3} />
      <Filled top={70.1} left={458.5} width={58}>
        {day}
      </Filled>

      <Text top={95} left={72.5} className="conduct-notice-bold">
        {wording.typeHeading}
      </Text>
      {tiers.map((tier, index) => (
        <Fragment key={tier}>
          <span className="conduct-notice-box" style={at(tierRows[index]! + 2.6, 73.6)}>
            {content.tier === tier ? 'X' : ''}
          </span>
          <Text top={tierRows[index]!} left={84.7}>
            {tierName[tier]}
          </Text>
          {tier === 'final' && note && (
            <Text top={tierRows[index]! + 3.4} left={176} className="conduct-notice-note">
              {note}
            </Text>
          )}
        </Fragment>
      ))}

      {wording.disclaimer.map((line, index) => (
        <Text
          key={line}
          top={[159.5, 171, 182.6][index]!}
          left={72.5}
          className="conduct-notice-fine"
        >
          {index === 0 && <b>* </b>}
          {line}
        </Text>
      ))}

      {sections.map((top, index) => (
        <Fragment key={top}>
          <Text top={top} left={72.5} className="conduct-notice-bold">
            {index + 1}.
          </Text>
          <Text top={top} left={90.5} className="conduct-notice-bold">
            {headings[index]}
          </Text>
          {lines.map((line) => (
            <Rule key={line} top={top + line} left={90.6} right={539.8} />
          ))}
          <Answer top={top} text={answers[index]!} marked={marked} />
        </Fragment>
      ))}

      <Text top={507.1} left={72.5} className="conduct-notice-bold">
        4.
      </Text>
      <Text top={507.1} left={93} className="conduct-notice-bold">
        {wording.sections.consequences}
      </Text>
      {wording.consequences.map((line, index) => (
        <Text key={line} top={[531.8, 544.5][index]!} left={90.5} className="conduct-notice-italic">
          {line}
        </Text>
      ))}

      <Filled top={560.9} left={80.5} width={365} className="conduct-notice-signature">
        {content.supervisorName}
      </Filled>
      <Filled top={568.7} left={458} width={59}>
        {day}
      </Filled>
      <Rule top={580.4} left={72.2} right={518.3} />
      <Text top={581.5} left={72.5}>
        {wording.supervisorSignature}
      </Text>
      <Text top={581.5} left={468.3}>
        Date
      </Text>

      <Text top={612.9} left={72.5} className="conduct-notice-bold">
        {wording.employeeComments}
      </Text>
      {[643.2, 662, 680.7].map((top) => (
        <Rule key={top} top={top} left={72.5} right={539.8} />
      ))}

      <Text top={713.5} left={72.5}>
        {wording.acknowledgement}
      </Text>
      <Rule top={724.8} left={313.3} right={538.9} />
      <Text top={726.2} left={324.5}>
        {wording.employeeSignature}
      </Text>
      <Text top={726.2} left={468.3}>
        Date
      </Text>
    </article>
  );
}

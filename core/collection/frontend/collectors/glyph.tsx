import type { Pattern } from './rules.js';

// A pattern drawn as a tiny day: a track, its runs as pins or ticks, a repeat's hours as a band.

const W = 150;
const H = 40;
const MID = H / 2;
const at = (hour: number, width: number) => (width * hour) / 24;

function Track({ width, y, height }: { width: number; y: number; height: number }) {
  return <rect className="glyph-track" x={0} y={y} width={width} height={height} rx={height / 2} />;
}
/** A repeat's hours, 5 AM to 10 PM, with a tick every `step` hours. */
function Band({
  width,
  y,
  height,
  tick,
  step,
}: {
  width: number;
  y: number;
  height: number;
  tick: number;
  step: number;
}) {
  const ticks = [];
  for (let hour = 5 + step / 2; hour < 22; hour += step) ticks.push(hour);
  return (
    <>
      <rect
        className="glyph-band"
        x={at(5, width)}
        y={y}
        width={at(22, width) - at(5, width)}
        height={height}
        rx={height / 2}
      />
      {ticks.map((hour) => (
        <rect
          key={hour}
          className="glyph-ink"
          x={at(hour, width) - 1}
          y={y + height / 2 - tick / 2}
          width={2}
          height={tick}
          rx={1}
        />
      ))}
    </>
  );
}

/** A pattern's card art. */
export function Glyph({ pattern }: { pattern: Pattern }) {
  const pin = (hour: number) => (
    <g key={hour}>
      <rect
        className="glyph-ink"
        x={at(hour, W) - 1.5}
        y={MID - 12}
        width={3}
        height={19}
        rx={1.5}
      />
      <circle className="glyph-ink" cx={at(hour, W)} cy={MID - 12} r={4.5} />
    </g>
  );
  return (
    <svg className="glyph" viewBox={`0 0 ${W} ${H}`} aria-hidden="true">
      <Track width={W} y={MID - 6} height={12} />
      {pattern === 'once' && pin(9)}
      {pattern === 'few' && [6.5, 12, 17.5].map(pin)}
      {pattern === 'repeat' && <Band width={W} y={MID - 10} height={20} tick={10} step={1.25} />}
    </svg>
  );
}

/** A pattern's small mark, beside a rule's words. */
export function Mark({ pattern }: { pattern: Pattern }) {
  const width = 30;
  const tick = (hour: number) => (
    <rect
      key={hour}
      className="glyph-ink"
      x={at(hour, width) - 1.2}
      y={1}
      width={2.4}
      height={14}
      rx={1.2}
    />
  );
  return (
    <svg className="glyph" viewBox={`0 0 ${width} 16`} width={width} height={16} aria-hidden="true">
      <Track width={width} y={4} height={8} />
      {pattern === 'once' && tick(9)}
      {pattern === 'few' && [6.5, 12, 17.5].map(tick)}
      {pattern === 'repeat' && <Band width={width} y={2} height={12} tick={6} step={3} />}
    </svg>
  );
}

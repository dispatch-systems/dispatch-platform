import { Count } from '../shared/Count.js';
import { Segments } from '../shared/Segments.js';
import type { Section } from '../inbox/sections.js';
import { VIEWS } from './section-views.js';
import './sections.css';

// The switch between the list's sections, at its top while there are two or more, each by its
// mark and label (`section-views.tsx`), with how many wait in it.

export function Sections({
  sections,
  section,
  waiting,
  choose,
}: {
  sections: readonly Section[];
  section: Section;
  /** How many wait in each. */
  waiting: Record<Section, number>;
  choose: (section: Section) => void;
}) {
  return (
    <Segments
      className="sections"
      label="Sections"
      value={section}
      onChange={(value) => choose(value as Section)}
      items={sections.map((each) => [
        each,
        <>
          {VIEWS[each].icon}
          {VIEWS[each].label}
          <Count waiting={waiting[each]} />
        </>,
      ])}
    />
  );
}

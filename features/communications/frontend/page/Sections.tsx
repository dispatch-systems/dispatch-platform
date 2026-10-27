import type { ReactNode } from 'react';
import { Phone, Route } from 'lucide-react';
import { SlackLogo } from '../../../slack/frontend/index.js';
import { Count } from '../shared/Count.js';
import { Segments } from '../shared/Segments.js';
import type { Section } from '../inbox/inbox.js';
import './sections.css';

// The sections of the list, at its top while there are two or more: Today's Routes, the work
// phones of the drivers on a route today; Messaging, the numbers that text the DSP's phones; and
// Slack, its workspaces' direct messages, each with how many wait in it. On a phone, or with all
// three in a narrow list, Today's Routes goes by Routes.

const shown: Record<Section, { label: ReactNode; icon: ReactNode }> = {
  routes: {
    label: (
      <>
        <span className="long">Today's Routes</span>
        <span className="short">Routes</span>
      </>
    ),
    icon: <Route size={14} aria-hidden="true" />,
  },
  messaging: { label: 'Messaging', icon: <Phone size={14} aria-hidden="true" /> },
  slack: { label: 'Slack', icon: <SlackLogo size={14} /> },
};

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
          {shown[each].icon}
          {shown[each].label}
          <Count waiting={waiting[each]} />
        </>,
      ])}
    />
  );
}

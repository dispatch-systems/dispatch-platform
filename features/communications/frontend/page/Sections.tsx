import type { ReactNode } from 'react';
import { Phone, Route } from 'lucide-react';
import { SlackLogo } from '../../../slack/frontend/index.js';
import { Tabs } from '../../../../core/shell/frontend/ui/index.js';
import type { Section } from '../inbox.js';

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
    <div className="sections" data-count={sections.length}>
      <Tabs
        label="Sections"
        value={section}
        onChange={(value) => choose(value as Section)}
        items={sections.map((each) => [
          each,
          <>
            {shown[each].icon}
            {shown[each].label}
            {waiting[each] > 0 && <span className="count">{waiting[each]}</span>}
          </>,
        ])}
      />
    </div>
  );
}

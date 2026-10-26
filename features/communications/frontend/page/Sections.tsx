import type { ReactNode } from 'react';
import { Phone } from 'lucide-react';
import { SlackLogo } from '../../../slack/frontend/index.js';
import { Tabs } from '../../../../core/shell/frontend/ui/index.js';
import type { Section } from '../inbox.js';

// The sections of the list, at its top while there are two or more: Messaging, the numbers that
// text the DSP's phones, and Slack, its workspaces' direct messages, each with how many wait in
// it.

const shown: Record<Section, { label: string; icon: ReactNode }> = {
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
    <div className="sections">
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

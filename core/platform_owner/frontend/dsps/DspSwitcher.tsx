import { useState } from 'react';
import { ChevronsUpDown } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import { Popover } from '../../../shell/frontend/ui/index.js';
import { DspListPane } from './DspListPane.js';

// Beside the open DSP's name: every DSP, searched and grouped by state, to open another.
export function DspSwitcher({
  onSelect,
  ...list
}: {
  dsps: DspSummary[];
  query: string;
  setQuery: (value: string) => void;
  selected: string;
  onSelect: (dsp: DspSummary) => void;
  refresh: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover
      className="dsp-switcher"
      triggerClassName="icon-button"
      panelClassName="dsp-switcher-panel"
      label="Switch DSP"
      trigger={<ChevronsUpDown size={18} aria-hidden="true" />}
      open={open}
      onRequestOpen={() => setOpen(true)}
      onRequestClose={() => setOpen(false)}
    >
      <DspListPane
        {...list}
        onSelect={(dsp) => {
          setOpen(false);
          onSelect(dsp);
        }}
      />
    </Popover>
  );
}

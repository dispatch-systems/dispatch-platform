import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { Sidebar } from '../../frontend/runtime/slots.js';
import { SidebarPanels } from '../../frontend/shell/SidebarPanels.js';

// The sidebar sidebar-panels.spec.ts draws: the app's list, and a stand-in for Settings' sliding
// in for it while the address is Settings'. As in the app, the list shown follows the address:
// its change is drawn in the next render React schedules, not at once as a click's is, so a
// slide ending at that moment is drawn in the same render.

const settings: Sidebar = { id: 'settings', scope: 'dsp', label: 'Settings' };
const under = () => (location.hash === '#settings' ? settings : undefined);

function Panels() {
  const [shown, show] = useState(under);
  useEffect(() => {
    const changed = () => show(under());
    addEventListener('hashchange', changed);
    return () => removeEventListener('hashchange', changed);
  }, []);
  return (
    <SidebarPanels
      shown={shown}
      draw={(panel) =>
        panel ? (
          <a href="#profile">Profile</a>
        ) : (
          <nav aria-label="Primary navigation">
            <a href="#home">Home page</a>
            <a href="#settings">Settings</a>
          </nav>
        )
      }
    />
  );
}

createRoot(document.getElementById('root')!).render(<Panels />);

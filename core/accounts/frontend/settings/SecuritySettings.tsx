import { PasswordPanel } from './PasswordPanel.js';
import { SessionsPanel } from './SessionsPanel.js';
import { MultiFactorPanel } from './MultiFactorPanel.js';
import './security.css';

export function SecuritySettings() {
  return (
    <div className="security-settings">
      <PasswordPanel />
      <MultiFactorPanel />
      <SessionsPanel />
    </div>
  );
}

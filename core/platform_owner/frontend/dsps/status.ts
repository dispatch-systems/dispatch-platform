import type { DspSummary } from '../../../accounts/api/index.js';

/** A DSP as the platform's page lists it: reachable, waiting for its owner, or stopped. */
export type DspState = 'online' | 'invited' | 'disabled';
export const dspStates: DspState[] = ['online', 'invited', 'disabled'];
export const stateLabels: Record<DspState, string> = {
  online: 'Online',
  invited: 'Invitation Sent',
  disabled: 'Disabled',
};
export const dspState = (dsp: DspSummary): DspState =>
  dsp.status === 'suspended' || dsp.status === 'failed'
    ? 'disabled'
    : dsp.ownerStatus === 'active'
      ? 'online'
      : 'invited';
/** Who owns the DSP, or where its owner's invitation stands. */
export const ownerText = (dsp: DspSummary) =>
  dsp.ownerStatus === 'active'
    ? (dsp.ownerEmail ?? 'Not assigned')
    : dsp.ownerStatus === 'invited'
      ? `Invitation sent to ${dsp.ownerEmail}`
      : 'Invitation expired';

import type { DspView } from '../../../../../../core/accounts/api/index.js';
import { PhonesCard } from '../../../../../communications/frontend/texting/phone/PhonesCard.js';
import { hrLinePhones } from '../../api/client.js';

/** HR Line's card on a DSP's Connections tab: its own phones, as Dispatch Phone's card shows
 * Dispatch Phone's, paired, renamed and removed apart from them. */
export function HrLineCard({ view }: { view: DspView }) {
  return <PhonesCard view={view} title="HR Line" phones={hrLinePhones} />;
}

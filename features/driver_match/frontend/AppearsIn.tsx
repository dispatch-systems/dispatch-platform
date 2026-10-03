import type { DriverData } from '../../../shared/contracts/index.js';
import { dataLabels, dataOrder } from './driver-match.js';
import { dataIcons } from './dataIcons.js';

/** The five kinds of collected data, lit where the person appears. */
export function AppearsIn({ data }: { data: DriverData[] }) {
  const label = data.length
    ? `Appears in ${data.map((d) => dataLabels[d]).join(', ')}`
    : 'Appears in no collected data';
  return (
    <span className="driver-appears" role="img" aria-label={label}>
      {dataOrder.map((d) => {
        const Icon = dataIcons[d];
        return (
          <span key={d} className={data.includes(d) ? 'on' : undefined} title={dataLabels[d]}>
            <Icon size={15} aria-hidden="true" />
          </span>
        );
      })}
    </span>
  );
}

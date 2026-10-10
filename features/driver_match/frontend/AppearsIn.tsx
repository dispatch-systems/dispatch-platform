import type { DriverData } from '../api/index.js';
import { dataKinds, dataLabel } from './driver-match.js';
import { DataIcon } from './DataIcon.js';

/** Every kind of collected data, lit where the person appears. */
export function AppearsIn({ data }: { data: DriverData[] }) {
  const label = data.length
    ? `Appears in ${data.map(dataLabel).join(', ')}`
    : 'Appears in no collected data';
  return (
    <span className="driver-appears" role="img" aria-label={label}>
      {dataKinds.map((d) => (
        <span key={d} className={data.includes(d) ? 'on' : undefined} title={dataLabel(d)}>
          <DataIcon kind={d} size={15} />
        </span>
      ))}
    </span>
  );
}

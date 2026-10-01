import { useDriverMatch } from '../../app/endpoints.js';

/** The tab's name, with how many pairs wait for a decision. */
export function DriverMatchTabLabel() {
  const review = useDriverMatch().data?.counts.review ?? 0;
  return (
    <>
      Driver Match
      {review > 0 && (
        <span className="driver-tab-flag" aria-label={`${review} to review`}>
          {review}
        </span>
      )}
    </>
  );
}

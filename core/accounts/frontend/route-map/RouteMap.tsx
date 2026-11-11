import { useEffect, useRef } from 'react';
import { mapUrl } from './map-asset.js';

/** How the route reads: animated while a profile is created, or settled when the link is spent. */
export type RouteState = 'live' | 'cancelled' | 'delivered';

/** The geographic asset is requested only when the desktop map is mounted. */
export function RouteMap({
  desktop,
  prefix,
  route = 'live',
}: {
  desktop: boolean;
  prefix: 'onboarding' | 'member-profile';
  route?: RouteState;
}) {
  const map = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = map.current;
    if (!element) return;
    const layers = element.querySelectorAll('svg');
    const resize = () => {
      const { width, height } = element.getBoundingClientRect();
      if (!width || !height) return;
      const mapWidth = Math.min(width <= 1100 ? 1760 : 1440, (2600 * width) / height);
      const mapHeight = (mapWidth * height) / width;
      const top = Math.max(
        -800,
        Math.min(
          1800 - mapHeight,
          mapHeight > 1000 ? (1000 - mapHeight) / 2 : 600 - mapHeight * 0.68,
        ),
      );
      for (const layer of layers)
        layer.setAttribute('viewBox', `0 ${top} ${mapWidth} ${mapHeight}`);
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(element);
    return () => observer.disconnect();
  }, [desktop]);
  return desktop ? (
    <div ref={map} className={`${prefix}-map`} data-route={route} aria-hidden="true">
      <svg className={`${prefix}-map-layer`} focusable="false">
        <use href={`${mapUrl}#region`} />
      </svg>
      {/* Keep animated paint separate from the detailed, static geographic layer. */}
      <svg className={`${prefix}-map-layer ${prefix}-map-motion`} focusable="false">
        {route === 'live' && (
          <>
            <use className={`${prefix}-departure-pulse`} href={`${mapUrl}#departure-pulse`} />
            <use className={`${prefix}-route-trail`} href={`${mapUrl}#delivery-route`} />
            {/* A round, near-zero-length dash follows the shared route without a JS frame loop. */}
            <g className={`${prefix}-traveler`}>
              <use className={`${prefix}-traveler-halo`} href={`${mapUrl}#delivery-route`} />
              <use className={`${prefix}-traveler-outline`} href={`${mapUrl}#delivery-route`} />
              <use className={`${prefix}-traveler-core`} href={`${mapUrl}#delivery-route`} />
            </g>
            <use className={`${prefix}-destination-pulse`} href={`${mapUrl}#delivery-pulse`} />
            <use
              className={`${prefix}-destination-pulse ${prefix}-destination-echo`}
              href={`${mapUrl}#delivery-pulse`}
            />
          </>
        )}
        {/* An unusable link greys the route out and crosses out its stop. */}
        {route === 'cancelled' && (
          <>
            <use className={`${prefix}-route-cancelled`} href={`${mapUrl}#delivery-route`} />
            <use href={`${mapUrl}#delivery-cancelled`} />
          </>
        )}
        {/* A used link shows the delivery made: the stop keeps its arrival rings. */}
        {route === 'delivered' && (
          <>
            <use className={`${prefix}-delivered`} href={`${mapUrl}#delivery-pulse`} />
            <use
              className={`${prefix}-delivered ${prefix}-delivered-echo`}
              href={`${mapUrl}#delivery-pulse`}
            />
          </>
        )}
        {/* The marker emerges from beneath the station instead of covering its logo. */}
        <use href={`${mapUrl}#delivery-station`} />
      </svg>
    </div>
  ) : null;
}

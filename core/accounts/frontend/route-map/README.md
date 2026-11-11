# Route map

`route-map.svg` is the Ventura County vector artwork shared by DSP onboarding and member profiles, with the Camarillo–Ventura illustrative route and no geographic labels. Source data: OpenFreeMap / OpenMapTiles / [OpenStreetMap contributors](https://www.openstreetmap.org/copyright).

`RouteMap.tsx` frames the geographic and motion layers. Its prefix keeps each screen's classes and stylesheet separate; profiles also select live, cancelled or delivered states. The `region` spans x=0…1800, y=-800…1800. Named route, station and pulse geometry serves the CSS animation layer; a round dash follows the static route without a JavaScript frame loop.

`map-asset.ts` loads once on desktop after invitation lookup. Mobile does not request the map. A failed image never blocks the form. Keep the SVG independent rather than inlining it or replacing it with a bitmap. Vite fingerprints it; the build generates Brotli/gzip representations for cached delivery. Existing browser and artifact tests retain their load, failure and transfer-budget checks.

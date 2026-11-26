# Login vans

Each load of the sign-in page shows one van, picked at random from `../van/vans.ts`.
`login-van-step.glb` and `login-van-edv.glb` are original geometry authored from vehicle
photographs supplied for the Signal design and by the owner. Amazon/Prime marks depict those
reference vehicles; no stock mesh or reference photograph is embedded. Three.js is MIT licensed.

- Edit `tooling/assets/step-van.js` or `tooling/assets/edv.js`, built from the shared parts in
  `tooling/assets/kit.js`; run `npx tsx tooling/assets/export-login-vans.ts` to rebuild every
  van. A new van is one more entry there and in `../van/vans.ts`. Named parts and outlines
  are kept in the source. Authoring code is excluded from the shipped app; visitors load the
  prebuilt, merged model of the van they were shown.
- Rotation, rendering limits, camera and dither size: `../van/settings.ts`.
- Ordered dithering: `../van/ordered-shader.ts`. White ink serves both themes.
- Contour geometry: `../LoginArtwork.tsx`; colors and layout: `../sign-in.css`.
- `login-van-<name>-poster.png` is each van's transparent render, used while loading or without
  WebGL.
- Model and renderer load only above 700px. Keep the JS and CSS breakpoints together.
  Both are fingerprinted and compressed at build time, with immutable caching.
- Rendering is capped at 30 fps / 1.5× DPR / 1.5 million pixels, suspended offscreen or in
  hidden tabs, and fully disposed on sign-in or switching to mobile. No server rendering.

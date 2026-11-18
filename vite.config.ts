import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { privateFeatures } from './tooling/build/private-features.js';
import { backendProxy } from './tooling/dev/backend-paths.js';
import { privateSourceGuard } from './tooling/dev/private-sources.js';
// Standalone Vite defaults; npm run dev supplies its own ports and session proxy.
const api = `http://127.0.0.1:${process.env.DISPATCH_DEV_API_PORT ?? '5180'}`;
export default defineConfig({
  root: 'app/frontend',
  base: './',
  plugins: [react(), privateFeatures(process.cwd()), privateSourceGuard(process.cwd(), api)],
  build: {
    outDir: '../../.build/dashboard',
    emptyOutDir: true,
    sourcemap: false,
    // Every library the dashboard bundles, with its license's text, served beside them as the
    // font's is (INTER-LICENSE.txt), as their licenses ask copies to carry.
    license: { fileName: 'assets/THIRD-PARTY-LICENSES.txt' },
    rolldownOptions: {
      output: {},
      treeshake: {
        // The shared `ui` modules run nothing when imported. Saying so keeps what a page
        // doesn't use behind the `ui` index out of its chunks, as before Rolldown: the
        // routes that render a table, a date field or a dialog load it, and its styles,
        // with them.
        moduleSideEffects: [
          {
            test: /[\\/]core[\\/]shell[\\/]frontend[\\/]ui[\\/][^\\/]+\.tsx?$/,
            sideEffects: false,
          },
        ],
      },
    },
  },
  server: {
    host: '127.0.0.1',
    port: Number(process.env.DISPATCH_DEV_PORT || 0),
    strictPort: true,
    proxy: backendProxy({ target: api, changeOrigin: false }),
  },
});

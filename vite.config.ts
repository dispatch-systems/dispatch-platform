import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
// Standalone Vite defaults; npm run dev supplies its own ports and session proxy.
const api = `http://127.0.0.1:${process.env.DISPATCH_DEV_API_PORT ?? '5180'}`;
export default defineConfig({
  root: 'app/frontend',
  base: './',
  plugins: [react()],
  build: {
    outDir: '../../.build/dashboard',
    emptyOutDir: true,
    sourcemap: false,
    rolldownOptions: {
      output: {
        // An owner's lazily loaded entry is `<owner>/frontend/index.ts`: name its chunk after
        // the owner rather than the `frontend` folder, so chunks stay readable.
        chunkFileNames: (chunk) => {
          const owner = /[\\/]([^\\/]+)[\\/]frontend[\\/]index\.tsx?$/.exec(
            chunk.facadeModuleId ?? '',
          )?.[1];
          return `assets/${chunk.name === 'frontend' && owner ? owner : '[name]'}-[hash].js`;
        },
      },
      treeshake: {
        // The table modules run nothing when imported. Saying so lets the unused table
        // code behind the shared `ui` index leave the entry chunk, as it did before
        // Rolldown; the routes that render tables load it with them.
        moduleSideEffects: [
          {
            test: /[\\/]core[\\/]shell[\\/]frontend[\\/]ui[\\/](useDataTable|tableCsv|DataTable)\.tsx?$/,
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
    proxy: { '/api': { target: api, changeOrigin: false } },
  },
});

import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// `npm run dev` proxies /api to the server DOCKET_SERVER names, so the page and the API share an origin.
const server = process.env.DOCKET_SERVER ?? 'http://docket.localhost';

export default defineConfig({
  base: '/ui/',
  plugins: [svelte()],
  server: {
    port: 5190,
    proxy: {
      '/api': {
        target: server,
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
  build: { target: 'es2022' },
  test: { include: ['src/**/*.test.ts'] },
});

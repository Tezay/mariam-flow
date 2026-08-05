import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig(({ mode }) => ({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    // In development the dashboard runs on Vite and the daemon on 8080; the
    // proxy makes both look like one origin, so cookies behave exactly as
    // they will once the daemon serves the built files itself.
    proxy: {
      '/api': 'http://127.0.0.1:8080',
      '/health': 'http://127.0.0.1:8080',
    },
  },
  test: {
    include: ['src/**/*.test.ts'],
    // Node by default: the logic suites need no DOM and start faster without
    // one. Component files opt in with a `@vitest-environment` docblock.
    environment: 'node',
    setupFiles: ['src/lib/tests/setup.ts'],
  },
  // Under test, Vite resolves Svelte's server build unless told otherwise,
  // and its lifecycle functions throw the moment a component is mounted.
  resolve: {
    conditions: mode === 'test' ? ['browser'] : undefined,
  },
}));

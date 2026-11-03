import { reactRouter } from '@react-router/dev/vite';
import tailwindcss from '@tailwindcss/vite';
import { readFileSync } from 'node:fs';
import { defineConfig } from 'vite';

const cargoToml = readFileSync(new URL('../../Cargo.toml', import.meta.url), 'utf8');
const version = cargoToml.match(/^version = "(.+)"$/m)?.[1] ?? 'dev';

export default defineConfig({
  clearScreen: false,
  define: {
    __DAROYAN_VERSION__: JSON.stringify(version),
  },
  resolve: {
    tsconfigPaths: true,
  },
  server: {
    proxy: {
      '/v1': process.env.API_URL ?? 'http://127.0.0.1:8910',
      '/p/': process.env.API_URL ?? 'http://127.0.0.1:8910',
      '/mcp': process.env.API_URL ?? 'http://127.0.0.1:8910',
    },
  },
  build: {
    assetsInlineLimit: (file) => (file.endsWith('.woff2') ? false : undefined),
  },
  plugins: [tailwindcss(), reactRouter()],
});

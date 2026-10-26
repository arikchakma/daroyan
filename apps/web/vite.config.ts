import { reactRouter } from '@react-router/dev/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  clearScreen: false,
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
  plugins: [tailwindcss(), reactRouter()],
});

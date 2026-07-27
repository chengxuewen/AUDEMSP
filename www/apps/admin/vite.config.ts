import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    host: '0.0.0.0',  // allow external access
    port: 5173,
    proxy: {
      '/api': 'http://localhost:9802',
      '/ws': { target: 'ws://localhost:9802', ws: true },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
});

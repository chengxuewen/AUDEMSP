import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    host: '0.0.0.0',  // allow external access
    port: 5173,
    proxy: {
      // PIT-106 (I2 review): events WS (/api/admin/events) 此前缺 ws:true → 5173 下
      // 永远卡 CONNECTING 并堆积半开 socket 拖垮 proxy（/ws 视频也受影响）
      '/api': { target: 'http://127.0.0.1:9800', changeOrigin: true, ws: true },
      '/ws': { target: 'ws://127.0.0.1:9800', ws: true, changeOrigin: true },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
});

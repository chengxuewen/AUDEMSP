import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// 品牌化: VITE_APP_TITLE env 注入 admin 标题（缺省 "MediaServo Admin" 保持现状）。
const APP_TITLE = process.env.VITE_APP_TITLE || 'MediaServo Admin';

export default defineConfig({
  plugins: [react()],
  define: { __APP_TITLE__: JSON.stringify(APP_TITLE) },
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

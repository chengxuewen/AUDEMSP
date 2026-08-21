import React from 'react';
import ReactDOM from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import App from './App';
import './App.css';

// When served at /admin, set basename for proper routing
const basename = window.location.pathname.startsWith('/admin') ? '/admin' : '/';

// 品牌化标题（vite define 编译期注入——运行时改 env 无效，需 pnpm build + server 重建，C24）
declare const __APP_TITLE__: string;
document.title = __APP_TITLE__;

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <BrowserRouter basename={basename}>
      <App />
    </BrowserRouter>
  </React.StrictMode>,
);

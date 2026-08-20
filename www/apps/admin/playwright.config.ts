import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 30000,
  globalSetup: './e2e/global-setup.ts',
  projects: [
    // 管理端套件（admin token）: 既有 dashboard/settings/sfu 视频 + H3 admin 视图
    {
      name: 'admin',
      testIgnore: /roles-(dispatcher|operator)\.spec\.ts|login\.spec\.ts/,
      use: { baseURL: 'http://localhost:5173', storageState: '.auth/admin.json' },
    },
    // dispatcher 套件（H3 角色感知只读视图）
    {
      name: 'dispatcher',
      testMatch: /roles-dispatcher\.spec\.ts/,
      use: { baseURL: 'http://localhost:5173', storageState: '.auth/dispatcher.json' },
    },
    // operator 套件（I1 review: 无监控导航 + admin REST 拒绝）
    {
      name: 'operator',
      testMatch: /roles-operator\.spec\.ts/,
      use: { baseURL: 'http://localhost:5173', storageState: '.auth/operator.json' },
    },
    // 匿名套件（无 storageState）: 登录页 + 路由守卫 + 401 自愈
    {
      name: 'anon',
      testMatch: /login\.spec\.ts/,
      use: { baseURL: 'http://localhost:5173' },
    },
  ],
});

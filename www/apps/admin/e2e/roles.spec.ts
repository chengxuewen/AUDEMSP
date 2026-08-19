import { test, expect } from '@playwright/test';

// H3: 角色感知 UI e2e — 需要 Docker server (9800) + vite dev (5173)。
// 前置: config/accounts.docker.yaml 含 dev admin/dispatcher 账号。

const ADMIN = { username: 'admin', password: 'admin123' };
const DISPATCHER = { username: 'dispatcher', password: 'dispatch123' };

async function login(page: import('@playwright/test').Page, creds: { username: string; password: string }) {
  await page.goto('/settings');
  await page.locator('input[placeholder="username"]').fill(creds.username);
  await page.locator('input[placeholder="password"]').fill(creds.password);
  await page.locator('button', { hasText: 'Login' }).click();
  await expect(page.locator('.token-status.saved')).toContainText(`Signed in as ${creds.username}`);
}

test.describe('H3 role-aware views', () => {
  test('admin login → audio + vehicles nav visible, header shows role', async ({ page }) => {
    await login(page, ADMIN);
    // header 显示账号 + 角色
    await expect(page.locator('.header .version')).toContainText('admin [admin]');
    // can_monitor 导航
    await expect(page.locator('nav a', { hasText: 'Audio Conference' })).toBeVisible();
    await expect(page.locator('nav a', { hasText: 'Vehicles' })).toBeVisible();
  });

  test('dispatcher login → audio + vehicles visible, admin-only REST denied', async ({ page }) => {
    await login(page, DISPATCHER);
    await expect(page.locator('.header .version')).toContainText('dispatcher [dispatcher]');
    await expect(page.locator('nav a', { hasText: 'Audio Conference' })).toBeVisible();
    await expect(page.locator('nav a', { hasText: 'Vehicles' })).toBeVisible();

    // 服务端只读强制（前端已隐藏 Close/配置 UI; 直接打 REST 断言 401）
    const token = await page.evaluate(() => localStorage.getItem('mediaservo_admin_token'));
    const configStatus = await page.evaluate(async (tok) => {
      const res = await fetch('/api/admin/config', { headers: { Authorization: `Bearer ${tok}` } });
      return res.status;
    }, token);
    expect(configStatus).toBe(401);
    const deleteStatus = await page.evaluate(async (tok) => {
      const res = await fetch('/api/admin/rooms/nope', { method: 'DELETE', headers: { Authorization: `Bearer ${tok}` } });
      return res.status;
    }, token);
    expect(deleteStatus).toBe(401);
    // 只读端点放行
    const statusStatus = await page.evaluate(async (tok) => {
      const res = await fetch('/api/admin/status', { headers: { Authorization: `Bearer ${tok}` } });
      return res.status;
    }, token);
    expect(statusStatus).toBe(200);
  });

  test('admin token keeps full access', async ({ page }) => {
    await login(page, ADMIN);
    const token = await page.evaluate(() => localStorage.getItem('mediaservo_admin_token'));
    const deleteStatus = await page.evaluate(async (tok) => {
      const res = await fetch('/api/admin/rooms/nope', { method: 'DELETE', headers: { Authorization: `Bearer ${tok}` } });
      return res.status;
    }, token);
    expect(deleteStatus).toBe(404); // 只读拦截不存在 → 走到房间查找
  });
});

test.describe('H3 audio + vehicles pages', () => {
  test('audio panel renders (rooms or empty state)', async ({ page }) => {
    await login(page, DISPATCHER);
    await page.locator('nav a', { hasText: 'Audio Conference' }).click();
    await expect(page.locator('h2', { hasText: 'Audio Conference Rooms' })).toBeVisible();
    // 有房间 → 房间卡; 无房间 → empty 文案（live 服务器可能两者之一）
    const hasRooms = await page.locator('.audio-room').count();
    if (hasRooms === 0) {
      await expect(page.locator('.empty')).toContainText('No active audio rooms');
    }
  });

  test('vehicles view renders (reports or empty state)', async ({ page }) => {
    await login(page, ADMIN);
    await page.locator('nav a', { hasText: 'Vehicles' }).click();
    await expect(page.locator('h2', { hasText: 'Vehicle Status' })).toBeVisible();
    const hasVehicles = await page.locator('.vehicle-card').count();
    if (hasVehicles === 0) {
      await expect(page.locator('.empty')).toContainText('No vehicle status reports');
    }
  });
});

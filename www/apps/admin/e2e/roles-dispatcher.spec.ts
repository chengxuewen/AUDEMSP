import { test, expect } from '@playwright/test';

// H3: dispatcher 角色只读视图（storageState 已登录 dispatcher — global-setup）。
// 断言: 音频/车辆可见 + admin-only REST 拒绝（服务端强制）+ 音频面板渲染。

test.describe('H3 dispatcher role-aware views', () => {
  test('dispatcher sees audio + vehicles nav, header shows role', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('.header .version')).toContainText('dispatcher [dispatcher]');
    await expect(page.locator('nav a', { hasText: 'Audio Conference' })).toBeVisible();
    await expect(page.locator('nav a', { hasText: 'Vehicles' })).toBeVisible();
  });

  test('dispatcher admin-only REST denied, read-only allowed', async ({ page }) => {
    await page.goto('/');
    const token = await page.evaluate(() => localStorage.getItem('mediaservo_admin_token'));
    // config = admin 专属
    const configStatus = await page.evaluate(async (tok) => {
      const res = await fetch('/api/admin/config', { headers: { Authorization: `Bearer ${tok}` } });
      return res.status;
    }, token);
    expect(configStatus).toBe(401);
    // 写操作（删房间）拒绝
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

  test('dispatcher audio panel renders (rooms or empty state)', async ({ page }) => {
    await page.goto('/audio');
    await expect(page.locator('h2', { hasText: 'Audio Conference Rooms' })).toBeVisible();
    const hasRooms = await page.locator('.audio-room').count();
    if (hasRooms === 0) {
      await expect(page.locator('.empty')).toContainText('No active audio rooms');
    }
  });
});

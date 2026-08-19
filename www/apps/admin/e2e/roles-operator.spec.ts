import { test, expect } from '@playwright/test';

// I1 review 回归: operator（G3 can_status 但无 admin REST 准入）不应看到监控导航 —
// 前端 canMonitor 必须与 server auth_middleware 准入一致（admin|dispatcher）。

test.describe('I1 operator admission alignment', () => {
  test('operator does NOT see audio/vehicles nav, header shows role', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('.header .version')).toContainText('operator [operator]');
    await expect(page.locator('nav a', { hasText: 'Audio Conference' })).toHaveCount(0);
    await expect(page.locator('nav a', { hasText: 'Vehicles' })).toHaveCount(0);
    // Dashboard 数据走 admin REST → operator 被服务端拒绝 → 401 错误态（准入一致性）
    await expect(page.locator('.error')).toContainText('Authentication required');
  });

  test('operator admin REST denied (server admission)', async ({ page }) => {
    await page.goto('/');
    const token = await page.evaluate(() => localStorage.getItem('mediaservo_admin_token'));
    const statusStatus = await page.evaluate(async (tok) => {
      const res = await fetch('/api/admin/status', { headers: { Authorization: `Bearer ${tok}` } });
      return res.status;
    }, token);
    expect(statusStatus).toBe(401);
  });
});

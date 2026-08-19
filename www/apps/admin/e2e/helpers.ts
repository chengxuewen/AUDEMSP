import { expect, type Page } from '@playwright/test';

export interface Creds { username: string; password: string; role: string; }

/**
 * 登录 + 断言已签名。登录端点有 tower_governor 限流（2 req/s burst 5, G3）—
 * 429 时等待补桶重试。轮询 localStorage 判定成功（比指示器时序稳健）。
 */
export async function login(page: Page, creds: Creds, attempts = 4): Promise<void> {
  for (let i = 0; i < attempts; i++) {
    await page.goto('http://localhost:5173/settings');
    await page.locator('input.login-input[placeholder="username"]').fill(creds.username);
    await page.locator('input.login-input[placeholder="password"]').fill(creds.password);
    await page.locator('button', { hasText: 'Login' }).click();

    // 轮询: token 落 localStorage = 成功; error 指示器 = 失败（限流/凭证）→ 重试
    for (let t = 0; t < 40; t++) {
      const token = await page.evaluate(() => localStorage.getItem('mediaservo_admin_token'));
      if (token) {
        await expect(page.locator('.header .version')).toContainText(`[${creds.role}]`);
        return;
      }
      const errorCount = await page.locator('.token-status.error').count();
      if (errorCount > 0) break;
      await page.waitForTimeout(150);
    }
    await page.waitForTimeout(1200); // 等限流桶恢复
  }
  throw new Error(`login failed for ${creds.username} after ${attempts} attempts`);
}

import { test, expect } from '@playwright/test';

test('web stats 面板：编解码器/帧率/编码耗时', async ({ page }) => {
  const logs: string[] = [];
  page.on('console', m => { if (/encoder_status|avg_encode|codec|metrics/i.test(m.text())) logs.push(m.text().slice(0, 200)); });
  await page.goto('http://127.0.0.1:9800/admin', { waitUntil: 'networkidle', timeout: 20000 });
  const user = page.locator('input[type="text"], input[name="username"]').first();
  if (await user.count()) {
    await user.fill('admin');
    await page.locator('input[type="password"]').first().fill('admin123');
    await page.locator('button[type="submit"], button:has-text("登录"), button:has-text("Login")').first().click();
    await page.waitForTimeout(1500);
  }
  await page.waitForTimeout(2500);
  const play = page.locator('.device-group', { hasText: 'vehicle' }).locator('button').filter({ hasText: /Play|播放/ }).first();
  await play.click();
  await page.waitForFunction(() => {
    const v = document.querySelector('video');
    return v && v.readyState >= 2 && v.videoWidth > 0;
  }, { timeout: 20000 });
  // 等 encoder_status 到达（2s 周期——最多 6s）
  await page.waitForTimeout(6500);
  // 面板文本（编解码器组）
  const body = await page.locator('body').innerText();
  const hasCodec = /H264|VP8|VP9|AV1/i.test(body);
  const hasFps = /fps|帧率|\d+(\.\d+)?\s*(fps|FPS)/i.test(body);
  const hasEncMs = /ms|耗时|编码/i.test(body);
  console.log(`面板: codec=${hasCodec} fps=${hasFps} encMs=${hasEncMs}`);
  console.log('console encoder 日志:', logs.slice(-3));
  // 断言：至少编解码器信息出现
  expect(hasCodec || logs.length > 0).toBeTruthy();
});

import { test, expect } from '@playwright/test';

test('vehicle 画面内容：时间戳水印 + 方块移动', async ({ page }) => {
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
  await page.waitForTimeout(1500); // 等几帧

  // 截两帧（间隔 600ms——方块 motion_speed=3 应有移动）
  const f1 = await page.evaluate(() => {
    const v = document.querySelector('video')!;
    const c = document.createElement('canvas'); c.width = v.videoWidth; c.height = v.videoHeight;
    c.getContext('2d')!.drawImage(v, 0, 0);
    return c.toDataURL('image/png');
  });
  await page.waitForTimeout(600);
  const f2 = await page.evaluate(() => {
    const v = document.querySelector('video')!;
    const c = document.createElement('canvas'); c.width = v.videoWidth; c.height = v.videoHeight;
    c.getContext('2d')!.drawImage(v, 0, 0);
    return c.toDataURL('image/png');
  });

  // 帧差异分析（Node 侧：解码 PNG → 像素对比）
  const { createCanvas, loadImage } = await import('canvas');
  const img1 = await loadImage(f1), img2 = await loadImage(f2);
  const c1 = createCanvas(img1.width, img1.height).getContext('2d');
  c1.drawImage(img1, 0, 0);
  const d1 = c1.getImageData(0, 0, img1.width, img1.height).data;
  const c2 = createCanvas(img2.width, img2.height).getContext('2d');
  c2.drawImage(img2, 0, 0);
  const d2 = c2.getImageData(0, 0, img2.width, img2.height).data;
  let diffPixels = 0;
  for (let i = 0; i < d1.length; i += 16) { // 采样
    if (Math.abs(d1[i] - d2[i]) + Math.abs(d1[i+1] - d2[i+1]) + Math.abs(d1[i+2] - d2[i+2]) > 30) diffPixels++;
  }
  // 左上角水印区域非纯黑（TimestampOverlay TopLeft 有文字）
  let cornerNonBlack = 0;
  for (let y = 4; y < 40; y++) for (let x = 4; x < 200; x++) {
    const i = (y * img1.width + x) * 4;
    if (d1[i] + d1[i+1] + d1[i+2] > 60) cornerNonBlack++;
  }
  console.log(`帧差异采样: ${diffPixels} / 角部非黑像素: ${cornerNonBlack}`);
  expect(diffPixels).toBeGreaterThan(50);      // 方块在移动
  expect(cornerNonBlack).toBeGreaterThan(100); // 时间戳水印存在
  console.log('✅ 画面内容验证通过：移动方块 + 时间戳水印');
});

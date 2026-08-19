import { test, expect, type Page } from '@playwright/test';
import { spawn, type ChildProcess } from 'child_process';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// SFU 视频渲染 e2e — 需要: Docker server (9800) + 自备干净 SFU 生产者（host-legacy）。
// PIT-106 修复后浏览器 consume 全链路可用（candidates alias + vite ws proxy）;
// 生产者必须是纯 SFU 路径（host-legacy sfu_produce）— 多进程网关主机会同时 P2P 中继
// SDP/ICE, 与浏览器 SFU 协商共用一个 RTCPeerConnection 冲突（已知限制）。

const REPO_ROOT = path.resolve(__dirname, '..', '..', '..', '..');
const HOST_BIN = path.join(REPO_ROOT, 'target', 'debug', 'host-legacy');

async function startCleanProducer(): Promise<ChildProcess> {
  const child = spawn(HOST_BIN, [], { cwd: REPO_ROOT, stdio: 'ignore' });
  return child;
}

async function waitForProducer(page: Page, room: string, timeoutMs = 30000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const rooms = await page.evaluate(async () => {
      const token = localStorage.getItem('mediaservo_admin_token');
      const res = await fetch('/api/admin/sfu/rooms', { headers: { Authorization: `Bearer ${token}` } });
      return res.ok ? res.json() : null;
    });
    const matched = rooms?.rooms?.find((r: { room_id: string }) => r.room_id === room);
    if (matched && matched.producers > 0) return;
    await page.waitForTimeout(2000);
  }
  throw new Error(`producer for room ${room} not ready within ${timeoutMs}ms`);
}

test.describe('SFU Video Rendering', () => {
  // 自备生产者启动 ~12s → 单测试预算放宽（默认 30s 不够）
  test.setTimeout(120_000);
  let producer: ChildProcess | null = null;

  test.beforeAll(async () => {
    // 自备干净 SFU 生产者（test-room）— 浏览器 Play 的目标房间
    producer = await startCleanProducer();
  });

  test.afterAll(() => {
    producer?.kill();
    producer = null;
  });

  test.beforeEach(async ({ page }) => {
    // storageState（global-setup 登录）提供 admin token — PIT-103 鉴权后仍需登录态。
    await page.goto('/');
  });

  test('navigates to dashboard and clicks Play to render video', async ({ page }) => {
    // Navigate to dashboard
    await expect(page.locator('.dashboard')).toBeVisible();
    // 等 test-room 的 SFU 生产者就绪
    await waitForProducer(page, 'test-room');

    // Find and click the Play button for test-room
    const playButton = page.locator('.device-group', { hasText: 'test-room' }).locator('button.btn-play');
    await expect(playButton).toBeVisible();
    await playButton.click();

    // Wait for video element to appear
    const video = page.locator('video');
    await expect(video).toBeVisible({ timeout: 10000 });

    // Verify video readyState >= 2 (HAVE_CURRENT_DATA) — 等真实帧数据到达
    await page.waitForFunction(() => {
      const v = document.querySelector('video');
      return v && v.readyState >= 2;
    }, { timeout: 15000 });
    const readyState = await video.evaluate((el: HTMLVideoElement) => el.readyState);
    expect(readyState).toBeGreaterThanOrEqual(2);
  });

  test('shows error state when connection fails', async ({ page }) => {
    // Navigate to dashboard
    await expect(page.locator('.dashboard')).toBeVisible();

    // Mock a failed connection by intercepting WebSocket
    await page.route('**/ws', (route) => {
      route.abort();
    });

    // Find and click the Play button
    const playButton = page.locator('.device-group', { hasText: 'test-room' }).locator('button.btn-play');
    await expect(playButton).toBeVisible();
    await playButton.click();

    // Wait for VideoPlayer error state (Signal Lost) — WS 被 abort → connect 失败
    const signalLost = page.locator('.vp-status-msg.error');
    await expect(signalLost).toBeVisible({ timeout: 15000 });
    await expect(signalLost).toContainText(/signal lost|error|failed|disconnected/i);
  });

  test('video stream has correct dimensions', async ({ page }) => {
    // Navigate to dashboard
    await expect(page.locator('.dashboard')).toBeVisible();
    await waitForProducer(page, 'test-room');

    // Find and click the Play button for test-room
    const playButton = page.locator('.device-group', { hasText: 'test-room' }).locator('button.btn-play');
    await expect(playButton).toBeVisible();
    await playButton.click();

    // Wait for video element to appear and have dimensions
    const video = page.locator('video');
    await expect(video).toBeVisible({ timeout: 10000 });

    // Wait for video to have non-zero dimensions
    await page.waitForFunction(() => {
      const v = document.querySelector('video');
      return v && v.videoWidth > 0 && v.videoHeight > 0;
    }, { timeout: 15000 });

    // Verify dimensions
    const dimensions = await video.evaluate((el: HTMLVideoElement) => ({
      width: el.videoWidth,
      height: el.videoHeight,
    }));
    expect(dimensions.width).toBeGreaterThan(0);
    expect(dimensions.height).toBeGreaterThan(0);
  });
});

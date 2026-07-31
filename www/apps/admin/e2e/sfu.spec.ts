import { test, expect } from '@playwright/test';

test.describe('SFU Video Rendering', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('navigates to dashboard and clicks Play to render video', async ({ page }) => {
    // Navigate to dashboard
    await expect(page.locator('.dashboard')).toBeVisible();

    // Find and click the Play button
    const playButton = page.locator('button', { hasText: 'Play' });
    await expect(playButton).toBeVisible();
    await playButton.click();

    // Wait for video element to appear
    const video = page.locator('video');
    await expect(video).toBeVisible({ timeout: 10000 });

    // Verify video readyState >= 2 (HAVE_CURRENT_DATA)
    // This means the browser has loaded enough data to render a frame
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
    const playButton = page.locator('button', { hasText: 'Play' });
    await expect(playButton).toBeVisible();
    await playButton.click();

    // Wait for error state to appear
    const errorIndicator = page.locator('.error, .status-error, [data-status="error"]');
    await expect(errorIndicator).toBeVisible({ timeout: 15000 });

    // Verify error message is displayed
    await expect(errorIndicator).toContainText(/error|failed|disconnected/i);
  });

  test('video stream has correct dimensions', async ({ page }) => {
    // Navigate to dashboard
    await expect(page.locator('.dashboard')).toBeVisible();

    // Find and click the Play button
    const playButton = page.locator('button', { hasText: 'Play' });
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

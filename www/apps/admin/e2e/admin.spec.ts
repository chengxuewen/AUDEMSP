import { test, expect } from '@playwright/test';

test.describe('Admin Layout', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('renders header and sidebar navigation', async ({ page }) => {
    await expect(page.locator('header')).toContainText('OMSPBase Admin');
    await expect(page.locator('nav a', { hasText: 'Dashboard' })).toBeVisible();
    await expect(page.locator('nav a', { hasText: 'Settings' })).toBeVisible();
  });

  test('dashboard is the default route', async ({ page }) => {
    await expect(page.locator('nav a.active')).toContainText('Dashboard');
  });

  test('navigates to settings via sidebar', async ({ page }) => {
    await page.locator('nav a', { hasText: 'Settings' }).click();
    await expect(page).toHaveURL('/settings');
    await expect(page.locator('nav a.active')).toContainText('Settings');
  });
});

test.describe('Dashboard Page', () => {
  test('shows dashboard content on load', async ({ page }) => {
    await page.goto('/');
    // The dashboard shows "No active devices" when empty, or stats cards when live
    await expect(page.locator('.dashboard')).toBeVisible();
  });
});

test.describe('Settings Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings');
  });

  test('has token input and save button', async ({ page }) => {
    await expect(page.locator('h2')).toContainText('Settings');
    await expect(page.locator('input.token-input')).toBeVisible();
    await expect(page.locator('button', { hasText: 'Save Token' })).toBeVisible();
  });

  test('save button is clickable with empty input (no-op)', async ({ page }) => {
    await page.locator('button', { hasText: 'Save Token' }).click();
    // No token status appears because input was empty
    await expect(page.locator('.token-status')).toHaveCount(0);
  });

  test('saves and clears a token', async ({ page }) => {
    await page.locator('input.token-input').fill('test-jwt-token');
    await page.locator('button', { hasText: 'Save Token' }).click();
    await expect(page.locator('.token-status')).toContainText('Token saved');
    // Clear button appears after save
    const clearBtn = page.locator('button', { hasText: 'Clear' });
    await expect(clearBtn).toBeVisible();
    await clearBtn.click();
    await expect(page.locator('.token-status')).toHaveCount(0);
    await expect(page.locator('button', { hasText: 'Clear' })).toHaveCount(0);
  });
});

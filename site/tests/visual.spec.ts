import { expect, test } from '@playwright/test';
import { ROUTES, viewport, WIDTHS } from './shell.spec';

/**
 * Pixel baselines.
 *
 * Four, deliberately: two pages at two widths. A baseline per page per
 * breakpoint is a suite nobody updates, and a suite nobody updates is a suite
 * everybody re-records blind. These four cover the two layouts that exist.
 *
 * Record them with `--update-snapshots`. `maxDiffPixelRatio` and disabled
 * animations are set in playwright.config.ts.
 */

test.describe('at 1280', () => {
  test.use({ viewport: viewport(WIDTHS.desktop) });

  test('the landing page matches its baseline at 1280', async ({ page }) => {
    await page.goto(ROUTES.landing);
    await expect(page).toHaveScreenshot('landing-1280.png', { fullPage: true });
  });

  test('a docs page matches its baseline at 1280', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    await expect(page).toHaveScreenshot('docs-1280.png', { fullPage: true });
  });
});

test.describe('at 390', () => {
  test.use({ viewport: viewport(WIDTHS.phone) });

  test('the landing page matches its baseline at 390', async ({ page }) => {
    await page.goto(ROUTES.landing);
    await expect(page).toHaveScreenshot('landing-390.png', { fullPage: true });
  });

  test('a docs page matches its baseline at 390', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    await expect(page).toHaveScreenshot('docs-390.png', { fullPage: true });
  });
});

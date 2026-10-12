import { expect, test } from '@playwright/test';
import { expectHidden, REGION, region, ROUTES, viewport, WIDTHS } from './shell.spec';

/**
 * The drawer below `lg`.
 *
 * A disclosure is the one thing a static-HTML check can never see: the markup
 * for an open drawer and a shut one is the same markup.
 */

test.describe('on a phone', () => {
  test.use({ viewport: viewport(WIDTHS.phone) });

  test.beforeEach(async ({ page }) => {
    await page.goto(ROUTES.docsPage);
  });

  test('the sidebar is behind a button at 390', async ({ page }) => {
    await expect(region(page, 'sidebarToggle')).toBeVisible();
    await expectHidden(page, 'sidebar');
  });

  test('the sidebar opens when the menu button is pressed', async ({ page }) => {
    await region(page, 'sidebarToggle').click();
    await expect(region(page, 'sidebar')).toBeVisible();
  });

  test('the sidebar closes again', async ({ page }) => {
    const toggle = region(page, 'sidebarToggle');
    await toggle.click();
    await expect(region(page, 'sidebar')).toBeVisible();

    // Either spelling of "shut it" is correct: a dedicated close control, or
    // the same button again. A drawer that opens and cannot be closed is the
    // failure this test is for.
    const close = page.locator(REGION.sidebarClose).first();
    if (await close.isVisible()) {
      await close.click();
    } else {
      await toggle.click();
    }
    await expect(page.locator(REGION.sidebar)).toBeHidden();
  });

  test('the menu button reports its state with aria-expanded', async ({ page }) => {
    const toggle = region(page, 'sidebarToggle');
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  });

  test('the open sidebar does not make the page scroll sideways', async ({ page }) => {
    // A drawer that slides in from off-canvas and forgets to clip itself
    // widens the document rather than covering it.
    await region(page, 'sidebarToggle').click();
    await expect(region(page, 'sidebar')).toBeVisible();
    const { scrollWidth, clientWidth } = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    }));
    expect(scrollWidth).toBeLessThanOrEqual(clientWidth);
  });
});

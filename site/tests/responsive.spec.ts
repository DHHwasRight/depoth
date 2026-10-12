import { expect, test } from '@playwright/test';
import {
  documentOverflow,
  expectHidden,
  expectVisible,
  region,
  ROUTES,
  viewport,
  WIDTHS,
} from './shell.spec';

/**
 * The three-column shell, at the widths where it changes shape.
 *
 * Nothing here can be read off the HTML: the sidebar is in the markup at every
 * width, and only the computed layout says whether a reader can see it.
 */

const everyWidth = Object.entries(WIDTHS);

for (const [name, width] of everyWidth) {
  test.describe(`at ${width} (${name})`, () => {
    test.use({ viewport: viewport(width) });

    // The single most common real layout failure, and invisible to any HTML
    // check: one element wider than the viewport drags the whole page
    // sideways. Asserted on both a content-light and a content-heavy page,
    // because the usual culprit is content -- a code block or a table.
    test(`the landing page does not scroll horizontally at ${width}`, async ({ page }) => {
      await page.goto(ROUTES.landing);
      const { scrollWidth, clientWidth } = await documentOverflow(page);
      expect(scrollWidth, `document is ${scrollWidth}px wide in a ${clientWidth}px viewport`)
        .toBeLessThanOrEqual(clientWidth);
    });

    test(`a docs page does not scroll horizontally at ${width}`, async ({ page }) => {
      await page.goto(ROUTES.docsPage);
      const { scrollWidth, clientWidth } = await documentOverflow(page);
      expect(scrollWidth, `document is ${scrollWidth}px wide in a ${clientWidth}px viewport`)
        .toBeLessThanOrEqual(clientWidth);
    });
  });
}

test.describe('below lg', () => {
  test.use({ viewport: viewport(WIDTHS.tablet) });

  test('the sidebar is hidden at 768', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    await expectHidden(page, 'sidebar');
  });

  test('the on-this-page rail is hidden at 768', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    await expectHidden(page, 'rail');
  });
});

test.describe('at lg', () => {
  test.use({ viewport: viewport(WIDTHS.laptop) });

  test('the sidebar appears at 1024 and not at 768', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    await expectVisible(page, 'sidebar');

    await page.setViewportSize(viewport(WIDTHS.tablet));
    await expectHidden(page, 'sidebar');
  });

  test('the sidebar is 288px wide once it appears', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    const box = await region(page, 'sidebar').boundingBox();
    expect(box, 'sidebar has no box').not.toBeNull();
    // 288px exactly, give or take a border or a scrollbar gutter.
    expect(box!.width).toBeGreaterThanOrEqual(284);
    expect(box!.width).toBeLessThanOrEqual(292);
  });

  test('the on-this-page rail is still hidden at 1024', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    await expectHidden(page, 'rail');
  });

  test('the menu button is gone once the sidebar is permanent at 1024', async ({ page }) => {
    // Two ways to the same navigation on one screen is a reported bug, not a
    // hypothetical: readers open the drawer over a sidebar that was already
    // there.
    await page.goto(ROUTES.docsPage);
    await expectHidden(page, 'sidebarToggle');
  });
});

test.describe('at xl', () => {
  test.use({ viewport: viewport(WIDTHS.desktop) });

  test('the on-this-page rail appears at 1280 and not at 1024', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    await expectVisible(page, 'rail');

    await page.setViewportSize(viewport(WIDTHS.laptop));
    await expectHidden(page, 'rail');
  });

  test('the sidebar is still there at 1280', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    await expectVisible(page, 'sidebar');
  });
});

test.describe('above every breakpoint', () => {
  test.use({ viewport: viewport(WIDTHS.wide) });

  test('the content pane stops growing between 1280 and 1440', async ({ page }) => {
    // A docs column that keeps widening with the window is unreadable by 1440.
    await page.goto(ROUTES.docsPage);
    const wide = await region(page, 'main').boundingBox();

    await page.setViewportSize(viewport(WIDTHS.desktop));
    const desktop = await region(page, 'main').boundingBox();

    expect(wide, 'main has no box').not.toBeNull();
    expect(desktop, 'main has no box').not.toBeNull();
    expect(wide!.width - desktop!.width).toBeLessThanOrEqual(
      WIDTHS.wide - WIDTHS.desktop,
    );
  });
});

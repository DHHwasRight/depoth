import { expect, test } from '@playwright/test';
import { inside, ROUTES, viewport, WIDTHS } from './shell.spec';

/**
 * Which sidebar link says "you are here".
 *
 * The Rust harness sees that the attribute exists somewhere in the file. What
 * it cannot see is that the marked link is the one the reader is standing on,
 * and that the mark moves when they walk. A navigation generated once and
 * stamped onto every page passes the markup check and fails this one.
 */

// The sidebar must be on screen for its links to be queryable as rendered.
test.use({ viewport: viewport(WIDTHS.desktop) });

// `:visible`, because a shell that renders the sidebar twice -- once as the
// drawer, once as the column -- legitimately marks the link in both, and only
// one of them is on screen at a time.
const current = (page: import('@playwright/test').Page) =>
  page.locator(inside('sidebar', 'a[aria-current="page"]:visible'));

test('exactly one sidebar link carries aria-current=page', async ({ page }) => {
  await page.goto(ROUTES.docsPage);
  await expect(current(page)).toHaveCount(1);
});

test('the link marked aria-current is the page being read', async ({ page }) => {
  await page.goto(ROUTES.docsPage);
  const href = await current(page).getAttribute('href');
  expect(href, 'no marked link').not.toBeNull();
  const resolved = new URL(href!, page.url()).pathname.replace(/\/$/, '');
  expect(resolved).toBe(ROUTES.docsPage.replace(/\/$/, ''));
});

test('the active link moves when the reader moves', async ({ page }) => {
  await page.goto(ROUTES.docsPage);
  const first = await current(page).getAttribute('href');

  await page.goto(ROUTES.otherDocsPage);
  await expect(current(page)).toHaveCount(1);
  const second = await current(page).getAttribute('href');

  expect(second, 'the same link is marked on both pages').not.toBe(first);
  const resolved = new URL(second!, page.url()).pathname.replace(/\/$/, '');
  expect(resolved).toBe(ROUTES.otherDocsPage.replace(/\/$/, ''));
});

test('the marked link is styled differently from its neighbours', async ({ page }) => {
  // aria-current with no visual difference tells a screen reader where it is
  // and tells everybody else nothing.
  await page.goto(ROUTES.docsPage);
  const styles = await page.locator(inside('sidebar', 'a:visible')).evaluateAll((links) =>
    links.map((link) => {
      const style = getComputedStyle(link);
      return {
        active: link.getAttribute('aria-current') === 'page',
        signature: [style.color, style.backgroundColor, style.fontWeight].join('|'),
      };
    }),
  );
  const active = styles.find((s) => s.active);
  const others = styles.filter((s) => !s.active);
  expect(active, 'no marked link').toBeDefined();
  expect(others.length, 'the sidebar has one link').toBeGreaterThan(0);
  expect(
    others.every((other) => other.signature !== active!.signature),
    `marked link renders as ${active!.signature}, same as a plain one`,
  ).toBe(true);
});

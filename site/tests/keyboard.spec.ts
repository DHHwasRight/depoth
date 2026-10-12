import { expect, test } from '@playwright/test';
import { REGION, ROUTES, viewport, WIDTHS } from './shell.spec';

/**
 * What a keyboard reaches, and what it can see once it gets there.
 *
 * The skip link is in the markup on every site that has one; whether it is the
 * first thing Tab reaches, and whether it becomes visible when it is focused,
 * is a browser question.
 */

test.use({ viewport: viewport(WIDTHS.desktop) });

/** Tag, text and the ring, for whatever has focus right now. */
async function focused(page: import('@playwright/test').Page) {
  return page.evaluate(() => {
    const el = document.activeElement as HTMLElement | null;
    if (!el || el === document.body) return null;
    const style = getComputedStyle(el);
    return {
      tag: el.tagName.toLowerCase(),
      text: (el.textContent ?? '').trim().slice(0, 40),
      href: el.getAttribute('href'),
      outlineStyle: style.outlineStyle,
      outlineWidth: parseFloat(style.outlineWidth) || 0,
      boxShadow: style.boxShadow,
      // A skip link is positioned off-screen until focused; once focused it
      // must actually be on screen, or it is decoration.
      rect: el.getBoundingClientRect().toJSON(),
    };
  });
}

function hasRing(state: NonNullable<Awaited<ReturnType<typeof focused>>>) {
  const outline = state.outlineStyle !== 'none' && state.outlineWidth > 0;
  const shadow = state.boxShadow !== 'none' && state.boxShadow !== '';
  return outline || shadow;
}

test('the first Tab from the top reaches the skip link', async ({ page }) => {
  await page.goto(ROUTES.docsPage);
  await page.keyboard.press('Tab');

  const state = await focused(page);
  expect(state, 'nothing took focus on the first Tab').not.toBeNull();
  const isSkipLink = await page.locator(REGION.skipLink).first().evaluate(
    (node) => node === document.activeElement,
  ).catch(() => false);
  expect(isSkipLink, `first Tab landed on <${state!.tag}> ${JSON.stringify(state!.text)}`).toBe(true);
});

test('the focused skip link is on screen', async ({ page }) => {
  // A skip link is parked off-screen until it has focus. `toBeInViewport`
  // rather than one reading of the box: the link slides in, and a single
  // measurement taken as the transition starts reports it still outside.
  await page.goto(ROUTES.docsPage);
  const skipLink = page.locator(REGION.skipLink).first();
  await page.keyboard.press('Tab');
  await expect(skipLink, 'the skip link stays off-screen while focused').toBeInViewport();
  await expect(skipLink).toBeVisible();
});

test('the second Tab reaches the navigation', async ({ page }) => {
  await page.goto(ROUTES.docsPage);
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');

  const inNav = await page.evaluate(() => {
    const el = document.activeElement;
    return !!el && !!el.closest('header, nav');
  });
  const state = await focused(page);
  expect(inNav, `second Tab landed on <${state?.tag}> ${JSON.stringify(state?.text)}`).toBe(true);
});

test('the skip link jumps past the chrome into main', async ({ page }) => {
  await page.goto(ROUTES.docsPage);
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');

  const target = await page.evaluate(() => {
    const hash = location.hash.slice(1);
    const node = hash ? document.getElementById(hash) : null;
    if (!node) return null;
    return { inMain: node.tagName.toLowerCase() === 'main' || !!node.closest('main') };
  });
  expect(target, 'the skip link points at no element on the page').not.toBeNull();
  expect(target!.inMain).toBe(true);
});

test('every interactive element shows a focus ring when tabbed to', async ({ page }) => {
  // Tab, rather than .focus(): :focus-visible only applies to a keyboard, and
  // a ring that exists under :focus but not :focus-visible is invisible to
  // exactly the people who need it.
  await page.goto(ROUTES.docsPage);

  const ringless: string[] = [];
  const seen = new Set<string>();
  for (let i = 0; i < 40; i += 1) {
    await page.keyboard.press('Tab');
    const state = await focused(page);
    if (!state) break;
    const key = `${state.tag}:${state.href ?? ''}:${state.text}`;
    if (seen.has(key)) break; // wrapped round to the start
    seen.add(key);
    if (!hasRing(state)) ringless.push(`<${state.tag}> ${JSON.stringify(state.text)}`);
  }

  expect(seen.size, 'nothing was reachable by keyboard').toBeGreaterThan(0);
  expect(ringless, `no visible focus ring on: ${ringless.join(', ')}`).toEqual([]);
});

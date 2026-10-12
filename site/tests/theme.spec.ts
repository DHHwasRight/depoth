import { expect, test } from '@playwright/test';
import { isTransparent, luminance, ROUTES } from './shell.spec';

/**
 * Light, dark, and the system default.
 *
 * The contract: an explicit choice stamps `data-theme="dark"` or `"light"` on
 * the root element; the default stamps nothing, so system dark is reachable
 * only through `prefers-color-scheme`. That is three states, and a site that
 * only ever paints the one its author uses looks right locally and wrong
 * everywhere else.
 */

async function bodyBackground(page: import('@playwright/test').Page) {
  return page.evaluate(() => getComputedStyle(document.body).backgroundColor);
}

test.describe('the system default', () => {
  test('no explicit choice stamps nothing on the root element', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    const stamped = await page.evaluate(() =>
      document.documentElement.getAttribute('data-theme'),
    );
    expect(stamped, 'the default must be unstamped, or system dark cannot work').toBeNull();
  });

  test('the body paints its own background rather than inheriting the host', async ({ page }) => {
    // The classic bug. A transparent body borrows whatever is behind it, so
    // the page looks right in a browser with a white canvas and wrong the
    // moment it is embedded or the canvas is dark.
    await page.goto(ROUTES.docsPage);
    const background = await bodyBackground(page);
    expect(isTransparent(background), `body background is ${background}`).toBe(false);
  });

  test('a system dark preference darkens the page', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto(ROUTES.docsPage);
    const background = await bodyBackground(page);
    expect(isTransparent(background), `body background is ${background}`).toBe(false);
    expect(luminance(background), `body background is ${background}`).toBeLessThan(0.2);
  });

  test('a system light preference lightens the page', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto(ROUTES.docsPage);
    const background = await bodyBackground(page);
    expect(luminance(background), `body background is ${background}`).toBeGreaterThan(0.6);
  });

  test('the two system schemes do not render the same page', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto(ROUTES.docsPage);
    const light = await bodyBackground(page);

    await page.emulateMedia({ colorScheme: 'dark' });
    const dark = await bodyBackground(page);

    expect(dark, `both schemes paint ${light}`).not.toBe(light);
  });
});

test.describe('an explicit choice', () => {
  test('data-theme=dark overrides a light system preference', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto(ROUTES.docsPage);
    await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'dark'));
    const background = await bodyBackground(page);
    // Guard first: a transparent body reads as luminance 0 and would pass the
    // darkness check while painting nothing at all.
    expect(isTransparent(background), `body background is ${background}`).toBe(false);
    expect(luminance(background), `body background is ${background}`).toBeLessThan(0.2);
  });

  test('data-theme=light overrides a dark system preference', async ({ page }) => {
    // The half that is forgotten: a dark-first stylesheet writes the dark
    // rules under a media query and never under `[data-theme="light"]`, so
    // the toggle works one way only.
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto(ROUTES.docsPage);
    await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'light'));
    const background = await bodyBackground(page);
    expect(luminance(background), `body background is ${background}`).toBeGreaterThan(0.6);
  });

  test('text stays readable against the dark background', async ({ page }) => {
    // A theme that flips the ground and not the ink is worse than no theme.
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto(ROUTES.docsPage);
    const { ink, ground } = await page.evaluate(() => {
      const main = document.querySelector('main') ?? document.body;
      const paragraph = main.querySelector('p') ?? main;
      return {
        ink: getComputedStyle(paragraph).color,
        ground: getComputedStyle(document.body).backgroundColor,
      };
    });
    const contrast =
      (Math.max(luminance(ink), luminance(ground)) + 0.05) /
      (Math.min(luminance(ink), luminance(ground)) + 0.05);
    expect(contrast, `${ink} on ${ground}`).toBeGreaterThanOrEqual(4.5);
  });
});

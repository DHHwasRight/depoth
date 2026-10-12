/**
 * The contract every other spec here is written against, and the handful of
 * measurements they share.
 *
 * It carries the `.spec.ts` suffix and declares no test on purpose: Playwright
 * refuses to load a test file that imports the config, so the shared values
 * cannot live there, and this directory holds `*.spec.ts` files. A test file
 * with no tests is inert -- the runner lists nothing from it.
 */
import { expect, type Locator, type Page } from '@playwright/test';

/** Fixed port, so `scripts/serve.sh` and the config cannot disagree. */
export const PORT = 4317;
export const BASE_URL = `http://127.0.0.1:${PORT}`;

/**
 * The widths that matter, named for what changes at each. They are the
 * breakpoints of the three-column shell, not a round-number sample.
 */
export const WIDTHS = {
  /** Phone. Sidebar behind a button, no rail. */
  phone: 390,
  /** Tablet. Still below `lg`, so still no sidebar. */
  tablet: 768,
  /** `lg`: the 288px sidebar appears. */
  laptop: 1024,
  /** `xl`: the on-this-page rail appears. */
  desktop: 1280,
  /** Above every breakpoint; the shell should stop growing. */
  wide: 1440,
} as const;

/** Height is irrelevant to every layout claim here, so it is one value. */
export const HEIGHT = 900;

export function viewport(width: number) {
  return { width, height: HEIGHT };
}

/**
 * The routes under test, from `depoth.yaml`'s navigation.
 * `/docs/configuration/` is the page with both wide code and tables.
 */
export const ROUTES = {
  landing: '/',
  docsIndex: '/docs/',
  docsPage: '/docs/configuration/',
  otherDocsPage: '/docs/install/',
} as const;

/**
 * The markup contract, as CSS.
 *
 * Each region lists every spelling the templates might reasonably use, comma
 * joined, so a template that marks its sidebar `[data-sidebar]` and one that
 * marks it `nav[aria-label="Docs"]` both satisfy the same test. A region that
 * matches nothing fails the test that needs it, which is the correct outcome:
 * the test is a claim about the page, not about a selector.
 */
export const REGION = {
  sidebar:
    '[data-sidebar], #sidebar, nav[aria-label*="sidebar" i], nav[aria-label*="docs" i], aside[aria-label*="docs" i], .sidebar',
  rail:
    '[data-rail], #on-this-page, nav[aria-label*="on this page" i], aside[aria-label*="on this page" i], nav[aria-label*="contents" i], .toc',
  sidebarToggle:
    '[data-sidebar-toggle], [data-nav-toggle], button[aria-controls="docs-sidebar"], button[aria-controls="sidebar"], button[aria-label*="menu" i], button[aria-label*="navigation" i]',
  sidebarClose: '[data-sidebar-close], button[aria-label*="close" i]',
  skipLink:
    '[data-skip-link], a.skip-link, a[href="#main"], a[href="#content"], a[href="#main-content"]',
  main: 'main',
} as const;

export type RegionName = keyof typeof REGION;

export function region(page: Page, name: RegionName): Locator {
  return page.locator(REGION[name]).first();
}

/**
 * A selector for descendants of a region.
 *
 * The suffix has to be distributed over every alternative: `"a, b" + " x"` is
 * `"a, b x"`, which matches the whole of `a` and only part of `b`. That silent
 * misreading made a count assertion pass against the region itself.
 */
export function inside(name: RegionName, suffix: string): string {
  return REGION[name]
    .split(',')
    .map((alternative) => `${alternative.trim()} ${suffix}`)
    .join(', ');
}

/** Asserts a region is on screen, naming the region when it is not. */
export async function expectVisible(page: Page, name: RegionName) {
  await expect(region(page, name), `${name} should be visible`).toBeVisible();
}

/**
 * Asserts a region is not on screen. A region absent from the DOM also passes:
 * `hidden lg:block` and "not rendered below lg" are both correct answers to
 * "is the sidebar showing on a phone".
 */
export async function expectHidden(page: Page, name: RegionName) {
  await expect(page.locator(REGION[name]), `${name} should be hidden`).toBeHidden();
}

/**
 * The width the document *wants* against the width it *has*.
 *
 * One number that catches the commonest real layout failure: an element wider
 * than the viewport drags the whole page sideways, and no amount of reading
 * the HTML reveals it.
 */
export async function documentOverflow(page: Page) {
  return page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }));
}

/** Relative luminance of a computed `rgb()`/`rgba()` string, 0..1. */
export function luminance(color: string): number {
  const parts = color.match(/[\d.]+/g);
  if (!parts || parts.length < 3) throw new Error(`not a colour: ${color}`);
  const [r, g, b] = parts
    .slice(0, 3)
    .map(Number)
    .map((channel) => {
      const c = channel / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** True when a colour is fully transparent -- the bug the theme tests look for. */
export function isTransparent(color: string): boolean {
  if (color.replace(/\s/g, '') === 'transparent') return true;
  const parts = color.match(/[\d.]+/g);
  return parts?.length === 4 && Number(parts[3]) === 0;
}

import { defineConfig, devices, expect, type Locator, type Page } from '@playwright/test';

/**
 * Browser tests for the depoth documentation site.
 *
 * The division of labour with `crates/depoth-test` is strict. That crate reads
 * the built HTML and owns every claim that markup alone can settle: dead
 * links, dead assets, duplicate chrome, incomplete documents, links into a
 * partial, heading order. It runs in milliseconds. Nothing here repeats it.
 *
 * This suite owns only what a browser can see and a parser cannot: layout at a
 * real width, a container that scrolls instead of the page, a disclosure that
 * opens, a computed colour, a focus ring, a pixel baseline.
 */

/** Fixed port, so `scripts/serve.sh` and this config cannot disagree. */
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

/** The routes under test. `/docs/configuration` is the page with wide code and tables. */
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
    '[data-sidebar-toggle], button[aria-controls="sidebar"], header button[aria-expanded], button[aria-label*="menu" i], button[aria-label*="navigation" i]',
  sidebarClose: '[data-sidebar-close], button[aria-label*="close" i]',
  skipLink: '[data-skip-link], a.skip-link, a[href="#main"], a[href="#content"], a[href="#main-content"]',
  main: 'main',
} as const;

export type RegionName = keyof typeof REGION;

export function region(page: Page, name: RegionName): Locator {
  return page.locator(REGION[name]).first();
}

/** Asserts a region is in the page and shown, naming the region when it is not. */
export async function expectVisible(page: Page, name: RegionName) {
  await expect(region(page, name), `${name} should be visible`).toBeVisible();
}

/**
 * Asserts a region is not shown. A region that is absent from the DOM also
 * passes: `hidden lg:block` and "not rendered below lg" are both correct
 * answers to "is the sidebar showing on a phone".
 */
export async function expectHidden(page: Page, name: RegionName) {
  await expect(page.locator(REGION[name]), `${name} should be hidden`).toBeHidden();
}

/**
 * The width the document *wants* against the width it *has*.
 *
 * One number that catches the commonest real layout failure: an element wider
 * than the viewport pushes the whole page sideways, and no amount of reading
 * the HTML reveals it.
 */
export async function documentOverflow(page: Page) {
  return page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }));
}

/** Perceived lightness of a computed `rgb()`/`rgba()` string, 0..1. */
export function luminance(color: string): number {
  const parts = color.match(/[\d.]+/g);
  if (!parts || parts.length < 3) throw new Error(`not a colour: ${color}`);
  const [r, g, b] = parts.slice(0, 3).map(Number).map((channel) => {
    const c = channel / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** True when a colour is fully transparent -- the bug the theme tests look for. */
export function isTransparent(color: string): boolean {
  const normalised = color.replace(/\s/g, '');
  if (normalised === 'transparent') return true;
  const parts = color.match(/[\d.]+/g);
  return parts?.length === 4 && Number(parts[3]) === 0;
}

export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : [['list']],
  // A screenshot of a page that is still fading in is a screenshot of nothing.
  expect: {
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.01,
      animations: 'disabled',
      caret: 'hide',
      scale: 'css',
    },
  },
  use: {
    baseURL: BASE_URL,
    // Pinned, so a baseline does not change with the machine's own setting.
    // The theme spec overrides it per test.
    colorScheme: 'light',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], viewport: viewport(WIDTHS.desktop) },
    },
  ],
  webServer: {
    command: 'bash scripts/serve.sh',
    url: `${BASE_URL}/`,
    // The first Rust build compiles the whole workspace from cold. Ten minutes
    // is not generous for a fast machine; it is enough for a cold CI runner.
    timeout: 600_000,
    reuseExistingServer: !process.env.CI,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});

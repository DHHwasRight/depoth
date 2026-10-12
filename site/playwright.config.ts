import { defineConfig, devices } from '@playwright/test';

/**
 * Browser tests for the depoth documentation site.
 *
 * The division of labour with `crates/depoth-test` is strict. That crate reads
 * the built HTML and owns every claim markup alone can settle: dead links,
 * dead assets, duplicate chrome, incomplete documents, links into a partial,
 * heading order. It runs in milliseconds. Nothing here repeats it.
 *
 * This suite owns only what a browser can see and a parser cannot: layout at a
 * real width, a container that scrolls instead of the page, a disclosure that
 * opens, a computed colour, a focus ring, a pixel baseline.
 *
 * The shared contract -- widths, routes, region selectors -- is in
 * `tests/shell.spec.ts`, because Playwright refuses to load a test file that
 * imports this one.
 */

/** Must match `PORT` in tests/shell.spec.ts and the default in scripts/serve.sh. */
const PORT = 4317;
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: './tests',
  // Everything but `shell.spec.ts`, which holds the shared contract and
  // declares no test. Playwright refuses to let one test file import another,
  // so the file must be excluded here to be importable there.
  testMatch: /^(?!.*shell\.spec\.ts$).*\.spec\.ts$/,
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : [['list']],
  expect: {
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.01,
      // A screenshot of a page still fading in is a screenshot of nothing.
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
      // Every spec that cares sets its own viewport; this is the default for
      // the ones that do not.
      use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 900 } },
    },
  ],
  webServer: {
    command: 'bash scripts/serve.sh',
    url: `${BASE_URL}/`,
    // The first Rust build compiles the whole workspace from cold. Ten minutes
    // is not generous on a fast machine; it is enough on a cold CI runner.
    timeout: 600_000,
    reuseExistingServer: !process.env.CI,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});

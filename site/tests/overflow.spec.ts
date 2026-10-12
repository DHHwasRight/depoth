import { expect, test, type Locator } from '@playwright/test';
import { documentOverflow, ROUTES, viewport, WIDTHS } from './shell.spec';

/**
 * Wide content scrolls inside its own box.
 *
 * A code sample or a table is always wider than a phone. The question is only
 * which element takes the scroll: the block itself, or the page. If it is the
 * page, every line of prose on it is now 40px off-screen too.
 */

type Measurement = { scrollWidth: number; clientWidth: number; overflowX: string; text: string };

async function measure(locator: Locator): Promise<Measurement[]> {
  return locator.evaluateAll((nodes) =>
    nodes.map((node) => ({
      scrollWidth: node.scrollWidth,
      clientWidth: node.clientWidth,
      overflowX: getComputedStyle(node).overflowX,
      text: (node.textContent ?? '').slice(0, 60).replace(/\s+/g, ' '),
    })),
  );
}

test.describe('on a phone', () => {
  test.use({ viewport: viewport(WIDTHS.phone) });

  test('a code block wider than the phone scrolls itself and not the page', async ({ page }) => {
    await page.goto(ROUTES.docsPage);

    const blocks = await measure(page.locator('pre'));
    expect(blocks.length, 'the page under test has no code blocks').toBeGreaterThan(0);

    const widest = blocks.reduce((a, b) =>
      b.scrollWidth - b.clientWidth > a.scrollWidth - a.clientWidth ? b : a,
    );
    expect(
      widest.scrollWidth,
      'no code block overflows at 390px, so this test proves nothing -- ' +
        'the page under test needs a sample with a long line',
    ).toBeGreaterThan(widest.clientWidth);

    const { scrollWidth, clientWidth } = await documentOverflow(page);
    expect(scrollWidth, 'the code block pushed the document sideways').toBeLessThanOrEqual(
      clientWidth,
    );
  });

  test('every code block carries its own horizontal scroll', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    const blocks = await measure(page.locator('pre'));
    expect(blocks.length).toBeGreaterThan(0);
    for (const block of blocks) {
      expect(['auto', 'scroll'], `pre ${JSON.stringify(block.text)} has overflow-x: ${block.overflowX}`)
        .toContain(block.overflowX);
    }
  });

  test('every table carries its own horizontal scroll', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    // The scroller is usually the wrapper the renderer puts around the table,
    // so the claim is about the table or its parent, whichever scrolls.
    const tables = await page.locator('table').evaluateAll((nodes) =>
      nodes.map((node) => {
        const parent = node.parentElement;
        return {
          self: getComputedStyle(node).overflowX,
          parent: parent ? getComputedStyle(parent).overflowX : 'none',
          caption: (node.textContent ?? '').slice(0, 40).replace(/\s+/g, ' '),
        };
      }),
    );
    expect(tables.length, 'the page under test has no tables').toBeGreaterThan(0);
    for (const table of tables) {
      const scrolls = ['auto', 'scroll'].includes(table.self) || ['auto', 'scroll'].includes(table.parent);
      expect(scrolls, `table ${JSON.stringify(table.caption)} scrolls the page, not itself`).toBe(true);
    }
  });
});

test.describe('on a desktop', () => {
  test.use({ viewport: viewport(WIDTHS.desktop) });

  test('a long code line does not widen the content pane at 1280', async ({ page }) => {
    await page.goto(ROUTES.docsPage);
    const { scrollWidth, clientWidth } = await documentOverflow(page);
    expect(scrollWidth).toBeLessThanOrEqual(clientWidth);
  });
});

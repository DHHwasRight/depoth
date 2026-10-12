import { expect, test } from "@playwright/test";

/**
 * A card's title and its shadow must name the same thing.
 *
 * Each group on the landing page owns one hue and uses it for its hairline
 * and for the glow its cards cast on hover. The title used the page accent
 * instead, so hovering a green-glowing card turned its title blue — two
 * colours making two different claims about one card.
 *
 * This is a computed-style claim, so it cannot be checked by reading the
 * markup: both the glow and the title resolve through custom properties that
 * only exist once the stylesheet is applied.
 */

/** `rgb(a, b, c)` -> [a, b, c]; the alpha form is accepted too. */
function channels(colour: string): [number, number, number] {
  const parts = colour.match(/[\d.]+/g);
  if (!parts || parts.length < 3) throw new Error(`not a colour: ${colour}`);
  return [Number(parts[0]), Number(parts[1]), Number(parts[2])];
}

/** How far apart two colours are, as a fraction of the full range. */
function distance(a: string, b: string): number {
  const [r1, g1, b1] = channels(a);
  const [r2, g2, b2] = channels(b);
  return Math.sqrt((r1 - r2) ** 2 + (g1 - g2) ** 2 + (b1 - b2) ** 2) / (255 * Math.sqrt(3));
}

test("a card title takes its group's hue on hover, not the page accent", async ({ page }) => {
  await page.goto("/");
  // Colour is asserted exactly, so the transition has to be out of the way.
  // Polling for "it changed" returns mid-animation and compares a blend.
  await page.addStyleTag({ content: "*, *::before, *::after { transition: none !important }" });

  const cards = page.locator(".doc-card");
  const count = await cards.count();
  expect(count, "the landing page should have cards").toBeGreaterThan(3);

  const seen = new Set<string>();

  for (let i = 0; i < count; i++) {
    const card = cards.nth(i);
    const title = card.locator(".doc-card__title");

    const colour = () => title.evaluate((el) => getComputedStyle(el).color);
    const resting = await colour();
    await card.hover();
    const hovered = await colour();
    expect(hovered, `card ${i} did not change on hover`).not.toBe(resting);

    // The group's own hue, read from the same custom property the shadow uses.
    const groupInk = await card.evaluate((el) =>
      getComputedStyle(el).getPropertyValue("--glow-ink").trim(),
    );
    expect(groupInk, `card ${i} sits outside a group`).not.toBe("");

    // Resolve the group's token through the browser so both sides are rgb().
    const resolved = await page.evaluate((value) => {
      const probe = document.createElement("span");
      probe.style.color = value;
      document.body.appendChild(probe);
      const out = getComputedStyle(probe).color;
      probe.remove();
      return out;
    }, groupInk);

    expect(
      distance(hovered, resolved),
      `card ${i}: title ${hovered} should match its glow ${resolved}`,
    ).toBeLessThan(0.02);

    seen.add(resolved);
    await page.mouse.move(0, 0);
  }

  // If every group resolved to one colour the rule would pass by doing
  // nothing, which is the failure this whole file exists to catch.
  expect(seen.size, "the groups should not all share one hue").toBeGreaterThan(1);
});

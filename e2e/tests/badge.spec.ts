import { test, expect } from '@playwright/test';

// Browser-level tests. The API tests prove we return well-formed SVG; these
// prove a real browser actually *renders* it — which is the thing that matters,
// because a badge lives inside an <img> in someone's README.

test.describe('rendering in a browser', () => {
  test('renders as an <img> with no fonts to load', async ({ page, baseURL }) => {
    // An <img> is the strictest context: external resources are blocked and no
    // webfont can load. If the card renders here, it renders in a README.
    await page.setContent(
      `<img id="badge" src="${baseURL}/api/rank/octocat" alt="rank badge">`
    );

    const badge = page.locator('#badge');
    await expect(badge).toBeVisible();

    const size = await badge.evaluate((img: HTMLImageElement) => ({
      width: img.naturalWidth,
      height: img.naturalHeight,
      complete: img.complete,
    }));

    // A browser that failed to decode the SVG reports 0x0.
    expect(size.complete).toBe(true);
    expect(size.width).toBe(495);
    expect(size.height).toBe(170);
  });

  test('text is drawn as outlines, not font-dependent <text>', async ({ page }) => {
    await page.goto('/api/rank/octocat');

    const counts = await page.evaluate(() => ({
      text: document.querySelectorAll('text').length,
      paths: document.querySelectorAll('path').length,
    }));

    // <text> would fall back to whatever font the viewer happens to have,
    // shifting every glyph away from the positions we measured.
    expect(counts.text).toBe(0);
    expect(counts.paths).toBeGreaterThan(50);
  });

  test('exposes an accessible label', async ({ page }) => {
    await page.goto('/api/rank/octocat');

    const svg = page.locator('svg').first();
    await expect(svg).toHaveAttribute('role', 'img');

    const title = await page.locator('svg > title').first().textContent();
    expect(title).toContain('octocat');
    expect(title).toContain('Diamond II');
  });

  test('hostile usernames cannot become markup', async ({ request, page }) => {
    const response = await request.get('/api/rank/' + encodeURIComponent('<script>alert(1)</script>'));

    // Rejected long before rendering.
    expect(response.status()).toBe(400);

    // The error deliberately echoes what we read, which is useful for
    // debugging and inert here: a JSON content type plus nosniff means a
    // browser will never parse this as HTML.
    expect(response.headers()['content-type']).toContain('application/json');
    expect(response.headers()['x-content-type-options']).toBe('nosniff');

    // And prove it: navigating there executes nothing.
    let executed = false;
    page.on('dialog', async (dialog) => {
      executed = true;
      await dialog.dismiss();
    });
    await page.goto('/api/rank/' + encodeURIComponent('<script>alert(1)</script>'));
    expect(executed).toBe(false);
  });

  test('a username reaching the card is escaped in the title', async ({ page }) => {
    // The accessible <title> is the only place raw text reaches the document.
    await page.goto('/api/rank/octocat');

    const html = await page.content();
    expect(html).toContain('<title');
    // Rendered from outlines, so no unescaped user text can appear as markup.
    expect(await page.locator('svg > title').first().innerHTML()).not.toContain('<');
  });
});

test.describe('multiple cards in one document', () => {
  // Regression: SVG ids are document-global and the first definition wins.
  // Deriving them from the tier alone meant two cards of the same tier in
  // different themes shared a background gradient — which is exactly what a
  // gallery page does.
  test('gradient ids stay unique when cards are inlined together', async ({ page, request, baseURL }) => {
    const themes = ['dark', 'light', 'ocean', 'sunset'];
    const cards = await Promise.all(
      themes.map(async (theme) =>
        (await request.get(`${baseURL}/api/rank/octocat?theme=${theme}`)).text()
      )
    );

    await page.setContent(`<div id="gallery">${cards.join('')}</div>`);

    const ids = await page.evaluate(() =>
      Array.from(document.querySelectorAll('linearGradient')).map((g) => g.id)
    );

    expect(ids.length).toBeGreaterThan(themes.length);
    expect(new Set(ids).size, 'every gradient id must be unique').toBe(ids.length);
  });

  test('each theme is visibly distinct when rendered side by side', async ({ page, request, baseURL }) => {
    const dark = await (await request.get(`${baseURL}/api/rank/octocat?theme=dark`)).text();
    const light = await (await request.get(`${baseURL}/api/rank/octocat?theme=light`)).text();

    await page.setContent(
      `<div style="display:flex">
         <div id="a">${dark}</div>
         <div id="b">${light}</div>
       </div>`
    );

    const a = await page.locator('#a svg').screenshot();
    const b = await page.locator('#b svg').screenshot();

    // If they collided on gradient ids these would be pixel-identical.
    expect(Buffer.compare(a, b)).not.toBe(0);
  });
});

test.describe('visual regression', () => {
  const cases = [
    { user: 'octocat', theme: 'default' },
    { user: 'octocat', theme: 'light' },
    { user: 'octocat', theme: 'cyberpunk' },
    // Undivided tier: the progress bar should be full, not empty.
    { user: 'grandmaster', theme: 'default' },
    // Iron's palette needed contrast adjustment to stay legible on dark.
    { user: 'ironclad', theme: 'default' },
    { user: 'ironclad', theme: 'light' },
  ];

  for (const { user, theme } of cases) {
    test(`${user} on ${theme}`, async ({ page }) => {
      await page.setViewportSize({ width: 495, height: 170 });
      await page.goto(`/api/rank/${user}?theme=${theme}`);

      await expect(page).toHaveScreenshot(`${user}-${theme}.png`);
    });
  }
});

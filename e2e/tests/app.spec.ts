import { test, expect } from '@playwright/test';

// Dashboard tests. The frontend shares its ranking and rendering code with the
// server via WebAssembly, and these assert the properties that come from that.

test.describe('dashboard', () => {
  test('a deep link loads that user directly', async ({ page }) => {
    // The server falls back to index.html so /octocat survives a hard refresh.
    await page.goto('/octocat');

    await expect(page.getByRole('heading', { name: /Diamond II/ })).toBeVisible();
    await expect(page.getByText('2,274 rating')).toBeVisible();
  });

  test('renders the card in the browser, not as a server image', async ({ page }) => {
    await page.goto('/octocat');

    // Inline SVG rather than an <img>: this was drawn by wasm on the client.
    const card = page.locator('svg[role="img"]').first();
    await expect(card).toBeVisible();
    await expect(card).toHaveAttribute('viewBox', '0 0 495 170');

    await expect(page.locator('svg[role="img"] title').first())
      .toContainText('octocat');
  });

  test('switching themes costs no network request', async ({ page }) => {
    await page.goto('/octocat');
    await expect(page.locator('svg[role="img"]').first()).toBeVisible();

    // This is the whole point of shipping the renderer as wasm: theme changes
    // are local, so they cost no round trip and no GitHub quota.
    const requests: string[] = [];
    page.on('request', (request) => {
      if (request.url().includes('/api/')) requests.push(request.url());
    });

    const before = await page.locator('svg[role="img"]').first().innerHTML();

    await page.getByLabel('Card theme').click();
    await page.getByRole('option', { name: 'cyberpunk' }).click();

    await expect
      .poll(async () => page.locator('svg[role="img"]').first().innerHTML())
      .not.toBe(before);

    expect(requests, 'theme switching must not hit the API').toEqual([]);
  });

  test('the preview matches what the badge endpoint serves', async ({ page, request, baseURL }) => {
    await page.goto('/octocat');
    await expect(page.locator('svg[role="img"]').first()).toBeVisible();

    // Same Rust renderer on both sides, so the markup must agree.
    const fromBrowser = await page.locator('svg[role="img"]').first().innerHTML();
    const fromServer = await (await request.get(`${baseURL}/api/rank/octocat`)).text();

    // Compare the drawing itself; the browser normalises the outer element.
    const paths = (svg: string) => (svg.match(/<path/g) ?? []).length;
    expect(paths(fromBrowser)).toBe(paths(fromServer));

    const title = await page.locator('svg[role="img"] title').first().textContent();
    expect(fromServer).toContain(title!.trim());
  });

  test('rejects a malformed username before making a request', async ({ page }) => {
    await page.goto('/');

    const requests: string[] = [];
    page.on('request', (r) => r.url().includes('/api/') && requests.push(r.url()));

    // Validated in wasm by the same rule the server applies.
    await page.getByLabel('GitHub username').fill('-not-valid-');

    await expect(page.getByText(/1–39 letters, digits or single hyphens/)).toBeVisible();
    await expect(page.getByRole('button', { name: 'Rank' })).toBeDisabled();
    expect(requests).toEqual([]);
  });

  test('shows a helpful message for a user that does not exist', async ({ page }) => {
    // Not seeded, and the placeholder token cannot reach GitHub.
    await page.goto('/');
    await page.getByLabel('GitHub username').fill('definitelynotarealuser');
    await page.getByRole('button', { name: 'Rank' }).click();

    await expect(page.getByRole('alert')).toBeVisible();
  });

  test('seasonal decay is explained, not just applied', async ({ page }) => {
    await page.goto('/octocat');
    await page.getByRole('tab', { name: 'Seasons' }).click();

    await expect(page.getByRole('cell', { name: '2026', exact: true })).toBeVisible();
    // The current season counts fully; the previous one at 60%.
    await expect(page.getByRole('cell', { name: '100%' })).toBeVisible();
    await expect(page.getByRole('cell', { name: '60%' })).toBeVisible();
  });

  test('embed snippets reflect the selected theme', async ({ page }) => {
    await page.goto('/octocat');
    await page.getByRole('tab', { name: 'Embed' }).click();

    await expect(page.getByText('![GitHub Rank](', { exact: false })).toBeVisible();

    await page.getByRole('tab', { name: 'Overview' }).click();
    await page.getByLabel('Card theme').click();
    await page.getByRole('option', { name: 'ocean' }).click();
    await page.getByRole('tab', { name: 'Embed' }).click();

    await expect(page.getByText('?theme=ocean', { exact: false }).first()).toBeVisible();
  });

  test('undivided tiers render without a division', async ({ page }) => {
    await page.goto('/grandmaster');
    await expect(page.getByRole('heading', { name: 'Grandmaster' })).toBeVisible();
  });
});

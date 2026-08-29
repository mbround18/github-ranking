import { test, expect } from '@playwright/test';

// These use the `request` fixture rather than a browser: they assert the HTTP
// contract that README badges and the dashboard depend on.

test.describe('badge endpoint', () => {
  test('serves an SVG with the headers a hotlinked badge needs', async ({ request }) => {
    const response = await request.get('/api/rank/octocat');

    expect(response.status()).toBe(200);
    expect(response.headers()['content-type']).toContain('image/svg+xml');

    // GitHub's camo proxy sniffs content types otherwise.
    expect(response.headers()['x-content-type-options']).toBe('nosniff');

    // Badges are hotlinked from arbitrary origins by design.
    expect(response.headers()['access-control-allow-origin']).toBe('*');

    const cacheControl = response.headers()['cache-control'];
    expect(cacheControl).toContain('public');
    expect(cacheControl).toContain('max-age=86400');
    // Lets a cache serve the old badge while we refresh, so an expiring entry
    // never shows a broken image.
    expect(cacheControl).toContain('stale-while-revalidate');

    expect(await response.text()).toContain('</svg>');
  });

  test('reports whether the rank came from cache', async ({ request }) => {
    const response = await request.get('/api/rank/octocat');
    expect(response.headers()['x-cache']).toBe('HIT');
  });

  test('every theme renders and they differ from one another', async ({ request }) => {
    const themes = ['default', 'dark', 'light', 'minimal', 'cyberpunk', 'ocean', 'forest', 'sunset', 'galaxy'];
    const rendered = new Map<string, string>();

    for (const theme of themes) {
      const response = await request.get(`/api/rank/octocat?theme=${theme}`);
      expect(response.status(), `${theme} should render`).toBe(200);
      rendered.set(theme, await response.text());
    }

    expect(new Set(rendered.values()).size).toBe(themes.length);
  });

  test('an unknown theme falls back instead of breaking the image', async ({ request }) => {
    // A typo in someone's README must not replace their badge with an error.
    const response = await request.get('/api/rank/octocat?theme=not-a-theme');

    expect(response.status()).toBe(200);
    expect(response.headers()['content-type']).toContain('image/svg+xml');

    const fallback = await response.text();
    const explicit = await (await request.get('/api/rank/octocat?theme=default')).text();
    expect(fallback).toBe(explicit);
  });

  test('the legacy /badge path still works', async ({ request }) => {
    // READMEs in the wild may use the rewrite the original service exposed.
    const response = await request.get('/badge/octocat');
    expect(response.status()).toBe(200);
    expect(response.headers()['content-type']).toContain('image/svg+xml');
  });

  test('rejects malformed usernames with a structured error', async ({ request }) => {
    const response = await request.get('/api/rank/-not-valid');

    expect(response.status()).toBe(400);
    const body = await response.json();
    expect(body.error).toBe('ValidationError');
    expect(body.code).toBe(400);
    expect(body.requestId).toBeTruthy();
  });

  test('rejects out-of-range seasons', async ({ request }) => {
    const response = await request.get('/api/rank/octocat?season=1999');
    expect(response.status()).toBe(400);
    expect((await response.json()).details.hint).toContain('between');
  });
});

test.describe('json api', () => {
  test('returns the full rank payload in camelCase', async ({ request }) => {
    const response = await request.get('/api/v1/rank/octocat');
    expect(response.status()).toBe(200);

    const body = await response.json();
    expect(body.username).toBe('octocat');
    expect(body.rank).toMatchObject({ tier: 'Diamond', division: 'II', elo: 2274 });
    expect(body.stats.totalMergedPRs).toBe(342);

    // Consistently camelCase — the dashboard shouldn't have to handle two
    // naming conventions.
    expect(body.yearly[0]).toHaveProperty('privateContributions');
    expect(JSON.stringify(body)).not.toMatch(/_[a-z]/);
  });

  test('undivided tiers report a null division', async ({ request }) => {
    const body = await (await request.get('/api/v1/rank/grandmaster')).json();
    expect(body.rank.tier).toBe('Grandmaster');
    expect(body.rank.division).toBeNull();
  });

  test('unknown endpoints return the same error envelope', async ({ request }) => {
    const response = await request.get('/api/nonexistent');

    expect(response.status()).toBe(404);
    const body = await response.json();
    expect(body.error).toBe('NotFound');
    expect(body.requestId).toBeTruthy();
  });
});

test.describe('kubernetes probes', () => {
  test('liveness depends on nothing external', async ({ request }) => {
    const response = await request.get('/healthz');
    expect(response.status()).toBe(200);
    expect((await response.json()).status).toBe('ok');
  });

  test('readiness reports credentials and cache state', async ({ request }) => {
    const body = await (await request.get('/readyz')).json();

    expect(body.status).toBe('ready');
    expect(body.credentials.available).toBeGreaterThan(0);
    expect(body.cache.durable).toBe(true);
  });

  test('startup reports version and uptime', async ({ request }) => {
    const body = await (await request.get('/startupz')).json();

    expect(body.status).toBe('started');
    expect(typeof body.uptimeSeconds).toBe('number');
    expect(body.version).toBeTruthy();
  });

  test('metrics are scrapable by prometheus', async ({ request }) => {
    const response = await request.get('/metrics');

    expect(response.status()).toBe(200);
    expect(response.headers()['content-type']).toContain('text/plain');

    const body = await response.text();
    for (const metric of [
      'github_ranked_requests_total',
      'github_ranked_cache_hits_total',
      'github_ranked_credentials_available',
      'github_ranked_request_duration_seconds_bucket',
    ]) {
      expect(body, `${metric} should be exposed`).toContain(metric);
    }

    // Every series needs both HELP and TYPE or scrapers reject it.
    const help = (body.match(/# HELP /g) ?? []).length;
    const type = (body.match(/# TYPE /g) ?? []).length;
    expect(help).toBe(type);
  });
});

test('responses carry a correlation id', async ({ request }) => {
  const response = await request.get('/healthz', {
    headers: { 'x-request-id': 'e2e-trace-id' },
  });
  // An id from a proxy is adopted so traces join up across hops.
  expect(response.headers()['x-request-id']).toBe('e2e-trace-id');
});

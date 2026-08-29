import { defineConfig, devices } from '@playwright/test';

const PORT = Number(process.env.E2E_PORT ?? 10125);
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : [['list']],

  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
  },

  // Cards are pure vector with embedded outlines, so rendering is deterministic
  // across machines. A tight threshold is meaningful here rather than flaky.
  expect: {
    toHaveScreenshot: { maxDiffPixelRatio: 0.01 },
  },

  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],

  webServer: {
    // Seed first, then serve: the suite must not depend on GitHub being
    // reachable or on a real token.
    command: 'node seed.mjs && ../target/release/github-ranked',
    url: `${BASE_URL}/healthz`,
    reuseExistingServer: !process.env.CI,
    stdout: 'pipe',
    stderr: 'pipe',
    env: {
      PORT: String(PORT),
      HOST: '127.0.0.1',
      APP_ENV: 'development',
      CACHE_PATH: './.tmp/cache.db',
      // The built frontend, so the dashboard is exercised too.
      WEB_ROOT: '../web/dist',
      // Never used — every fixture is pre-seeded — but the server requires a
      // credential to start.
      GITHUB_TOKEN: 'ghp_e2e_placeholder_never_used',
      RUST_LOG: 'info',
    },
  },
});

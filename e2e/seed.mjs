// Seeds the cache so the end-to-end suite is hermetic.
//
// Without this every test would need a real GitHub token and live API calls,
// which makes the suite slow, rate-limited and non-deterministic. Writing
// straight into the durable cache exercises the entire serving path — cache
// read, render, HTTP — with only the GitHub fetch stubbed out.

import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, rmSync } from 'node:fs';
import { dirname } from 'node:path';

const DB_PATH = process.env.CACHE_PATH ?? './.tmp/cache.db';
const TTL_SECONDS = 3600;

/** Profiles chosen to cover divided tiers, undivided tiers, and extremes. */
const FIXTURES = [
  {
    username: 'octocat',
    displayName: 'The Octocat',
    rank: { tier: 'Diamond', division: 'II', elo: 2274, gp: 74, percentile: 99.1, wpi: 48210, zScore: 2.85 },
    stats: { totalMergedPRs: 342, totalCodeReviews: 1287, totalIssuesClosed: 96, totalCommits: 4521, totalStars: 12480 },
  },
  {
    // Undivided tier: the progress bar renders full rather than empty.
    username: 'grandmaster',
    displayName: 'Grand Master',
    rank: { tier: 'Grandmaster', division: null, elo: 2929, gp: 0, percentile: 100, wpi: 512000, zScore: 4.32 },
    stats: { totalMergedPRs: 1200, totalCodeReviews: 4800, totalIssuesClosed: 400, totalCommits: 22000, totalStars: 180000 },
  },
  {
    // Bottom of the ladder, and the tier whose palette needed contrast fixing.
    username: 'ironclad',
    displayName: null,
    rank: { tier: 'Iron', division: 'IV', elo: 120, gp: 80, percentile: 1.2, wpi: 1, zScore: -4.33 },
    stats: { totalMergedPRs: 0, totalCodeReviews: 0, totalIssuesClosed: 0, totalCommits: 2, totalStars: 0 },
  },
];

const now = Math.floor(Date.now() / 1000);

rmSync(DB_PATH, { force: true });
rmSync(`${DB_PATH}-wal`, { force: true });
rmSync(`${DB_PATH}-shm`, { force: true });
mkdirSync(dirname(DB_PATH), { recursive: true });

const db = new DatabaseSync(DB_PATH);
db.exec(`CREATE TABLE IF NOT EXISTS cache_entries (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL,
           expires_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_cache_expires ON cache_entries (expires_at);`);

const insert = db.prepare(
  `INSERT OR REPLACE INTO cache_entries (key, value, expires_at) VALUES (?, ?, ?)`
);

for (const fixture of FIXTURES) {
  const payload = {
    username: fixture.username,
    displayName: fixture.displayName,
    rank: fixture.rank,
    stats: {
      totalFollowers: 0,
      firstContributionYear: 2019,
      lastContributionYear: 2026,
      yearsActive: 8,
      ...fixture.stats,
    },
    yearly: [
      { year: 2026, commits: 800, prs: 60, reviews: 200, issues: 12, privateContributions: 30 },
      { year: 2025, commits: 640, prs: 44, reviews: 150, issues: 9, privateContributions: 12 },
    ],
    season: null,
    computedAt: now,
  };

  const expiresAt = now + TTL_SECONDS;
  // Matches the server's two-layer Entry envelope.
  const entry = JSON.stringify({ value: JSON.stringify(payload), expires_at: expiresAt });

  insert.run(`rank:${fixture.username}:all:public`, entry, expiresAt);
}

db.close();
console.log(`seeded ${FIXTURES.length} cached ranks into ${DB_PATH}`);

/** Shapes returned by `GET /api/v1/rank/{username}`. */

export type Tier =
  | "Iron" | "Bronze" | "Silver" | "Gold" | "Platinum"
  | "Emerald" | "Diamond" | "Master" | "Grandmaster" | "Challenger";

export type Division = "IV" | "III" | "II" | "I";

export interface RankResult {
  tier: Tier;
  /** Null for Master and above, which have no divisions. */
  division: Division | null;
  elo: number;
  gp: number;
  percentile: number;
  wpi: number;
  zScore: number;
}

export interface AggregatedStats {
  totalMergedPRs: number;
  totalCodeReviews: number;
  totalIssuesClosed: number;
  totalCommits: number;
  totalStars: number;
  totalFollowers: number;
  firstContributionYear: number;
  lastContributionYear: number;
  yearsActive: number;
}

/** One year of raw, undecayed contributions. */
export interface YearlyStats {
  year: number;
  commits: number;
  prs: number;
  reviews: number;
  issues: number;
  /** Counted for display only — never scored, so badges stay reproducible. */
  privateContributions: number;
}

export interface RankPayload {
  username: string;
  displayName: string | null;
  rank: RankResult;
  stats: AggregatedStats;
  yearly: YearlyStats[];
  season: number | null;
  computedAt: number;
}

export interface ApiError {
  error: string;
  code: number;
  message: string;
  details?: Record<string, unknown>;
  requestId: string;
  retryAfter?: number;
}

export const THEMES = [
  "default", "dark", "light", "minimal", "cyberpunk",
  "ocean", "forest", "sunset", "galaxy",
] as const;

export type Theme = (typeof THEMES)[number];

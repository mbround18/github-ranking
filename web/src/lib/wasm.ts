/**
 * The ranking engine and card renderer, compiled to WebAssembly.
 *
 * This is the *same* Rust code the server runs. Rendering a preview here is
 * byte-identical to the badge the API will serve, so switching themes costs no
 * round trip and no GitHub quota — and the preview can never drift from the
 * real thing.
 */

import init, {
  calculateRank as wasmCalculateRank,
  isValidUsername as wasmIsValidUsername,
  nextTierAt as wasmNextTierAt,
  renderCard as wasmRenderCard,
  seasonalDecay as wasmSeasonalDecay,
  themeNames as wasmThemeNames,
} from "@/wasm/github_ranked";
import type { AggregatedStats, RankPayload, RankResult } from "./types";

let loading: Promise<unknown> | null = null;

/** Load the wasm module. Safe to call repeatedly; the work happens once. */
export function loadEngine(): Promise<unknown> {
  loading ??= init();
  return loading;
}

/** Render a rank card to an SVG string. Requires {@link loadEngine} first. */
export function renderCard(
  payload: Pick<RankPayload, "username" | "rank" | "stats" | "season">,
  theme: string,
  currentYear: number = new Date().getUTCFullYear(),
): string {
  return wasmRenderCard({ ...payload, theme, currentYear });
}

/** Recompute a rank locally, for exploring "what if" without a fetch. */
export function calculateRank(stats: AggregatedStats): RankResult {
  return wasmCalculateRank(stats) as RankResult;
}

/** The Elo needed to reach the next tier, or undefined at Challenger. */
export function nextTierAt(elo: number): number | undefined {
  return wasmNextTierAt(elo);
}

/**
 * The weight a contribution year still carries.
 *
 * From the engine rather than reimplemented here, so the weights shown next to
 * the raw counts cannot drift from the scores the server computes.
 */
export function seasonalDecay(year: number, currentYear: number): number {
  return wasmSeasonalDecay(year, currentYear);
}

/** Validate before spending a round trip. The server validates again anyway. */
export function isValidUsername(username: string): boolean {
  return wasmIsValidUsername(username);
}

export function themeNames(): string[] {
  return wasmThemeNames();
}

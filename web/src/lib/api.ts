import type { ApiError, RankPayload } from "./types";

/** An error carrying the server's structured detail, including its request id. */
export class RankError extends Error {
  // Declared explicitly rather than as constructor parameter properties, which
  // `erasableSyntaxOnly` disallows.
  readonly status: number;
  readonly body: ApiError | null;

  constructor(status: number, body: ApiError | null) {
    super(body?.message ?? `Request failed with status ${status}`);
    this.name = "RankError";
    this.status = status;
    this.body = body;
  }

  /** True when the username simply doesn't exist, which isn't worth an alarming UI. */
  get isNotFound() {
    return this.status === 404;
  }

  get isRateLimited() {
    return this.status === 429;
  }
}

export async function fetchRank(
  username: string,
  options: { season?: number | null; force?: boolean; signal?: AbortSignal } = {},
): Promise<RankPayload> {
  const params = new URLSearchParams();
  if (options.season) params.set("season", String(options.season));
  if (options.force) params.set("force", "true");

  const query = params.toString();
  const response = await fetch(
    `/api/v1/rank/${encodeURIComponent(username)}${query ? `?${query}` : ""}`,
    { signal: options.signal, headers: { accept: "application/json" } },
  );

  if (!response.ok) {
    // The error envelope is consistent across every endpoint, but don't assume
    // it parsed — a proxy could have returned something else entirely.
    const body = await response.json().catch(() => null);
    throw new RankError(response.status, body);
  }

  return response.json();
}

/** The badge URL to embed in a README. */
export function badgeUrl(username: string, theme: string, origin = window.location.origin) {
  const suffix = theme === "default" ? "" : `?theme=${theme}`;
  return `${origin}/api/rank/${username}${suffix}`;
}

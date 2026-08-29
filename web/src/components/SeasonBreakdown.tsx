import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { seasonalDecay } from "@/lib/wasm";
import type { YearlyStats } from "@/lib/types";

interface Props {
  yearly: YearlyStats[];
  /** Weights come from the wasm engine, so they need it loaded. */
  engineReady: boolean;
}

export function SeasonBreakdown({ yearly, engineReady }: Props) {
  const currentYear = new Date().getUTCFullYear();
  const seasons = [...yearly].sort((a, b) => b.year - a.year);

  if (seasons.length === 0) {
    return (
      <Card>
        <CardContent className="py-10 text-center text-sm text-muted-foreground">
          No recorded contributions.
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Seasonal decay</CardTitle>
        <CardDescription>
          Older contributions count for less, so a rank reflects recent activity.
          These are raw counts — the multiplier is what the score sees.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b text-left text-xs text-muted-foreground">
                <th className="pb-2 font-medium">Season</th>
                <th className="pb-2 text-right font-medium">Weight</th>
                <th className="pb-2 text-right font-medium">PRs</th>
                <th className="pb-2 text-right font-medium">Reviews</th>
                <th className="pb-2 text-right font-medium">Issues</th>
                <th className="pb-2 text-right font-medium">Commits</th>
                <th className="pb-2 text-right font-medium">Private</th>
              </tr>
            </thead>
            <tbody>
              {seasons.map((season) => {
                const weight = engineReady
                  ? seasonalDecay(season.year, currentYear)
                  : null;
                return (
                  <tr key={season.year} className="border-b last:border-0">
                    <td className="py-2 font-medium tabular-nums">{season.year}</td>
                    <td
                      className="py-2 text-right tabular-nums"
                      // Fade the row weight so a glance shows what still counts.
                      style={weight === null ? undefined : { opacity: 0.4 + weight * 0.6 }}
                    >
                      {weight === null ? "—" : `${Math.round(weight * 100)}%`}
                    </td>
                    <td className="py-2 text-right tabular-nums">{season.prs.toLocaleString()}</td>
                    <td className="py-2 text-right tabular-nums">{season.reviews.toLocaleString()}</td>
                    <td className="py-2 text-right tabular-nums">{season.issues.toLocaleString()}</td>
                    <td className="py-2 text-right tabular-nums">{season.commits.toLocaleString()}</td>
                    <td className="py-2 text-right tabular-nums text-muted-foreground">
                      {season.privateContributions.toLocaleString()}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
        <p className="mt-4 text-xs text-muted-foreground">
          Private contributions are shown for context but never scored, so any
          badge stays reproducible by anyone.
        </p>
      </CardContent>
    </Card>
  );
}

import { Card, CardContent } from "@/components/ui/card";
import type { AggregatedStats } from "@/lib/types";

const METRICS = [
  { key: "totalMergedPRs", label: "Pull requests", weight: "35%", accent: "text-sky-400" },
  { key: "totalCodeReviews", label: "Code reviews", weight: "35%", accent: "text-violet-400" },
  { key: "totalIssuesClosed", label: "Issues", weight: "15%", accent: "text-amber-400" },
  { key: "totalCommits", label: "Commits", weight: "10%", accent: "text-emerald-400" },
  { key: "totalStars", label: "Stars", weight: "5%", accent: "text-orange-400" },
] as const;

export function StatsGrid({ stats }: { stats: AggregatedStats }) {
  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
      {METRICS.map((metric) => (
        <Card key={metric.key}>
          <CardContent className="space-y-1 p-4">
            <div className="flex items-baseline justify-between gap-2">
              <span className="text-xs text-muted-foreground">{metric.label}</span>
              <span className="text-[10px] tabular-nums text-muted-foreground/60">
                {metric.weight}
              </span>
            </div>
            <div className={`text-2xl font-semibold tabular-nums ${metric.accent}`}>
              {stats[metric.key].toLocaleString()}
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

import { useCallback, useEffect, useState } from "react";
import { ArrowRight, Loader2, RefreshCw, Search } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { EmbedCode } from "@/components/EmbedCode";
import { RankCard } from "@/components/RankCard";
import { SeasonBreakdown } from "@/components/SeasonBreakdown";
import { StatsGrid } from "@/components/StatsGrid";
import { fetchRank, RankError } from "@/lib/api";
import { THEMES, type RankPayload } from "@/lib/types";
import { isValidUsername, loadEngine, nextTierAt } from "@/lib/wasm";

/** The username is the path, so a card can be linked and shared directly. */
function usernameFromPath(): string {
  return decodeURIComponent(window.location.pathname.replace(/^\/+|\/+$/g, ""));
}

export default function App() {
  const [engineReady, setEngineReady] = useState(false);
  const [query, setQuery] = useState(usernameFromPath);
  const [payload, setPayload] = useState<RankPayload | null>(null);
  const [error, setError] = useState<RankError | Error | null>(null);
  const [loading, setLoading] = useState(false);
  const [theme, setTheme] = useState("default");

  useEffect(() => {
    loadEngine()
      .then(() => setEngineReady(true))
      .catch((cause) => setError(cause instanceof Error ? cause : new Error(String(cause))));
  }, []);

  const load = useCallback(async (username: string, force = false) => {
    const trimmed = username.trim();
    if (!trimmed) return;

    setLoading(true);
    setError(null);
    try {
      setPayload(await fetchRank(trimmed, { force }));
    } catch (cause) {
      setPayload(null);
      setError(cause instanceof Error ? cause : new Error(String(cause)));
    } finally {
      setLoading(false);
    }
  }, []);

  // Deep links and the back button both go through the same path.
  useEffect(() => {
    const initial = usernameFromPath();
    if (initial) void load(initial);

    const onPopState = () => {
      const name = usernameFromPath();
      setQuery(name);
      if (name) void load(name);
      else setPayload(null);
    };

    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, [load]);

  function submit(event: React.FormEvent) {
    event.preventDefault();
    const trimmed = query.trim();
    if (!trimmed) return;

    window.history.pushState({}, "", `/${encodeURIComponent(trimmed)}`);
    void load(trimmed);
  }

  // Validated in wasm by the same rule the server applies, so the UI can reject
  // a typo before spending a round trip.
  const queryIsValid = !engineReady || query.trim() === "" || isValidUsername(query.trim());

  return (
    <div className="min-h-screen bg-background text-foreground">
      <div className="mx-auto w-full max-w-5xl px-4 py-10 sm:py-16">
        <header className="space-y-3">
          <div className="flex items-center gap-3">
            <h1 className="text-2xl font-semibold tracking-tight sm:text-3xl">
              GitHub Ranked
            </h1>
            <Badge variant="secondary" className="font-mono text-[10px]">
              rust + wasm
            </Badge>
          </div>
          <p className="max-w-2xl text-sm text-muted-foreground">
            Competitive skill ratings from GitHub contributions. Collaboration is
            weighted heavily, and older seasons decay, so a rank reflects recent
            work rather than a lifetime total.
          </p>
        </header>

        <form onSubmit={submit} className="mt-8 flex flex-col gap-3 sm:flex-row">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="GitHub username"
              aria-label="GitHub username"
              aria-invalid={!queryIsValid}
              className="pl-9"
              autoComplete="off"
              spellCheck={false}
            />
          </div>
          <Button type="submit" disabled={loading || !queryIsValid || !query.trim()}>
            {loading ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <ArrowRight className="size-4" />
            )}
            Rank
          </Button>
        </form>

        {!queryIsValid && (
          <p className="mt-2 text-xs text-destructive">
            GitHub usernames are 1–39 letters, digits or single hyphens.
          </p>
        )}

        {error && <ErrorNotice error={error} username={query.trim()} />}

        {loading && !payload && <LoadingState />}

        {payload && (
          <Result
            payload={payload}
            theme={theme}
            onThemeChange={setTheme}
            engineReady={engineReady}
            onRefresh={() => void load(payload.username, true)}
            refreshing={loading}
          />
        )}

        {!payload && !loading && !error && <EmptyState />}
      </div>
    </div>
  );
}

function Result({
  payload, theme, onThemeChange, engineReady, onRefresh, refreshing,
}: {
  payload: RankPayload;
  theme: string;
  onThemeChange: (theme: string) => void;
  engineReady: boolean;
  onRefresh: () => void;
  refreshing: boolean;
}) {
  const { rank } = payload;
  const label = rank.division ? `${rank.tier} ${rank.division}` : rank.tier;
  const nextAt = engineReady ? nextTierAt(rank.elo) : undefined;

  return (
    <section className="mt-10 space-y-8">
      <div className="flex flex-col items-start justify-between gap-4 sm:flex-row sm:items-center">
        <div>
          <div className="flex items-baseline gap-3">
            <h2 className="text-xl font-semibold">{label}</h2>
            <span className="font-mono text-sm text-muted-foreground">
              {rank.elo.toLocaleString()} rating
            </span>
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            {payload.displayName ? `${payload.displayName} · ` : ""}
            top {(100 - rank.percentile).toFixed(1)}%
            {nextAt !== undefined &&
              ` · ${Math.max(0, Math.ceil(nextAt - rank.elo)).toLocaleString()} to the next tier`}
          </p>
        </div>

        <div className="flex items-center gap-2">
          {/* The primitive can emit null on clear; the card always needs a theme. */}
          <Select value={theme} onValueChange={(value) => value && onThemeChange(value)}>
            <SelectTrigger className="w-[150px]" aria-label="Card theme">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {THEMES.map((name) => (
                <SelectItem key={name} value={name} className="capitalize">
                  {name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant="outline"
            size="icon"
            onClick={onRefresh}
            disabled={refreshing}
            aria-label="Refresh from GitHub"
          >
            <RefreshCw className={`size-4 ${refreshing ? "animate-spin" : ""}`} />
          </Button>
        </div>
      </div>

      <div className="flex justify-center rounded-xl border bg-muted/30 p-4 sm:p-8">
        <RankCard payload={payload} theme={theme} engineReady={engineReady} />
      </div>

      <p className="text-center text-xs text-muted-foreground">
        Rendered in your browser by the same engine the badge endpoint uses, so
        switching themes costs no request.
      </p>

      <Separator />

      <Tabs defaultValue="overview">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="seasons">Seasons</TabsTrigger>
          <TabsTrigger value="embed">Embed</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="mt-6">
          <StatsGrid stats={payload.stats} />
        </TabsContent>

        <TabsContent value="seasons" className="mt-6">
          <SeasonBreakdown yearly={payload.yearly} />
        </TabsContent>

        <TabsContent value="embed" className="mt-6">
          <EmbedCode username={payload.username} theme={theme} />
        </TabsContent>
      </Tabs>
    </section>
  );
}

function ErrorNotice({ error, username }: { error: Error; username: string }) {
  const rankError = error instanceof RankError ? error : null;

  const title = rankError?.isNotFound
    ? "No such user"
    : rankError?.isRateLimited
      ? "Rate limited"
      : "Something went wrong";

  const description = rankError?.isNotFound
    ? `GitHub has no account named "${username}".`
    : rankError?.isRateLimited
      ? "The GitHub API quota is exhausted. Cached ranks still work; try again shortly."
      : error.message;

  return (
    <Alert variant={rankError?.isNotFound ? "default" : "destructive"} className="mt-6">
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription className="space-y-1">
        <span>{description}</span>
        {/* Surfaced so a bug report can be traced to the exact request. */}
        {rankError?.body?.requestId && (
          <span className="font-mono text-[11px] opacity-70">
            request {rankError.body.requestId}
          </span>
        )}
      </AlertDescription>
    </Alert>
  );
}

function LoadingState() {
  return (
    <div className="mt-10 space-y-8">
      <Skeleton className="h-8 w-56" />
      <div className="flex justify-center rounded-xl border bg-muted/30 p-8">
        <Skeleton className="h-[170px] w-[495px] max-w-full rounded-2xl" />
      </div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
        {Array.from({ length: 5 }, (_, index) => (
          <Skeleton key={index} className="h-20 rounded-xl" />
        ))}
      </div>
    </div>
  );
}

function EmptyState() {
  return (
    <Card className="mt-10 border-dashed bg-transparent">
      <CardContent className="py-14 text-center">
        <p className="text-sm text-muted-foreground">
          Enter a GitHub username to see their rank.
        </p>
        <p className="mt-2 text-xs text-muted-foreground/70">
          Ranks are computed from public contributions only, so anyone can
          reproduce them.
        </p>
      </CardContent>
    </Card>
  );
}

import { useEffect, useMemo, useState } from "react";
import { renderCard } from "@/lib/wasm";
import type { RankPayload } from "@/lib/types";
import { Skeleton } from "@/components/ui/skeleton";

interface Props {
  payload: RankPayload;
  theme: string;
  engineReady: boolean;
}

/**
 * The badge, rendered in the browser by the same Rust code the server runs.
 *
 * Switching themes is instant and costs nothing — no request, no GitHub quota —
 * and what you see here is byte-for-byte what the badge endpoint will serve.
 */
export function RankCard({ payload, theme, engineReady }: Props) {
  const [error, setError] = useState<string | null>(null);

  const svg = useMemo(() => {
    if (!engineReady) return null;
    try {
      setError(null);
      return renderCard(payload, theme);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Could not render the card");
      return null;
    }
  }, [payload, theme, engineReady]);

  useEffect(() => {
    if (error) console.error("card render failed:", error);
  }, [error]);

  if (!engineReady) {
    return <Skeleton className="h-[170px] w-[495px] max-w-full rounded-2xl" />;
  }

  if (error || !svg) {
    return (
      <div className="flex h-[170px] w-[495px] max-w-full items-center justify-center rounded-2xl border border-dashed text-sm text-muted-foreground">
        Could not render this card
      </div>
    );
  }

  return (
    <div
      className="w-[495px] max-w-full [&>svg]:h-auto [&>svg]:w-full"
      // The SVG is produced by our own wasm from typed data, not from anything
      // user-authored, and the renderer escapes the one place text reaches the
      // document.
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}

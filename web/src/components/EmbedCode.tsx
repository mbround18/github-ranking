import { useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { badgeUrl } from "@/lib/api";

interface Props {
  username: string;
  theme: string;
}

export function EmbedCode({ username, theme }: Props) {
  const url = badgeUrl(username, theme);
  const snippets = [
    { label: "Markdown", value: `![GitHub Rank](${url})` },
    { label: "HTML", value: `<img src="${url}" alt="GitHub Rank">` },
    { label: "URL", value: url },
  ];

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Add it to your README</CardTitle>
        <CardDescription>
          The badge re-renders on request, so it stays current without you
          touching it again.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {snippets.map((snippet) => (
          <Snippet key={snippet.label} label={snippet.label} value={snippet.value} />
        ))}
      </CardContent>
    </Card>
  );
}

function Snippet({ label, value }: { label: string; value: string }) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      // Revert the affordance rather than leaving a permanent tick.
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard access can be denied; the snippet stays selectable regardless.
      setCopied(false);
    }
  }

  return (
    <div className="space-y-1.5">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap rounded-md bg-muted px-3 py-2 font-mono text-xs">
          {value}
        </code>
        <Button
          size="icon"
          variant="outline"
          onClick={copy}
          aria-label={copied ? `${label} copied` : `Copy ${label}`}
        >
          {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
        </Button>
      </div>
    </div>
  );
}

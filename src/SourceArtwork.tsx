import { useState } from "react";
import { TrackArtwork } from "./TrackArtwork";
import type { SourceEntry } from "./sources";

function thumbnailUrl(value: string | null | undefined) {
  try {
    const url = new URL(value ?? "");
    if (url.protocol === "https:" && !url.username && !url.password &&
      (url.hostname === "i.ytimg.com" || url.hostname.endsWith(".mzstatic.com")))
      return url.href;
  } catch { /* Fall back to saved artwork. */ }
  return undefined;
}

export function SourceArtwork({ entry }: { entry: SourceEntry }) {
  const [failed, setFailed] = useState<string>();
  const url = thumbnailUrl(entry.thumbnail);
  return url && failed !== url ? (
    <img className="source-result-cover" src={url} alt="" loading="lazy"
      referrerPolicy="no-referrer" onError={() => setFailed(url)} />
  ) : (
    <TrackArtwork className="source-result-cover"
      trackId={entry.provider === "local" && !/[/\\]/.test(entry.id) ? entry.id : undefined}
      path={entry.provider === "local" ? entry.id : undefined} />
  );
}

import type { ReactNode } from "react";
import type { View } from "./phases";

type IconName = View | "play" | "previous" | "next" | "volume" | "music";

export function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
  const paths: Record<IconName, ReactNode> = {
    library: (
      <>
        <path d="M4 4v16M8 4v16M13 5l5-1 3 15-5 1z" />
      </>
    ),
    playlists: (
      <>
        <path d="M4 5h16M4 10h10M4 15h8M18 13v8M14 17h8" />
      </>
    ),
    queue: (
      <>
        <path d="M4 5h16M4 10h16M4 15h8M16 14l5 3-5 3z" />
      </>
    ),
    lyrics: (
      <>
        <path d="M5 5h14M5 10h10M5 15h14M5 20h8" />
      </>
    ),
    artwork: (
      <>
        <rect x="3" y="3" width="18" height="18" rx="3" />
        <circle cx="8" cy="8" r="1" />
        <path d="m3 17 5-5 4 4 4-6 5 7" />
      </>
    ),
    sources: (
      <>
        <path d="M7 17H6a4 4 0 0 1-1-8 7 7 0 0 1 13-1 5 5 0 0 1 0 10h-1M12 12v9m-3-3 3 3 3-3" />
      </>
    ),
    equalizer: (
      <>
        <path d="M5 3v7m0 4v7M12 3v11m0 4v3M19 3v3m0 4v11M2 10h6M9 14h6M16 6h6" />
      </>
    ),
    settings: (
      <>
        <path d="M4 6h16M4 12h16M4 18h16" />
        <circle cx="8" cy="6" r="2" />
        <circle cx="16" cy="12" r="2" />
        <circle cx="10" cy="18" r="2" />
      </>
    ),
    phases: (
      <>
        <rect x="4" y="3" width="16" height="18" rx="2" />
        <path d="m7 8 1 1 2-2m2 1h5M7 13h2m3 0h5M7 17h2m3 0h5" />
      </>
    ),
    play: <path d="m9 5 11 7-11 7z" />,
    previous: (
      <>
        <path d="M5 5v14m13-14L7 12l11 7z" />
      </>
    ),
    next: (
      <>
        <path d="M19 5v14M6 5l11 7-11 7z" />
      </>
    ),
    volume: (
      <>
        <path d="M4 9h4l4-4v14l-4-4H4zM16 8a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14" />
      </>
    ),
    music: (
      <>
        <path d="M10 17V5l10-2v12M10 8l10-2" />
        <ellipse cx="6.5" cy="17.5" rx="3.5" ry="2.5" />
        <ellipse cx="16.5" cy="15.5" rx="3.5" ry="2.5" />
      </>
    ),
  };

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name]}
    </svg>
  );
}

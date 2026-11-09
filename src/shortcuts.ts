import { useEffect, useLayoutEffect, useRef } from "react";

/** Action produced by a window-level keyboard shortcut. */
export type ShortcutAction =
  "toggle" | "next" | "previous" | "seek-forward" | "seek-backward" | "mute";

/**
 * The subset of `KeyboardEvent` needed to resolve a shortcut. Keeping it
 * structural makes the mapping testable without a DOM implementation.
 */
export interface ShortcutEvent {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  repeat?: boolean;
  defaultPrevented?: boolean;
  target?: {
    tagName?: string;
    isContentEditable?: boolean;
  } | null;
}

const EDITABLE_TAGS = new Set(["input", "textarea", "select"]);
const ACTIVATABLE_TAGS = new Set(["button", "a", "summary"]);

/**
 * Maps a keyboard event to a playback action.
 *
 * Text fields keep their native editing keys and interactive elements keep
 * their own activation behaviour, so the global shortcuts never steal Space
 * from a focused button or arrow keys from a slider.
 */
export function resolveShortcut(event: ShortcutEvent): ShortcutAction | null {
  if (event.defaultPrevented || event.repeat) return null;

  const tag = event.target?.tagName?.toLowerCase();
  if (tag && (EDITABLE_TAGS.has(tag) || ACTIVATABLE_TAGS.has(tag))) return null;
  if (event.target?.isContentEditable) return null;

  const primary = event.ctrlKey || event.metaKey;
  if (primary && !event.altKey) {
    switch (event.key.toLowerCase()) {
      case "arrowright":
        return "next";
      case "arrowleft":
        return "previous";
      case "m":
        return "mute";
      default:
        return null;
    }
  }

  if (event.altKey || event.shiftKey) return null;
  switch (event.key) {
    case " ":
    case "Spacebar":
      return "toggle";
    case "ArrowRight":
      return "seek-forward";
    case "ArrowLeft":
      return "seek-backward";
    case "m":
    case "M":
      return "mute";
    default:
      return null;
  }
}

export interface ShortcutHandlers {
  toggle: () => void;
  next: () => void;
  previous: () => void;
  seek: (positionMs: number) => void;
  setMuted: (muted: boolean) => void;
  muted: boolean;
  positionMs: number;
  durationMs: number | null;
}

const SEEK_STEP_MS = 5_000;

function shortcutEvent(event: KeyboardEvent): ShortcutEvent {
  const target = event.target as HTMLElement | null;
  return {
    key: event.key,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    altKey: event.altKey,
    shiftKey: event.shiftKey,
    repeat: event.repeat,
    defaultPrevented: event.defaultPrevented,
    target: target
      ? {
          tagName: target.tagName,
          isContentEditable: target.isContentEditable,
        }
      : null,
  };
}

/** Installs window-level playback shortcuts while `enabled` is true. */
export function useGlobalShortcuts(
  handlers: ShortcutHandlers,
  enabled: boolean,
): void {
  const {
    toggle,
    next,
    previous,
    seek,
    setMuted,
    muted,
    positionMs,
    durationMs,
  } = handlers;

  const onKeyDown = useRef<(event: KeyboardEvent) => void>(() => {});
  useLayoutEffect(() => {
    onKeyDown.current = (event: KeyboardEvent) => {
      const action = resolveShortcut(shortcutEvent(event));
      if (!action) return;
      event.preventDefault();
      const limit = durationMs ?? Number.POSITIVE_INFINITY;
      switch (action) {
        case "toggle":
          toggle();
          break;
        case "next":
          next();
          break;
        case "previous":
          previous();
          break;
        case "seek-forward":
          seek(Math.min(positionMs + SEEK_STEP_MS, limit));
          break;
        case "seek-backward":
          seek(Math.max(positionMs - SEEK_STEP_MS, 0));
          break;
        case "mute":
          setMuted(!muted);
          break;
      }
    };
  });
  useEffect(() => {
    if (!enabled) return;
    const listener = (event: KeyboardEvent) => onKeyDown.current(event);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [enabled]);
}

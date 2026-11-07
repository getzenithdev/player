import { useSyncExternalStore } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const subscribers = new Set<() => void>();
let visible = true;
let focused = true;
let stop: (() => void) | undefined;

function subscribe(callback: () => void) {
  subscribers.add(callback);
  if (!stop) {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    focused = document.hasFocus();
    const update = () => {
      const next = !document.hidden && focused;
      if (visible === next) return;
      visible = next;
      subscribers.forEach((notify) => notify());
    };
    const focus = () => { focused = true; update(); };
    const blur = () => { focused = false; update(); };
    document.addEventListener("visibilitychange", update);
    window.addEventListener("focus", focus);
    window.addEventListener("blur", blur);
    if (isTauri()) {
      void listen<boolean>("artwork:visibility", ({ payload }) => {
        if (!disposed) { focused = payload; update(); }
      }).then((cleanup) => { if (disposed) cleanup(); else unlisten = cleanup; }).catch(() => {});
    }
    update();
    stop = () => {
      disposed = true;
      unlisten?.();
      document.removeEventListener("visibilitychange", update);
      window.removeEventListener("focus", focus);
      window.removeEventListener("blur", blur);
    };
  }
  return () => {
    subscribers.delete(callback);
    if (!subscribers.size) { stop?.(); stop = undefined; }
  };
}

// One shared subscription: paused/background artwork releases its media
// without pausing the music or installing listeners for every thumbnail.
export function useAppVisibility() {
  return useSyncExternalStore(subscribe, () => visible, () => false);
}

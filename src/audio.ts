import type { MediaEntry, QueueAction } from "./playlists";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";

export type PlaybackStatus =
  "idle" | "loading" | "playing" | "paused" | "stopped" | "error";

export type RepeatMode = "off" | "all" | "one";

export type TrackSummary = MediaEntry;

export interface PlaybackSnapshot {
  status: PlaybackStatus;
  track: TrackSummary | null;
  positionMs: number;
  durationMs: number | null;
  sampleRate: number | null;
  channels: number | null;
  format: string | null;
  volume: number;
  muted: boolean;
  repeat: RepeatMode;
  shuffle: boolean;
  queue: TrackSummary[];
  currentIndex: number | null;
  error: string | null;
  outputSampleRate: number | null;
  outputChannels: number | null;
}

export interface AudioCapabilities {
  decoder: string;
  output: string;
  formats: string[];
  unsupported: string[];
}

interface PositionEvent {
  positionMs: number;
  durationMs: number | null;
  status: PlaybackStatus;
}

export const emptySnapshot: PlaybackSnapshot = {
  status: "idle",
  track: null,
  positionMs: 0,
  durationMs: null,
  sampleRate: null,
  channels: null,
  format: null,
  volume: 0.7,
  muted: false,
  repeat: "off",
  shuffle: false,
  queue: [],
  currentIndex: null,
  error: null,
  outputSampleRate: null,
  outputChannels: null,
};

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function pickAudioFiles(): Promise<string[]> {
  const selection = await open({
    title: "Abrir archivos de audio",
    multiple: true,
    directory: false,
    filters: [
      {
        name: "Audio",
        extensions: [
          "mp3",
          "flac",
          "wav",
          "ogg",
          "oga",
          "aiff",
          "aif",
          "aifc",
          "m4a",
          "mp4",
          "aac",
          "alac",
          "caf",
        ],
      },
    ],
  });

  if (selection === null) return [];
  return Array.isArray(selection) ? selection : [selection];
}

export function useAudioEngine() {
  const [available] = useState(isTauriRuntime);
  const [snapshot, setSnapshot] = useState<PlaybackSnapshot | null>(null);
  const [capabilities, setCapabilities] = useState<AudioCapabilities | null>(
    null,
  );
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!available) return;

    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const attach = (unlisten: UnlistenFn) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    };

    void (async () => {
      try {
        const [current, details] = await Promise.all([
          invoke<PlaybackSnapshot>("audio_get_state"),
          invoke<AudioCapabilities>("audio_capabilities"),
        ]);
        if (disposed) return;
        setSnapshot(current);
        setCapabilities(details);
        attach(
          await listen<string>("audio:persistence-error", (event) =>
            setError(event.payload),
          ),
        );
        attach(
          await listen<string>("audio:notice", (event) =>
            setNotice(event.payload),
          ),
        );
        attach(
          await listen<PlaybackSnapshot>("audio:state", (event) => {
            setSnapshot(event.payload);
          }),
        );
        attach(
          await listen<PositionEvent>("audio:position", (event) => {
            setSnapshot((previous) =>
              previous
                ? {
                    ...previous,
                    positionMs: event.payload.positionMs,
                    durationMs: event.payload.durationMs ?? previous.durationMs,
                  }
                : previous,
            );
          }),
        );
      } catch (reason) {
        if (!disposed) setError(String(reason));
      }
    })();

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  }, [available]);

  const run = useCallback(
    async (command: string, args?: Record<string, unknown>) => {
      try {
        await invoke(command, args);
        setError(null);
      } catch (reason) {
        setError(String(reason));
      }
    },
    [],
  );

  const reportError = useCallback((reason: unknown) => {
    setError(String(reason));
  }, []);
  const openFiles = useCallback(
    (paths: string[], play = true) => run("audio_open_files", { paths, play }),
    [run],
  );
  const openEntries = useCallback(
    (entries: MediaEntry[], play = true) =>
      run("audio_open_entries", { entries, play }),
    [run],
  );
  const queueAction = useCallback(
    (action: QueueAction) => run("audio_queue_action", { action }),
    [run],
  );
  const playIndex = useCallback(
    (index: number) => run("audio_play_index", { index }),
    [run],
  );
  const play = useCallback(() => run("audio_play"), [run]);
  const pause = useCallback(() => run("audio_pause"), [run]);
  const toggle = useCallback(() => run("audio_toggle"), [run]);
  const stop = useCallback(() => run("audio_stop"), [run]);
  const next = useCallback(() => run("audio_next"), [run]);
  const previous = useCallback(() => run("audio_previous"), [run]);
  const seek = useCallback(
    (positionMs: number) => run("audio_seek", { positionMs }),
    [run],
  );
  const setVolume = useCallback(
    (volume: number) => run("audio_set_volume", { volume }),
    [run],
  );
  const setMuted = useCallback(
    (muted: boolean) => run("audio_set_muted", { muted }),
    [run],
  );
  const setRepeat = useCallback(
    (mode: RepeatMode) => run("audio_set_repeat", { mode }),
    [run],
  );
  const setShuffle = useCallback(
    (shuffle: boolean) => run("audio_set_shuffle", { shuffle }),
    [run],
  );
  const restartOutput = useCallback(() => run("audio_restart_output"), [run]);
  const clearQueue = useCallback(() => run("audio_clear_queue"), [run]);

  return {
    available,
    state: snapshot ?? emptySnapshot,
    capabilities,
    error,
    reportError,
    openFiles,
    openEntries,
    queueAction,
    notice,
    playIndex,
    play,
    pause,
    toggle,
    stop,
    next,
    previous,
    seek,
    setVolume,
    setMuted,
    setRepeat,
    setShuffle,
    restartOutput,
    clearQueue,
  };
}

export type AudioController = ReturnType<typeof useAudioEngine>;

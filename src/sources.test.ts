import { expect, it } from "vitest";
import {
  canConnect,
  canDisconnect,
  canQuery,
  emptySources,
  entryActions,
  fieldKind,
  fieldLabel,
  formatSize,
  hasCapability,
  stateHint,
  type ProviderStatus,
  type SourceState,
} from "./sources";

function provider(patch: Partial<ProviderStatus> = {}): ProviderStatus {
  return {
    id: "webdav",
    name: "WebDAV",
    auth: "password",
    capabilities: ["auth", "search", "read", "playback", "download"],
    required: ["endpoint", "username", "password"],
    note: "HTTPS",
    state: "disconnected",
    label: "",
    endpoint: "",
    expires_at: null,
    last_error: null,
    has_secret: false,
    settings: { client_id: "", endpoint: "", username: "" },
    ...patch,
  };
}

it("masks secrets and labels every connection field", () => {
  expect(fieldKind("password")).toBe("password");
  expect(fieldKind("api_key")).toBe("password");
  expect(fieldKind("client_secret")).toBe("password");
  expect(fieldKind("endpoint")).toBe("url");
  expect(fieldKind("client_id")).toBe("text");
  expect(fieldLabel("client_id")).toBe("ID de cliente");
  expect(fieldLabel("desconocido")).toBe("desconocido");
});

it("lets expired and failed connections forget credentials", () => {
  expect(canDisconnect(provider({ state: "expired", has_secret: true }))).toBe(
    true,
  );
  expect(canDisconnect(provider({ state: "error", has_secret: true }))).toBe(
    true,
  );
  expect(canDisconnect(provider({ state: "pending" }))).toBe(true);
  expect(canDisconnect(provider())).toBe(false);
  expect(canDisconnect(provider({ id: "local", state: "connected" }))).toBe(
    false,
  );
});

it("opens folders without trying to download or play them", () => {
  const entry = {
    provider: "webdav" as const,
    id: "music",
    name: "Music",
    kind: "folder",
    detail: "",
    size: null,
    source_url: null,
  };
  expect(entryActions(provider(), entry)).toEqual({
    play: false,
    download: false,
  });
  expect(entryActions(provider(), { ...entry, kind: "audio" })).toEqual({
    play: true,
    download: true,
  });
  expect(entryActions(provider(), { ...entry, kind: "file" })).toEqual({
    play: false,
    download: true,
  });
  expect(
    entryActions(provider({ capabilities: ["search", "read"] }), {
      ...entry,
      kind: "audio",
    }),
  ).toEqual({ play: false, download: false });
});

it("offers the connection form only when there is nothing connected", () => {
  const states: SourceState[] = [
    "disconnected",
    "pending",
    "connected",
    "expired",
    "revoked",
    "error",
  ];
  const offered = states.filter((state) => canConnect(provider({ state })));
  expect(offered).toEqual(["disconnected", "expired", "revoked", "error"]);
  expect(canConnect(provider({ id: "local", state: "disconnected" }))).toBe(
    false,
  );
});

it("queries the local library and connected providers only", () => {
  expect(canQuery(provider({ id: "local", state: "disconnected" }))).toBe(true);
  expect(canQuery(provider({ state: "connected" }))).toBe(true);
  expect(canQuery(provider({ state: "expired" }))).toBe(false);
  expect(canQuery(provider({ state: "pending" }))).toBe(false);
});

it("declares the capability the interface is allowed to offer", () => {
  const youtube = provider({
    id: "youtube",
    capabilities: ["auth", "search", "read"],
  });
  expect(hasCapability(youtube, "search")).toBe(true);
  expect(hasCapability(youtube, "playback")).toBe(false);
  expect(hasCapability(youtube, "download")).toBe(false);
  const drive = provider({
    id: "google-drive",
    capabilities: ["auth", "search", "read", "playback", "download", "revoke"],
  });
  expect(hasCapability(drive, "revoke")).toBe(true);
});

it("explains the current state without exposing credentials", () => {
  expect(stateHint(provider())).toBe("Sin conectar");
  expect(
    stateHint(provider({ state: "connected", label: "ana@example.com" })),
  ).toBe("ana@example.com");
  expect(
    stateHint(
      provider({ state: "expired", last_error: "La autorización caducó" }),
    ),
  ).toBe("La autorización caducó");
  expect(stateHint(provider({ state: "pending" }))).toBe(
    "Esperando la autorización en el navegador",
  );
  expect(stateHint(provider({ state: "connected", label: "" }))).toBe(
    "Conectada",
  );
});

it("formats sizes and keeps an empty snapshot", () => {
  expect(formatSize(null)).toBe("");
  expect(formatSize(512)).toBe("512 B");
  expect(formatSize(4096)).toBe("4 KB");
  expect(formatSize(5 * 1024 * 1024)).toBe("5.0 MB");
  expect(emptySources).toEqual({
    providers: [],
    results: [],
    external: false,
    pending_url: null,
    downloaded: null,
    warnings: [],
  });
});

it("keeps automatic YouTube usable without a connection form or API key", () => {
  const automatic = provider({
    id: "youtube-auto",
    auth: "none",
    state: "connected",
    has_secret: false,
    capabilities: ["search", "read", "playback", "download"],
  });
  expect(canConnect(automatic)).toBe(false);
  expect(canDisconnect(automatic)).toBe(false);
  expect(canQuery(automatic)).toBe(true);
  expect(
    entryActions(automatic, {
      provider: "youtube-auto",
      id: "BaW_jenozKc",
      name: "Test",
      kind: "audio",
      detail: "",
      size: null,
      source_url: null,
    }),
  ).toEqual({ play: true, download: true });
  expect(
    entryActions(
      provider({ id: "youtube", capabilities: ["search", "read"] }),
      {
        provider: "youtube",
        id: "BaW_jenozKc",
        name: "Test",
        kind: "video",
        detail: "",
        size: null,
        source_url: null,
      },
    ),
  ).toEqual({ play: false, download: false });
});

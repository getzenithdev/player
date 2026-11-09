const cacheReleasers = new Set<() => void>();
const imageReadQueue: Array<() => void> = [];
let activeImageReads = 0;

/** Avoid decoding and transferring an entire gallery's images simultaneously. */
export function scheduleImageRead<T>(read: () => Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const start = () => {
      ++activeImageReads;
      void Promise.resolve()
        .then(read)
        .then(resolve, reject)
        .finally(() => {
          --activeImageReads;
          imageReadQueue.shift()?.();
        });
    };
    if (activeImageReads < 4) start();
    else imageReadQueue.push(start);
  });
}
let releaseTimer: ReturnType<typeof setTimeout> | undefined;

function releaseCaches() {
  cacheReleasers.forEach((release) => release());
}

if (typeof window !== "undefined") {
  const cancelRelease = () => clearTimeout(releaseTimer);
  const scheduleRelease = () => {
    cancelRelease();
    releaseTimer = setTimeout(releaseCaches, 15_000);
  };
  window.addEventListener("blur", scheduleRelease);
  window.addEventListener("focus", cancelRelease);
  window.addEventListener("pagehide", releaseCaches);
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) scheduleRelease();
    else cancelRelease();
  });
}

/** Bounds retained image payloads; decoded images are released by their components. */
export class ImageMemoryCache<T> {
  private entries = new Map<
    string,
    { value: T; expires: number; bytes: number }
  >();
  private bytes = 0;
  private expiryTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    private maxBytes: number,
    private maxEntries: number,
    private sizeOf: (value: T) => number,
  ) {
    cacheReleasers.add(() => this.clear());
  }

  get(key: string): T | undefined {
    const entry = this.entries.get(key);
    if (!entry) return undefined;
    if (entry.expires <= Date.now()) {
      this.remove(key);
      this.scheduleExpiry();
      return undefined;
    }
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.value;
  }

  set(key: string, value: T, ttl: number) {
    this.remove(key);
    const bytes = this.sizeOf(value);
    if (
      bytes <= this.maxBytes &&
      !(typeof document !== "undefined" && document.hidden)
    ) {
      while (
        this.entries.size &&
        (this.bytes + bytes > this.maxBytes ||
          this.entries.size >= this.maxEntries)
      ) {
        this.remove(this.entries.keys().next().value!);
      }
      this.entries.set(key, { value, expires: Date.now() + ttl, bytes });
      this.bytes += bytes;
    }
    this.scheduleExpiry();
  }

  clear() {
    this.entries.clear();
    this.bytes = 0;
    clearTimeout(this.expiryTimer);
    this.expiryTimer = undefined;
  }

  private remove(key: string) {
    const entry = this.entries.get(key);
    if (entry) this.bytes -= entry.bytes;
    this.entries.delete(key);
  }

  private scheduleExpiry() {
    clearTimeout(this.expiryTimer);
    this.expiryTimer = undefined;
    if (!this.entries.size) return;
    const earliest = Math.min(
      ...Array.from(this.entries.values(), (entry) => entry.expires),
    );
    this.expiryTimer = setTimeout(
      () => {
        const now = Date.now();
        for (const [key, entry] of this.entries) {
          if (entry.expires <= now) this.remove(key);
        }
        this.scheduleExpiry();
      },
      Math.max(1, earliest - Date.now()),
    );
  }
}

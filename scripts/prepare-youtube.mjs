// Prepare pinned, checksum-verified local helpers; no API credentials required.
// Never downloads code at playback time. Commit a manifest change to update.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  chmod,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import manifest from "./youtube-tools.json" with { type: "json" };

const root = fileURLToPath(new URL("../src-tauri/tools/", import.meta.url));
const platform = `${process.platform}-${process.arch}`;
const target = manifest.platforms[platform];
if (!target) throw new Error(`Unsupported YouTube helper target: ${platform}`);
const suffix = process.platform === "win32" ? ".exe" : "";
const digest = (buffer) => createHash("sha256").update(buffer).digest("hex");
const stamp = JSON.stringify({
  platform,
  target,
  node: manifest.node,
  ytdlp: manifest.ytdlp,
});
try {
  if ((await readFile(path.join(root, "prepared.json"), "utf8")) === stamp) {
    for (const name of [
      `yt-dlp${suffix}`,
      `node${suffix}`,
      "Node-LICENSE.txt",
      "yt-dlp-LICENSE.txt",
      "yt-dlp-THIRD_PARTY_LICENSES.txt",
    ])
      await readFile(path.join(root, name));
    console.log(`YouTube helpers already prepared for ${platform}`);
    process.exit(0);
  }
} catch {
  /* Missing or outdated components are prepared below. */
}

async function download(url, expected) {
  const response = await fetch(url, { signal: AbortSignal.timeout(180_000) });
  if (!response.ok)
    throw new Error(`Download failed (${response.status}): ${url}`);
  const data = Buffer.from(await response.arrayBuffer());
  if (digest(data) !== expected) throw new Error(`SHA-256 mismatch: ${url}`);
  return data;
}
const temporary = await mkdtemp(path.join(tmpdir(), "zenith-youtube-"));
try {
  const archive = target.nodeFile;
  const [extractor, node] = await Promise.all([
    download(
      `https://github.com/yt-dlp/yt-dlp/releases/download/${manifest.ytdlp}/${target.ytdlpFile}`,
      target.ytdlpSha256,
    ),
    download(
      `https://nodejs.org/dist/${manifest.node}/${archive}`,
      target.nodeSha256,
    ),
  ]);
  await writeFile(path.join(temporary, archive), node);
  // Windows 10+, Linux and macOS provide tar. No interpolated shell commands.
  const archiveRoot = archive.replace(/\.tar\.gz$|\.zip$/, "");
  execFileSync(
    "tar",
    [
      "--no-same-owner",
      "-xf",
      path.join(temporary, archive),
      "-C",
      temporary,
      `${archiveRoot}/${process.platform === "win32" ? "node.exe" : "bin/node"}`,
      `${archiveRoot}/LICENSE`,
    ],
    { stdio: "pipe" },
  );
  const nodeRoot = path.join(temporary, archiveRoot);
  const nodePath = path.join(
    nodeRoot,
    process.platform === "win32" ? "node.exe" : "bin/node",
  );
  await mkdir(root, { recursive: true });
  const extractorPath = path.join(root, `yt-dlp${suffix}`);
  await writeFile(extractorPath, extractor);
  await chmod(extractorPath, 0o755);
  await copyFile(nodePath, path.join(root, `node${suffix}`));
  await chmod(path.join(root, `node${suffix}`), 0o755);
  await copyFile(
    path.join(nodeRoot, "LICENSE"),
    path.join(root, "Node-LICENSE.txt"),
  );
  for (const name of ["yt-dlp-LICENSE.txt", "yt-dlp-THIRD_PARTY_LICENSES.txt"])
    await copyFile(
      fileURLToPath(new URL(`./licenses/${name}`, import.meta.url)),
      path.join(root, name),
    );
  await writeFile(path.join(root, "prepared.json"), stamp);
  console.log(
    `Prepared yt-dlp ${manifest.ytdlp} + Node ${manifest.node} for ${platform}`,
  );
} finally {
  await rm(temporary, { recursive: true, force: true });
}

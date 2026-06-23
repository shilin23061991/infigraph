#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");
const os = require("os");
const { execSync } = require("child_process");

const VERSION = require("./package.json").version;

const PLATFORM_MAP = {
  "darwin-arm64": { target: "aarch64-apple-darwin", ext: "tar.gz" },
  "darwin-x64": { target: "x86_64-apple-darwin", ext: "tar.gz" },
  "linux-x64": { target: "x86_64-unknown-linux-gnu", ext: "tar.gz" },
  "win32-x64": { target: "x86_64-pc-windows-msvc", ext: "zip" },
};

const key = `${process.platform}-${process.arch}`;
const platform = PLATFORM_MAP[key];
if (!platform) {
  console.error(`[infigraph] Unsupported platform: ${key}`);
  console.error("[infigraph] Supported: macOS (arm64, x64), Linux (x64), Windows (x64)");
  process.exit(1);
}

const binDir = path.join(__dirname, "bin");
const binaryExt = process.platform === "win32" ? ".exe" : "";
const infigraphBin = path.join(binDir, `infigraph${binaryExt}`);
const mcpBin = path.join(binDir, `infigraph-mcp${binaryExt}`);
const modelsDir = path.join(binDir, "models");

// Skip if already installed at correct version
if (fs.existsSync(infigraphBin)) {
  try {
    const out = execSync(`"${infigraphBin}" --version`, {
      encoding: "utf8",
      timeout: 5000,
    });
    if (out.includes(VERSION)) {
      console.log(`[infigraph] v${VERSION} already installed`);
      runMigration();
      process.exit(0);
    }
  } catch (_) {
    // Binary exists but broken — re-download
  }
}

// Determine download URL
// Public GitHub releases (default)
let baseUrl = `https://github.com/intuit/infigraph/releases/download/v${VERSION}`;

// Intuit internal: override via env var
if (process.env.INFIGRAPH_MIRROR) {
  baseUrl = process.env.INFIGRAPH_MIRROR;
}

const archiveName = `infigraph-${platform.target}.${platform.ext}`;
const url = `${baseUrl}/${archiveName}`;
const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "infigraph-"));
const tmpArchive = path.join(tmpDir, archiveName);

console.log(`[infigraph] Downloading v${VERSION} for ${platform.target}...`);

try {
  // Use curl (available on macOS/Linux/Windows 10+)
  execSync(`curl -fSL --retry 3 "${url}" -o "${tmpArchive}"`, {
    stdio: ["pipe", "pipe", "inherit"],
    timeout: 120000,
  });
} catch (_) {
  console.error(`[infigraph] Download failed from ${url}`);
  console.error("[infigraph] If behind a corporate network, set INFIGRAPH_MIRROR to your Artifactory URL");
  cleanup(tmpDir);
  process.exit(1);
}

// Extract
console.log("[infigraph] Extracting...");
fs.mkdirSync(binDir, { recursive: true });

try {
  if (platform.ext === "tar.gz") {
    execSync(`tar -xzf "${tmpArchive}" -C "${binDir}"`, { stdio: "pipe" });
  } else {
    // Windows zip — use PowerShell
    execSync(
      `powershell -Command "Expand-Archive -Force -Path '${tmpArchive}' -DestinationPath '${binDir}'"`,
      { stdio: "pipe" }
    );
  }
} catch (e) {
  console.error(`[infigraph] Extraction failed: ${e.message}`);
  cleanup(tmpDir);
  process.exit(1);
}

// Set executable permission (unix)
if (process.platform !== "win32") {
  if (fs.existsSync(infigraphBin)) fs.chmodSync(infigraphBin, 0o755);
  if (fs.existsSync(mcpBin)) fs.chmodSync(mcpBin, 0o755);
}

// Verify
try {
  const ver = execSync(`"${infigraphBin}" --version`, {
    encoding: "utf8",
    timeout: 5000,
  });
  console.log(`[infigraph] Installed: ${ver.trim()}`);
} catch (_) {
  console.error("[infigraph] Installation verification failed");
  cleanup(tmpDir);
  process.exit(1);
}

cleanup(tmpDir);
runMigration();

function cleanup(dir) {
  try {
    fs.rmSync(dir, { recursive: true, force: true });
  } catch (_) {}
}

function runMigration() {
  try {
    require("./migrate.js");
  } catch (_) {}
}

# ADR-012 — The shell is Svelte 5 + TypeScript, packaged with Electron and served to browsers

**Status:** Accepted (product owner, 2026-10-17) · **Date:** 2026-10-17 · **Milestone:** OS0 · **Concept:** `docs/AGENTIC_OS.md` §10

## Context

The prototype shell is a 2,500-line `index.html` with inline scripts, no components, no tests of
its own and no accessibility structure. CLAUDE.md rule 7 ("do not restyle index.html") protected the
prototype's look while features were wired; 1.0 needs a mature, polished, multi-window environment
with camera, voice and rich visual effects that behave the same on every machine.

## Decision

- **Svelte 5 + TypeScript + Vite** for the shell: compiled, fine-grained reactivity suited to
  streaming surfaces and many live windows, small bundles, built-in transitions.
- **Electron** for the desktop app (macOS first, then Windows and Linux). Bundled Chromium gives
  identical rendering and web APIs everywhere — getUserMedia and MediaPipe on the GPU for gestures,
  Web Audio and WebRTC for voice, backdrop and blur materials — and the most mature desktop tooling
  (auto-update, crash reporting, code signing and notarization).
- **Thin main process.** The Electron main process only owns native concerns: windows, tray, global
  hotkey, notifications, deep links, auto-update, device permissions, `safeStorage` for the
  device's pairing token, and launching `agentrixd` as a supervised sidecar. It carries no product logic.
- **Same shell everywhere.** The renderer talks to the kernel over the shell bus (WebSocket), not
  Electron IPC, so the identical build is served by the kernel to browsers and phones (responsive, PWA later).
- **Hardened by default:** `contextIsolation: true`, `sandbox: true`, `nodeIntegration: false`, a
  strict Content-Security-Policy, a minimal typed preload bridge, no remote content in privileged windows.
- **Footprint:** one main window renders the spatial environment (Spaces, windows and cards are
  drawn inside it); a window can be detached into a native window on demand. This keeps memory near
  a single Chromium renderer.
- Types for surfaces, verbs and events are generated from `agentrix-protocol` JSON Schema.
- The prototype's visual language (dark field, glass cards, worlds) carries forward as design
  tokens; rule 7 applies to the prototype only and is superseded for the new shell.

## Alternatives considered

- **Tauri 2:** a ~15 MB app and lower memory with a Rust native side and a mobile path, but each OS's
  system webview differs (WKWebView, WebView2, WebKitGTK), which puts camera, voice and visual
  effects at risk of per-platform differences.
- **React 19:** the largest ecosystem; heavier re-render model for high-frequency streaming.
- **Browser-only:** no native integration; the OS would feel like a web page.

## Consequences

- A new `shell/` (Svelte) and `desktop/` (Electron main + preload, electron-builder) tree with its own
  CI (typecheck, unit, Playwright with Electron and browser targets).
- Installer around 150–250 MB and a Node.js runtime in the desktop app; no native mobile app from
  this stack — phones use the browser shell until a mobile decision is made.
- Because the kernel is a separate process and the shell talks to it over the shell bus, the wrapper
  can be replaced later without touching the kernel, agents or shell code.
- The prototype UI stays runnable until the new shell reaches parity in OS1, then is removed.

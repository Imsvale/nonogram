import { defineConfig } from "vitest/config";

// Relative base so the same build works at https://imsvale.github.io/nonogram/
// and at any other sub-path (or the dev server root) without reconfiguration.
export default defineConfig({
  base: "./",
  server: {
    // Vite's default ("localhost") can bind IPv6 [::1] only on Windows, which
    // browsers resolving localhost to IPv4 can't reach. Pin IPv4 loopback.
    // Doesn't open a browser: just browse to http://127.0.0.1:5173/ yourself.
    // Use `npm run dev -- --host` to expose it on the LAN (e.g. for a phone).
    host: "127.0.0.1",
    port: 5173,
  },
  build: { target: "es2022", sourcemap: true },
  test: { include: ["tests/**/*.test.ts"] },
});

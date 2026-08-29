import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    // 10k range: the usual dev ports are taken on this machine. Avoid 10080 —
    // browsers block it as an unsafe port.
    port: 10173,
    // Fail loudly rather than silently hopping to another port, which would
    // break the proxy assumptions below.
    strictPort: true,
    // The Rust service owns the API; proxy in dev so the frontend calls the
    // same paths it will in production.
    proxy: {
      "/api": { target: "http://127.0.0.1:10090", changeOrigin: true },
      "/badge": { target: "http://127.0.0.1:10090", changeOrigin: true },
    },
  },
  build: {
    // Served by the Rust binary from WEB_ROOT.
    outDir: "dist",
  },
});

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { VitePWA } from "vite-plugin-pwa";
import { readFileSync } from "node:fs";

const host = process.env.TAURI_DEV_HOST;
// Shown in Settings: the desktop version (Cargo.toml is its only source) and
// when this frontend was built, which tells PWA deploys apart.
const appVersion = readFileSync("src-tauri/Cargo.toml", "utf8").match(/^version = "(.+)"/m)![1];
const buildDate = new Date().toLocaleString("sv-SE").slice(0, 16);

export default defineConfig(({ mode }) => ({
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
    __BUILD_DATE__: JSON.stringify(buildDate),
  },
  plugins: [
    react(),
    tailwindcss(),
    VitePWA({
      // The service worker is for the web PWA only. Desktop installs from
      // before October 2026 left one at http://tauri.localhost that serves
      // their old frontend forever (a self-destroying sw.js never replaced
      // it); desktop now lives at https://tauri.localhost (useHttpsScheme in
      // tauri.conf.json), out of that worker's reach, and registers none.
      disable: mode !== "web",
      registerType: "autoUpdate",
      includeAssets: ["favicon.ico", "apple-touch-icon.png", "mask-icon.svg"],
      manifest: false, // We use public/manifest.json
      workbox: {
        globPatterns: ["**/*.{js,css,html,ico,png,svg,woff2}"],
        // The server answers these itself (the .msi downloads, the API): opening
        // such a link must reach it, not get the app's index.html.
        navigateFallbackDenylist: [/^\/download\//, /^\/api\//],
        runtimeCaching: [
          {
            urlPattern: /^https:\/\/fonts\.googleapis\.com\/.*/i,
            handler: "CacheFirst",
            options: {
              cacheName: "google-fonts-cache",
              expiration: { maxEntries: 10, maxAgeSeconds: 60 * 60 * 24 * 365 },
            },
          },
          {
            urlPattern: /^https:\/\/fonts\.gstatic\.com\/.*/i,
            handler: "CacheFirst",
            options: {
              cacheName: "google-fonts-static-cache",
              expiration: { maxEntries: 10, maxAgeSeconds: 60 * 60 * 24 * 365 },
            },
          },
        ],
      },
    }),
  ],

  // Vite options tailored for Tauri development
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },

  // Env variables starting with TAURI_ will be exposed to tauri's source code
  envPrefix: ["VITE_", "TAURI_"],

  build: {
    // Tauri uses Chromium on Windows and WebKit on macOS and Linux
    target: process.env.TAURI_PLATFORM === "windows" ? "chrome105" : "safari14",
    // Don't minify for debug builds
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    // Produce sourcemaps for debug builds
    sourcemap: !!process.env.TAURI_DEBUG,
    // Fonts stay files: the CSP (font-src 'self') refuses data: URIs.
    assetsInlineLimit: (file) => (file.endsWith(".woff2") ? false : undefined),
  },

  resolve: {
    alias: {
      "@": "/src",
      "@wasm": "/crates/mypass-wasm/pkg",
    },
  },
}));

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vitejs.dev/config/
export default defineConfig(async () => {
  const runtime = process.env.VITE_RUNTIME || "tauri";
  const proxyTarget = process.env.VITE_API_PROXY_TARGET;

  const server = {
    port: 1420,
    strictPort: true,
  };

  if (runtime === "web" && proxyTarget) {
    server.proxy = {
      "/api": {
        target: proxyTarget,
        changeOrigin: true,
        ws: true,
      },
    };
  }

  return {
  plugins: [react()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server,
  // 3. to make use of `TAURI_PLATFORM` and other env variables
  envPrefix: ["VITE_", "TAURI_"],
  };
});

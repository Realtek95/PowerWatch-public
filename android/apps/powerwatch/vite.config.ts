import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // Android 9's factory WebView is Chromium 69. Transpile syntax for that
  // baseline; the real packaged frontend is exercised by instrumentation.
  build: { target: "chrome69", cssTarget: "chrome69" },
});

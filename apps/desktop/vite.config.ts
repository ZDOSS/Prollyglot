import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const desktopRoot = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        main: resolve(desktopRoot, "index.html"),
        appearance: resolve(desktopRoot, "appearance.html"),
        overlay: resolve(desktopRoot, "overlay.html"),
        visualOverlay: resolve(desktopRoot, "visual-overlay.html"),
        regionSelector: resolve(desktopRoot, "region-selector.html")
      }
    }
  },
  server: {
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"]
    }
  }
});

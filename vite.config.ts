import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

// Two entry points: the settings window (index.html) and the popup (popup.html).
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: {
    target: "chrome105",
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        popup: fileURLToPath(new URL("./popup.html", import.meta.url)),
      },
    },
  },
});

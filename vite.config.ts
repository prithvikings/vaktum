import { resolve, sep } from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const cargoTargetDir = resolve(__dirname, "src-tauri", "target");

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: (path) =>
        path === cargoTargetDir || path.startsWith(`${cargoTargetDir}${sep}`),
    },
  },
});

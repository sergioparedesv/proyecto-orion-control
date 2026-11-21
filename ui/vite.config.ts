import { defineConfig } from "vite";

// Tauri espera un puerto fijo y no debe limpiar la consola de Rust.
export default defineConfig({
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2021", sourcemap: false, outDir: "dist", emptyOutDir: true },
});

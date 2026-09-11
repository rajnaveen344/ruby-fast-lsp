import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  base: "/ruby-fast-lsp/",
  plugins: [react()],
  server: { allowedHosts: ["ruby-fast-lsp.localhost"] },
  build: {
    target: "es2019",
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        manualChunks: undefined,
      },
    },
  },
});

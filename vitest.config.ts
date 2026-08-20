import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["./tests/setup.ts"],
    include: ["tests/unit/**/*.test.{ts,tsx}"],
    css: true,
    restoreMocks: true,
    fileParallelism: false,
    maxWorkers: 1,
    minWorkers: 1,
  },
});

import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // Serve the Trunk build output verbatim. Letting Vite transform the built
  // index.html breaks its inline module initializer and asset URLs.
  publicDir: "dist",
  test: {
    include: ["tests/**/*.spec.ts"],
    browser: {
      enabled: true,
      provider: playwright(),
      instances: [{ browser: "chromium" }],
      headless: true,
    },
  },
});

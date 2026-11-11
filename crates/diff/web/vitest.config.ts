import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // Serve the Trunk build output verbatim. Letting Vite transform the built
  // index.html breaks its inline module initializer and asset URLs.
  publicDir: "dist",
  test: {
    projects: [
      { name: "webgpu", args: ["--enable-unsafe-webgpu", "--use-angle=vulkan", "--enable-features=Vulkan"] },
      { name: "webgl2", args: ["--disable-webgpu", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] },
    ].map(({ name, args }) => ({
      test: {
        name,
        include: ["tests/**/*.spec.ts"],
        globalSetup: ["tests/remote.setup.ts"],
        provide: { graphicsContext: name },
        browser: {
          enabled: true,
          provider: playwright({ launchOptions: { args }, contextOptions: { deviceScaleFactor: 1.25 } }),
          instances: [{ browser: "chromium" }],
          headless: true,
        },
      },
    })),
  },
});

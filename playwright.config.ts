import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/ui",
  timeout: 60000,
  use: {
    baseURL: "http://127.0.0.1:1420",
    browserName: "chromium",
    channel: process.env.CI ? undefined : "msedge",
    viewport: { width: 390, height: 844 },
    userAgent: "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/120.0.0.0 Mobile Safari/537.36",
    locale: "zh-TW",
  },
  webServer: { command: "npm run dev -- --host 127.0.0.1", url: "http://127.0.0.1:1420", reuseExistingServer: !process.env.CI },
});

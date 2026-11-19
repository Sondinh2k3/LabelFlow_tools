import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/ui',
  fullyParallel: false,
  use: {
    baseURL: 'http://127.0.0.1:1420',
    viewport: { width: 1280, height: 820 },
    deviceScaleFactor: 1,
  },
  projects: [
    {
      name: 'chromium',
      use: {
        browserName: 'chromium',
        channel: process.env.CI ? undefined : 'msedge',
      },
    },
  ],
  webServer: process.env.LABELFLOW_UI_SERVER
    ? undefined
    : {
        command:
          'node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 1420 --strictPort',
        url: 'http://127.0.0.1:1420',
        reuseExistingServer: !process.env.CI,
      },
});

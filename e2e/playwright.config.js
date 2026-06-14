// Playwright config for the Tithe web UI. Drives the *dev server* (native sim +
// /api), which renders the same UI as the WASM build — so screenshots here
// reflect what ships. `reuseExistingServer` picks up a running `tithe-web`;
// otherwise Playwright starts one (the first `cargo run` compile can be slow).
const { defineConfig, devices } = require('@playwright/test');

module.exports = defineConfig({
  testDir: './tests',
  outputDir: './test-results',
  webServer: {
    command: 'cargo run -p tithe-web',
    cwd: '..',
    url: 'http://127.0.0.1:8770',
    timeout: 180_000,
    reuseExistingServer: true,
  },
  use: {
    baseURL: 'http://127.0.0.1:8770',
    viewport: { width: 1200, height: 900 },
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});

// Two-instance Ghost Talk E2E. The runner (scripts/run-all-e2e.*) builds the
// web release, starts the local relay, funds the instance wallets, and passes
// everything through environment variables.
import { defineConfig } from '@playwright/test';

const port = Number(process.env.GHOST_E2E_PORT || 8765);
const python = process.env.GHOST_E2E_PYTHON || (process.platform === 'win32' ? 'python' : 'python3');

export default defineConfig({
  testDir: './tests',
  globalSetup: './global-setup.mjs',
  timeout: 20 * 60 * 1000,
  expect: { timeout: 120 * 1000 },
  workers: 1,
  fullyParallel: false,
  reporter: [['list'], ['html', { open: 'never', outputFolder: '../../../../target/e2e/report' }]],
  outputDir: '../../../../target/e2e/results',
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    launchOptions: {
      // Deterministic microphone: Chromium's fake capture device (a tone).
      args: [
        '--use-fake-ui-for-media-stream',
        '--use-fake-device-for-media-stream',
        '--autoplay-policy=no-user-gesture-required',
      ],
    },
  },
  webServer: {
    command: `${python} -m http.server ${port} --bind 127.0.0.1 --directory ../../../../target/build/frontend`,
    url: `http://127.0.0.1:${port}/index.html`,
    reuseExistingServer: false,
    timeout: 60 * 1000,
  },
});

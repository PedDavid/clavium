// Browser tests against the demo server: `npm test` from this directory.
// The server is started with the release binary if CLAVIUM_BIN is set (CI),
// otherwise with `cargo run`.
import { defineConfig } from '@playwright/test';

const port = 18080;
const bin = process.env.CLAVIUM_BIN;
const args = `--demo --listen 127.0.0.1:${port} --metrics-listen 127.0.0.1:19090 --public-url http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: './tests',
  timeout: 30_000,
  use: { baseURL: `http://127.0.0.1:${port}`, browserName: 'chromium' },
  webServer: {
    command: bin ? `${bin} ${args}` : `cargo run --quiet -- ${args}`,
    cwd: '..',
    url: `http://127.0.0.1:${port}/`,
    timeout: 180_000,
    reuseExistingServer: false,
  },
});

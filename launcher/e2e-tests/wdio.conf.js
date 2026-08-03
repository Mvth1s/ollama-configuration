// WebDriver e2e smoke test for launcher/dist, driven through tauri-driver
// (WebKitWebDriver on Linux, msedgedriver on Windows). See CLAUDE.md's
// "e2e/UI tests" section for why this is separate from the #[cfg(test)]
// Rust unit tests, and why it can only be fully verified in CI (no
// webkit2gtk-driver/Xvfb, and no Windows machine with a matching
// msedgedriver, available in every dev environment). Adapted from Tauri's
// own WebdriverIO example:
// https://v2.tauri.app/develop/tests/webdriver/example/webdriverio/
//
// Windows support (isWindows below) is UNVERIFIED against a real Windows
// run as of the change that added it - see gui/e2e-tests/wdio.conf.js's
// own note (this file mirrors it) and .github/workflows/e2e.yml's
// e2e-windows job for how TAURI_DRIVER_NATIVE_DRIVER gets set.
import os from 'os';
import path from 'path';
import { spawn, spawnSync } from 'child_process';
import { fileURLToPath } from 'url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const isWindows = process.platform === 'win32';

let tauriDriver;
let exit = false;

export const config = {
  host: '127.0.0.1',
  port: 4444,
  specs: ['./test/specs/**/*.js'],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      'tauri:options': {
        application: path.resolve(
          __dirname,
          '../src-tauri/target/debug',
          isWindows ? 'selfllama-launcher.exe' : 'selfllama-launcher'
        ),
        // See gui/e2e-tests/wdio.conf.js's own note (this file mirrors
        // it): tauri-driver serializes this into ms:edgeOptions for
        // msedgedriver on Windows, and omitting it has been reported to
        // break session creation there.
        webviewOptions: {},
      },
    },
  ],
  reporters: ['spec'],
  framework: 'mocha',
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },

  // Build the debug binary so it exists for the webdriver session. Plain
  // `cargo build` is enough here (no tauri-cli/bundling involved) - see
  // launcher/README.md's plain `cargo run` dev workflow.
  onPrepare: () => {
    spawnSync('cargo', ['build'], {
      cwd: path.resolve(__dirname, '../src-tauri'),
      stdio: 'inherit',
      shell: true,
    });
  },

  beforeSession: () => {
    // See gui/e2e-tests/wdio.conf.js's beforeSession for why this env var
    // exists - tauri-driver needs --native-driver on Windows, unlike Linux
    // where WebKitWebDriver is found on PATH automatically.
    const driverArgs =
      isWindows && process.env.TAURI_DRIVER_NATIVE_DRIVER
        ? ['--native-driver', process.env.TAURI_DRIVER_NATIVE_DRIVER]
        : [];
    tauriDriver = spawn(
      path.resolve(os.homedir(), '.cargo', 'bin', isWindows ? 'tauri-driver.exe' : 'tauri-driver'),
      driverArgs,
      { stdio: [null, process.stdout, process.stderr] }
    );

    tauriDriver.on('error', (error) => {
      console.error('tauri-driver error:', error);
      process.exit(1);
    });
    tauriDriver.on('exit', (code) => {
      if (!exit) {
        console.error('tauri-driver exited with code:', code);
        process.exit(1);
      }
    });
  },

  afterSession: () => {
    closeTauriDriver();
  },
};

function closeTauriDriver() {
  exit = true;
  tauriDriver?.kill();
}

function onShutdown(fn) {
  const cleanup = () => {
    try {
      fn();
    } finally {
      process.exit();
    }
  };

  process.on('exit', cleanup);
  process.on('SIGINT', cleanup);
  process.on('SIGTERM', cleanup);
  process.on('SIGHUP', cleanup);
  process.on('SIGBREAK', cleanup);
}

onShutdown(() => {
  closeTauriDriver();
});

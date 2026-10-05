import { defineConfig } from '@playwright/test';
export default defineConfig({
  // Specs live in each owner's tests/browser/. An explicit testDir turns off .gitignore
  // pruning, so ask for it: discovery must not walk target/ or .build/.
  testDir: '.',
  testMatch: '**/tests/browser/**/*.spec.ts',
  respectGitIgnore: true,
  fullyParallel: true,
  workers: 4,
  timeout: 30000,
  retries: 0,
  reporter: 'list',
  outputDir: process.env.DISPATCH_TEST_OUTPUT || './test-results',
  use: {
    viewport: { width: 1440, height: 1000 },
    // Default device matches the seeded DSP; timezone regressions override this.
    timezoneId: 'America/Chicago',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
});

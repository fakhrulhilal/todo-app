import { defineConfig, devices } from '@playwright/test';

// The apps on the ports compose publishes them on (TODO_WEB_PORT,
// TODO_LEPTOS_PORT and TODO_TOPCOAT_PORT). Under OrbStack the domains work too:
//   DIOXUS_URL=https://todo.local LEPTOS_URL=https://leptos.todo.local npm test
const dioxus = process.env.DIOXUS_URL ?? 'http://localhost:8080';
const leptos = process.env.LEPTOS_URL ?? 'http://localhost:8083';
const topcoat = process.env.TOPCOAT_URL ?? 'http://localhost:8081';

export default defineConfig({
  testDir: './ui',
  // Every test makes todos of its own, but they share one database, so the
  // apps run one after the other rather than racing on the same rows.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    ...devices['Desktop Chrome'],
    // For the OrbStack domains: their certificates are trusted by the
    // system, not always by Playwright's own Chromium.
    ignoreHTTPSErrors: true,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    // metadata: what the spec may expect of each app (see `App` in the spec).
    { name: 'dioxus', use: { baseURL: dioxus }, metadata: { api: true, filterInUrl: true } },
    { name: 'leptos', use: { baseURL: leptos }, metadata: { api: true, filterInUrl: true } },
    { name: 'topcoat', use: { baseURL: topcoat }, metadata: { api: false, filterInUrl: true } },
  ],
});

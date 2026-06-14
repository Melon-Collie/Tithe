// Smoke test + screenshots of the web UI. The PNGs in `shots/` are for visual
// review (the watch view is validated by eye — CLAUDE.md > Workflow). Run with
// `npm test` in this folder; pass `--headed` to watch it live.
const { test, expect } = require('@playwright/test');

test('editor loads, run a match, watch it', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('h1')).toContainText('TITHE');
  await expect(page.locator('#ed0')).toBeVisible(); // a formation board
  await page.waitForTimeout(500); // let the formation canvases draw
  await page.screenshot({ path: 'shots/01-editor.png', fullPage: true });

  await page.click('#run');
  await expect(page.locator('#viewer-section')).toBeVisible({ timeout: 20_000 });
  await page.waitForTimeout(1500); // let playback advance into the action
  await page.screenshot({ path: 'shots/02-match.png', fullPage: true });
});

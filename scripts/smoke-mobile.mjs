/**
 * Mobile layout smoke — run with server up:
 *   AGENTRIX_URL=http://127.0.0.1:9847 node scripts/smoke-mobile.mjs
 */
import { chromium, devices } from 'playwright';

const base = (process.env.AGENTRIX_URL || 'http://127.0.0.1:9847').replace(/\/$/, '');
const iPhone = devices['iPhone 13'];

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ ...iPhone });
const page = await context.newPage();

try {
  await page.goto(base, { waitUntil: 'domcontentloaded', timeout: 30000 });
  await page.waitForSelector('.field', { timeout: 15000 });
  const field = page.locator('.field');
  const box = await field.boundingBox();
  if (!box || box.width > 500) {
    throw new Error(`expected narrow mobile field, got width=${box?.width}`);
  }
  const health = await page.request.get(`${base}/health`);
  if (!health.ok()) throw new Error(`health ${health.status()}`);
  const data = await health.json();
  if (!data.runtime) throw new Error('health missing runtime block');
  console.log('mobile smoke ok —', data.runtime.milestone, 'mock=', data.runtime.uses_mock_orchestration);
} finally {
  await browser.close();
}
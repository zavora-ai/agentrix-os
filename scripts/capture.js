// Captures the greeting → scenario bloom beat as PNG frames.
// Local demo:  node capture.js
// Production:  AGENTRIX_URL=https://os.zavora.example node capture.js
const puppeteer = require('puppeteer-core');
const path = require('path');
const fs = require('fs');

const CHROME =
  process.env.CHROME_PATH ||
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const BASE_URL = process.env.AGENTRIX_URL;
const URL = BASE_URL
  ? `${BASE_URL.replace(/\/$/, '')}/?demo=1`
  : 'file://' + path.resolve(__dirname, '../web/index.html') + '?demo=1';
const OUT = path.resolve(__dirname, '../demo/frames');
const W = 1280,
  H = 800,
  FPS = 20,
  SECONDS = 14;

(async () => {
  fs.rmSync(OUT, { recursive: true, force: true });
  fs.mkdirSync(OUT, { recursive: true });
  const browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: 'new',
    args: [
      `--window-size=${W},${H}`,
      '--hide-scrollbars',
      '--force-device-scale-factor=1',
    ],
  });
  const page = await browser.newPage();
  await page.setViewport({ width: W, height: H, deviceScaleFactor: 1 });
  console.log('capture target:', URL);
  await page.goto(URL, { waitUntil: 'networkidle0', timeout: 60000 });

  const total = FPS * SECONDS,
    interval = 1000 / FPS;
  let i = 0;
  const grab = async () => {
    await page.screenshot({
      path: path.join(OUT, `f${String(i).padStart(4, '0')}.png`),
    });
    i++;
  };

  await page.waitForFunction('typeof launch === "function"', { timeout: 15000 });
  await new Promise((r) => setTimeout(r, 600));
  for (let k = 0; k < FPS * 2 && i < total; k++) {
    await grab();
    await new Promise((r) => setTimeout(r, interval));
  }

  await page.evaluate(() => {
    clearTimeout(window.idleTimer);
    document.getElementById('greet').classList.remove('show');
    launch('Start my day');
  });

  while (i < total) {
    await grab();
    await new Promise((r) => setTimeout(r, interval));
  }

  await browser.close();
  console.log('captured', i, 'frames to', OUT);
})().catch((err) => {
  console.error('capture failed:', err.message);
  process.exit(1);
});
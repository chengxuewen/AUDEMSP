// E2E: Host produce → browser consume → video render
// Usage: node e2e-sfu-consume.cjs <admin_token>
const { chromium } = require('playwright');

const TOKEN = process.argv[2];
if (!TOKEN) { console.error('usage: node e2e-sfu-consume.cjs <token>'); process.exit(1); }

const CHROME = '/home/maxsense/.cache/ms-playwright/chromium_headless_shell-1232/chrome-headless-shell-linux64/chrome-headless-shell';
const APP = 'http://127.0.0.1:5173';

(async () => {
  const browser = await chromium.launch({ executablePath: CHROME, headless: process.env.HEADFUL ? false : true,
    args: ['--disable-features=WebRtcHideLocalIpsWithMdns'] });
  const page = await browser.newPage();
  const logs = [];
  page.on('console', (m) => logs.push(`[${m.type()}] ${m.text()}`));
  page.on('pageerror', (e) => logs.push(`[pageerror] ${String(e).slice(0, 300)}`));

  // 1. Set token + reload
  await page.goto(APP, { waitUntil: 'domcontentloaded' });
  await page.evaluate((t) => localStorage.setItem('mediaservo_admin_token', t), TOKEN);
  await page.reload({ waitUntil: 'domcontentloaded' });
  console.log('token set, waiting for device list...');

  // 2. Wait for device with Play button (test-room)
  const playBtn = page.locator('.btn-play').first();
  await playBtn.waitFor({ timeout: 15000 });
  console.log('device list loaded');
  await page.screenshot({ path: '/tmp/e2e-1-devices.png' });

  // 3. Click Play → VideoPlayer mounts → SFU connect/consume
  await playBtn.click();
  console.log('Play clicked, waiting for video...');

  // 4. Wait for video element with actual frames (videoWidth > 0)
  // PIT-56: 必须等真实帧 (videoWidth>0) — readyState>=2 会立即满足 (media 已加载但无解码帧)
  const videoReady = await page.waitForFunction(() => {
    const v = document.querySelector('video');
    return v && v.videoWidth > 0;
  }, { timeout: 90000 }).then(() => true).catch(() => false); // PIT-62: SquaresPattern 关键帧间隔 ~40s
  console.log(`video ready (width>0): ${videoReady}`);

  const vidInfo = await page.evaluate(() => {
    const v = document.querySelector('video');
    return v ? { w: v.videoWidth, h: v.videoHeight, rs: v.readyState, paused: v.paused } : null;
  });
  console.log('video info:', JSON.stringify(vidInfo));

  // PIT-64: Chrome 解码统计 (video 接收端)
  await new Promise(r => setTimeout(r, 5000)); // 给解码器时间
  const stats = await page.evaluate(async () => {
    const pc = window.__sfuPc;
    if (!pc) return { error: 'no pc' };
    const reports = await pc.getStats();
    const out = { video: {}, audio: {}, transport: {} };
    reports.forEach((r) => {
      if (r.type === 'inbound-rtp' && r.kind === 'video') {
        out.video = {
          bytesReceived: r.bytesReceived, packetsReceived: r.packetsReceived,
          framesDecoded: r.framesDecoded, framesReceived: r.framesReceived,
          keyFramesDecoded: r.keyFramesDecoded, decoderImplementation: r.decoderImplementation,
          jitter: r.jitter, droppedFrames: r.framesDropped,
        };
      }
    });
    reports.forEach((r) => {
      if (r.type === 'transport') {
        out.transport = { dtlsState: r.dtlsState, selectedCandidatePairState: r.selectedCandidatePairState };
      }
    });
    return out;
  });
  console.log('decoder stats:', JSON.stringify(stats));

  await page.screenshot({ path: '/tmp/e2e-2-playing.png' });
  console.log('--- console logs ---');
  logs.forEach((l) => console.log(l));
  await browser.close();
})().catch((e) => { console.error('E2E FAILED:', e.message); process.exit(1); });

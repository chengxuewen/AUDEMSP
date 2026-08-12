// 增量码率探针: 视频就绪后 15s 窗口, 2 次采样算 delta bitrate + 分辨率
const { chromium } = require('playwright');
const CHROME = '/home/maxsense/.cache/ms-playwright/chromium_headless_shell-1232/chrome-headless-shell-linux64/chrome-headless-shell';
const APP = 'http://127.0.0.1:5173';
const TOKEN = process.argv[2];
(async () => {
  const browser = await chromium.launch({ executablePath: CHROME, headless: true,
    args: ['--disable-features=WebRtcHideLocalIpsWithMdns'] });
  const page = await browser.newPage();
  page.on('console', m => { const t = m.text(); if (t.includes('error') || t.includes('transport-cc')) console.log('[console]', t.slice(0,200)); });
  await page.goto(APP, { waitUntil: 'domcontentloaded' });
  await page.evaluate(t => localStorage.setItem('audemsp_admin_token', t), TOKEN);
  await page.reload({ waitUntil: 'domcontentloaded' });
  const playBtn = page.locator('.btn-play').first();
  await playBtn.waitFor({ timeout: 15000 });
  await playBtn.click();
  await page.waitForFunction(() => { const v = document.querySelector('video'); return v && v.videoWidth > 0; }, { timeout: 90000 });
  console.log('video ready, waiting 15s for BWE ramp...');
  await new Promise(r => setTimeout(r, 15000));
  const sample = async (label) => {
    const s = await page.evaluate(async () => {
      const pc = window.__sfuPc;
      if (!pc) return null;
      const reports = await pc.getStats();
      let v = null;
      reports.forEach(r => { if (r.type === 'inbound-rtp' && r.kind === 'video') v = { bytes: r.bytesReceived, frames: r.framesDecoded, keyFrames: r.keyFramesDecoded, jitter: r.jitter }; });
      const vid = document.querySelector('video');
      return { ...v, vw: vid?.videoWidth, vh: vid?.videoHeight };
    });
    console.log(label, JSON.stringify(s));
    return s;
  };
  const s1 = await sample('T1:');
  await new Promise(r => setTimeout(r, 10000));
  const s2 = await sample('T2:');
  if (s1 && s2) {
    const bits = (s2.bytes - s1.bytes) * 8;
    console.log(`bitrate: ${(bits / 10 / 1000).toFixed(0)} kbps over 10s (frames ${s2.frames - s1.frames}, keyframes ${s2.keyFrames - s1.keyFrames})`);
  }
  await browser.close();
})().catch(e => { console.error('FAILED:', e.message); process.exit(1); });

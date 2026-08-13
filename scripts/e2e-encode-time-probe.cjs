// T4 实证: play → 等 encoder_status → 打开 stats 面板 → 读"平均编码耗时"
const { chromium } = require('playwright');
const CHROME = '/home/maxsense/.cache/ms-playwright/chromium_headless_shell-1232/chrome-headless-shell-linux64/chrome-headless-shell';
const APP = process.env.APP_URL || 'http://127.0.0.1:5173';
const TOKEN = process.argv[2];
(async () => {
  const browser = await chromium.launch({ executablePath: CHROME, headless: true,
    args: ['--disable-features=WebRtcHideLocalIpsWithMdns'] });
  const page = await browser.newPage();
  const encLogs = [];
  page.on('console', m => { const t = m.text(); if (t.includes('encoder_status')) encLogs.push(t.slice(0, 160)); });
  await page.goto(APP, { waitUntil: 'domcontentloaded' });
  await page.evaluate(t => localStorage.setItem('audemsp_admin_token', t), TOKEN);
  await page.reload({ waitUntil: 'domcontentloaded' });
  const candidates = await page.evaluate(() =>
    Array.from(document.querySelectorAll('*')).filter(el => el.children.length === 0 && el.textContent.trim().length > 0)
      .map(el => ({ tag: el.tagName, cls: String(el.className).slice(0, 30), text: el.textContent.trim().slice(0, 25) }))
      .filter(x => x.text.includes('Play') || x.text.includes('play') || x.cls.includes('btn'))
      .slice(0, 8));
  await page.waitForFunction(() => document.body.innerText.includes('Play'), { timeout: 20000 });
  await new Promise(r => setTimeout(r, 2000));
  await page.evaluate(() => {
    const el = Array.from(document.querySelectorAll('*')).find(e => e.textContent.includes('Play') && e.textContent.trim().length < 30);
    if (el) el.click();
  });
  await page.waitForFunction(() => { const v = document.querySelector('video'); return v && v.videoWidth > 0; }, { timeout: 90000 });
  console.log('video ready');
  await new Promise(r => setTimeout(r, 8000)); // 等 ≥4 个 encoder_status 周期
  // 打开 stats 面板
  await page.evaluate(() => {
    const bar = document.querySelector('.vp-metrics-bar');
    if (bar) bar.click();
  });
  await new Promise(r => setTimeout(r, 1500));
  const panelText = await page.evaluate(() => {
    const panel = document.querySelector('.vp-stats-panel');
    return panel ? panel.textContent : null;
  });
  console.log('=== stats 面板文本 ===');
  console.log(panelText ? panelText.replace(/\n+/g, ' | ').slice(0, 600) : '(panel not found)');
  console.log('=== encoder_status 日志 ===');
  encLogs.slice(0, 3).forEach(l => console.log(l));
  await browser.close();
})().catch(e => { console.error('FAILED:', e.message); process.exit(1); });

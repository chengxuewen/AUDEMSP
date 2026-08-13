// 9800/admin 全链路: play → video → 编码耗时
const { chromium } = require('playwright');
const CHROME = '/home/maxsense/.cache/ms-playwright/chromium_headless_shell-1232/chrome-headless-shell-linux64/chrome-headless-shell';
(async () => {
  const browser = await chromium.launch({ executablePath: CHROME, headless: true,
    args: ['--disable-features=WebRtcHideLocalIpsWithMdns'] });
  const page = await browser.newPage();
  page.on('console', m => { const t = m.text(); if (t.includes('error') || t.includes('encoder_status')) console.log('[console]', t.slice(0, 120)); });
  await page.goto('http://127.0.0.1:9800/admin', { waitUntil: 'networkidle', timeout: 30000 });
  await page.evaluate(t => localStorage.setItem('audemsp_admin_token', t), process.argv[2]);
  await page.reload({ waitUntil: 'networkidle', timeout: 30000 });
  console.log('page ready, waiting devices...');
  await page.waitForFunction(() => document.body.innerText.includes('Play'), { timeout: 20000 });
  console.log('Play found, clicking...');
  await new Promise(r => setTimeout(r, 1000));
  await page.evaluate(() => {
    const el = Array.from(document.querySelectorAll('*')).find(e => e.textContent.includes('Play') && e.textContent.trim().length < 30);
    if (el) el.click();
  });
  const ok = await page.waitForFunction(() => { const v = document.querySelector('video'); return v && v.videoWidth > 0; }, { timeout: 90000 })
    .then(() => true).catch(() => false);
  console.log('video rendered:', ok);
  await new Promise(r => setTimeout(r, 8000));
  await page.evaluate(() => document.querySelector('.vp-metrics-bar')?.click());
  await new Promise(r => setTimeout(r, 1500));
  const panel = await page.evaluate(() => document.querySelector('.vp-stats-panel')?.textContent || '(no panel)');
  const hasEncode = panel ? panel.includes('平均编码耗时') && !panel.includes('ms/帧</span>—') : false;
  console.log('panel:', panel ? panel.replace(/\n+/g, ' | ').slice(0, 500) : panel);
  console.log('PASS 平均编码耗时:', panel?.match(/平均编码耗时\s*([\d.]+ms\/帧|—)/)?.[1]);
  await browser.close();
})().catch(e => { console.error('FAILED:', e.message); process.exit(1); });

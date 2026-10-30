// Makes the launcher icons in res/ from icon/*.svg (run when the pictures change; the PNGs are committed).
//   NODE_PATH=$(npm root -g) node tools/android/make-icons.js        (needs playwright and its chromium)
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const here = __dirname;
const densities = { mdpi: 1, hdpi: 1.5, xhdpi: 2, xxhdpi: 3, xxxhdpi: 4 };
(async () => {
  const browser = await chromium.launch();
  const render = async (svgFile, size, out) => {
    const svg = fs.readFileSync(path.join(here, 'icon', svgFile), 'utf8');
    const page = await browser.newPage({ viewport: { width: size, height: size } });
    await page.setContent(`<body style="margin:0;background:transparent">${svg.replace('width="108" height="108"', `width="${size}" height="${size}"`)}</body>`);
    fs.mkdirSync(path.dirname(out), { recursive: true });
    await page.screenshot({ path: out, omitBackground: true, clip: { x: 0, y: 0, width: size, height: size } });
    await page.close();
  };
  for (const [name, scale] of Object.entries(densities)) {
    await render('legacy.svg', Math.round(48 * scale), path.join(here, 'res', `mipmap-${name}`, 'ic_launcher.png'));
    await render('foreground.svg', Math.round(108 * scale), path.join(here, 'res', `mipmap-${name}`, 'ic_launcher_foreground.png'));
  }
  await browser.close();
})();

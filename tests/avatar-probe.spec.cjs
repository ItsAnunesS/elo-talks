// Synthetic documents only: no WhatsApp account or network access is required.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const probe = fs.readFileSync(path.join(__dirname, "../src-tauri/src/avatar-probe.js"), "utf8");
(async () => {
  const browser = await chromium.launch({
    executablePath: process.env.CHROME_BIN,
    headless: true,
    chromiumSandbox: true,
  });
  try {
    const page = await browser.newPage();
    let requests = 0;
    await page.route("**/*", (route) => {
      requests++;
      return route.fulfill({
        contentType: "text/html",
        body: '<div id="app"><nav role="navigation"><button aria-label="Profile"></button></nav><main id="main"><img id="contact"></main></div>',
      });
    });
    await page.goto("https://web.whatsapp.com/");
    const photo = await page.evaluate(() => {
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = 64;
      const context = canvas.getContext("2d");
      context.fillStyle = "#bb66ff";
      context.fillRect(0, 0, 64, 64);
      return canvas.toDataURL();
    });
    await page.evaluate(async (photo) => {
      const contact = document.querySelector("#contact");
      contact.src = photo;
      await contact.decode();
    }, photo);
    assert.equal(await page.evaluate(probe), "", "Never fall back to a conversation photo");
    await page.evaluate(async (photo) => {
      const image = new Image();
      image.src = photo;
      await image.decode();
      document.querySelector('button[aria-label="Profile"]').append(image);
      window.fetch = () => {
        throw Error("Fetching is forbidden");
      };
      Storage.prototype.setItem = () => {
        throw Error("Storage is forbidden");
      };
    }, photo);
    const pixels = await page.evaluate(probe);
    assert.equal(pixels.length, 64 * 64 * 4 * 2);
    assert.equal(pixels.slice(0, 8), "bb66ffff");
    await page.evaluate(() =>
      document.querySelector("nav").append(document.querySelector("nav button").cloneNode(true)),
    );
    assert.equal(await page.evaluate(probe), "", "Ambiguous profile controls must retain initials");
    await page.evaluate(() => document.querySelector("nav").replaceChildren());
    assert.equal(await page.evaluate(probe), "", "Logout/missing profile must clear the avatar");
    await page.evaluate(async (photo) => {
      const button = document.createElement("button");
      button.setAttribute("aria-label", "Profile");
      const image = new Image();
      image.src = photo;
      await image.decode();
      button.append(image);
      document.querySelector("nav").append(button);
      CanvasRenderingContext2D.prototype.getImageData = () => {
        throw new DOMException("Tainted canvas", "SecurityError");
      };
    }, photo);
    assert.equal(await page.evaluate(probe), "", "Canvas security errors must retain initials");
    assert.equal(requests, 1, "The probe must never request a photo URL");
    await page.goto("https://example.com/");
    assert.equal(await page.evaluate(probe), "", "Other origins must not be inspected");
    console.log(
      "PASS: own-avatar pixels, missing/ambiguous profile, contact exclusion, origin restriction and no network/storage writes.",
    );
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});

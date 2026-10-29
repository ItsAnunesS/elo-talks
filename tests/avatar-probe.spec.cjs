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
    assert.equal(await page.evaluate(probe), "no-photo", "Never fall back to a conversation photo");
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
    await page.evaluate(() => {
      const copy = document.querySelector("nav button").cloneNode(true);
      copy.style.display = "none";
      document.querySelector("nav").append(copy);
      const contactHeader = document.createElement("header");
      contactHeader.append(document.querySelector("nav button").cloneNode(true));
      document.querySelector("#main").append(contactHeader);
    });
    assert.equal(
      await page.evaluate(probe),
      pixels,
      "Hidden duplicates and contact headers must be ignored",
    );
    await page.evaluate(() => {
      const header = document.createElement("header");
      header.id = "current-navigation";
      const button = document.createElement("div");
      button.setAttribute("role", "button");
      button.setAttribute("aria-label", "Perfil");
      button.append(document.querySelector("nav img").cloneNode(true));
      header.append(button);
      document.querySelector("#app").prepend(header);
      document.querySelector("nav").style.display = "none";
    });
    assert.equal(
      await page.evaluate(probe),
      pixels,
      "Header profile controls do not require navbar test IDs",
    );
    await page.evaluate(() => {
      document.querySelector("#current-navigation").remove();
      document.querySelector("nav").style.display = "";
    });
    await page.evaluate(() =>
      document.querySelector("nav").append(document.querySelector("nav button").cloneNode(true)),
    );
    assert.equal(
      await page.evaluate(probe),
      "ambiguous",
      "Ambiguous profile controls must retain initials",
    );
    await page.evaluate(() => document.querySelector("nav").replaceChildren());
    assert.equal(
      await page.evaluate(probe),
      "control-not-found",
      "Logout/missing profile must clear the avatar",
    );
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
    assert.equal(
      await page.evaluate(probe),
      "canvas-blocked",
      "Canvas security errors must retain initials",
    );
    assert.equal(requests, 1, "The probe must never request a photo URL");
    await page.goto("https://example.com/");
    assert.equal(await page.evaluate(probe), "wrong-origin", "Other origins must not be inspected");
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

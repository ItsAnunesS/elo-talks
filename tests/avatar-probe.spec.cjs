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
    // The reported live tooltip is "Você"; labels can belong to tabs or nested spans.
    for (const markup of [
      '<header><button aria-label="Você"><img></button></header>',
      '<header><div role="tab" title="Você"><img></div></header>',
      '<div role="tablist"><button title="You"><img></button></div>',
      '<header><div role="tab" aria-label="You"><img></div></header>',
      '<header><button><span title="Você"></span><img></button></header>',
      '<header><button><span hidden title="Você"></span><img></button></header>',
      '<header><button aria-labelledby="self-label"><img></button><span id="self-label" hidden>Você</span></header>',
      '<header><button aria-describedby="self-tooltip"><img></button></header><span role="tooltip" id="self-tooltip">You</span>',
      '<header><button data-navbar-item="profile"><img></button></header>',
      '<div><button data-navbar-item="profile"><img></button></div>',
      '<div><button data-navbar-item="true" aria-label="Você"><img></button></div>',
      '<header><button><img alt="Your profile picture"></button></header>',
      '<header><button><img alt="Sua foto de perfil"></button></header>',
    ]) {
      await page.evaluate(
        async ({ markup, photo }) => {
          document.querySelector("nav").style.display = "none";
          const fixture = document.createElement("section");
          fixture.id = "profile-variant";
          fixture.innerHTML = markup;
          document.querySelector("#app").prepend(fixture);
          const image = fixture.querySelector("img");
          image.src = photo;
          await image.decode();
          // Ensure nested labels have a rendered box without including a photo themselves.
          for (const label of fixture.querySelectorAll("span")) label.textContent = "Você";
        },
        { markup, photo },
      );
      assert.equal(await page.evaluate(probe), pixels, `Recognize self-profile variant: ${markup}`);
      await page.evaluate(() => document.querySelector("#profile-variant").remove());
    }
    for (const markup of [
      '<div id="main"><header><button title="Você"><img></button></header></div>',
      '<div id="side"><header><button aria-label="You"><img></button></header></div>',
      '<header><div role="row"><button title="Você"><img></button></div></header>',
      '<header><div role="listitem"><button title="You"><img></button></div></header>',
      '<div role="dialog"><header><button title="Você"><img></button></header></div>',
      '<header><div role="menu"><button title="Você"><img></button></div></header>',
      '<section><button title="Você"><img></button></section>',
      '<header><button title="You are invited"><img></button></header>',
      '<header><button><img alt="Profile picture"></button></header>',
      "<header><button><img></button></header>",
      '<header><button aria-labelledby="message-label"><img></button></header><main id="main"><span id="message-label">Você</span></main>',
      '<header><button aria-labelledby="nested-label"><img></button><span id="nested-label"><span>Você</span></span></header>',
      '<header><button aria-labelledby="wrong-label"><img></button><span id="wrong-label">Contact photo</span></header>',
    ]) {
      await page.evaluate(
        async ({ markup, photo }) => {
          const fixture = document.createElement("section");
          fixture.id = "profile-variant";
          fixture.innerHTML = markup;
          document.querySelector("#app").prepend(fixture);
          const image = fixture.querySelector("img");
          image.src = photo;
          await image.decode();
        },
        { markup, photo },
      );
      assert.equal(
        await page.evaluate(probe),
        "control-not-found",
        `Reject unsafe candidate: ${markup}`,
      );
      await page.evaluate(() => document.querySelector("#profile-variant").remove());
    }
    await page.evaluate(() => {
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
      "image-source-unsupported",
      "An unsupported tainted source must retain initials without another request",
    );
    assert.equal(requests, 1, "Readable and unsupported sources never trigger recovery requests");
    await page.goto("https://example.com/");
    assert.equal(await page.evaluate(probe), "wrong-origin", "Other origins must not be inspected");
    // Exercise real browser CORS checks with intercepted synthetic HTTPS resources.
    const cors = await browser.newPage();
    const png = Buffer.from(photo.split(",")[1], "base64");
    const reads = [];
    const pending = new Map();
    const delayResponse = (path) => {
      let release, signal;
      const response = new Promise((resolve) => {
        release = resolve;
      });
      const started = new Promise((resolve) => {
        signal = resolve;
      });
      pending.set(path, { response, signal });
      return { started, release };
    };
    await cors.context().addCookies([
      {
        name: "session",
        value: "synthetic-private",
        domain: "pps.whatsapp.net",
        path: "/",
        secure: true,
        sameSite: "None",
      },
    ]);
    await cors.route("**/*", async (route) => {
      const request = route.request();
      const url = new URL(request.url());
      if (url.hostname === "web.whatsapp.com")
        return route.fulfill({
          contentType: "text/html",
          body: '<meta charset="utf-8"><div id="app"><header><button aria-label="Você"><img></button></header></div>',
        });
      if (request.resourceType() === "image")
        return route.fulfill({ contentType: "image/png", body: png });
      reads.push({ url: request.url(), headers: await request.allHeaders() });
      const delay = pending.get(url.pathname);
      if (delay) {
        delay.signal();
        await delay.response;
      }
      if (url.pathname === "/redirect.png")
        return route.fulfill({
          status: 302,
          headers: {
            Location: "https://untrusted.example/photo.png",
            "Access-Control-Allow-Origin": "*",
          },
        });
      const headers = {
        "Access-Control-Allow-Origin":
          url.pathname === "/denied.png" ? "https://denied.example" : "*",
      };
      if (url.pathname === "/large.png") headers["Content-Length"] = String(1024 * 1024);
      return route.fulfill({
        contentType: "image/png",
        headers,
        body:
          url.pathname === "/svg.png"
            ? Buffer.from("<svg/>")
            : url.pathname === "/large.png"
              ? Buffer.alloc(1024 * 1024)
              : png,
      });
    });
    await cors.goto("https://web.whatsapp.com/");
    await cors.evaluate(() => {
      const nativeFetch = window.fetch.bind(window);
      window.fetch = (url, options) => {
        window.lastFetchOptions = { ...options, signal: undefined };
        return nativeFetch(url, options);
      };
      Storage.prototype.setItem = () => {
        throw Error("Avatar storage is forbidden");
      };
    });
    const setPhoto = async (url) =>
      cors.evaluate(async (url) => {
        const image = document.querySelector("header img");
        image.src = url;
        await image.decode();
      }, url);
    await setPhoto("https://pps.whatsapp.net/allowed.png");
    assert.equal(
      await cors.evaluate(probe),
      pixels,
      "A CORS-authorized read recovers a tainted avatar",
    );
    assert.equal(reads.length, 1);
    assert.equal(
      reads[0].headers.cookie,
      undefined,
      "Never send session cookies with the recovery request",
    );
    assert.equal(reads[0].headers.referer, undefined);
    assert.deepEqual(await cors.evaluate(() => window.lastFetchOptions), {
      mode: "cors",
      credentials: "omit",
      cache: "no-store",
      redirect: "error",
      referrerPolicy: "no-referrer",
      signal: undefined,
    });
    assert.equal(await cors.evaluate(probe), pixels);
    assert.equal(reads.length, 1, "Repeated probes reuse memory instead of refetching");
    for (const [path, expected] of [
      ["denied.png", "image-request-failed"],
      ["redirect.png", "image-request-failed"],
      ["large.png", "image-too-large"],
      ["svg.png", "image-request-failed"],
    ]) {
      await setPhoto(`https://pps.whatsapp.net/${path}`);
      assert.ok((await cors.evaluate(probe)) === expected, `${path}: expected ${expected}`);
      const count = reads.length;
      assert.equal(await cors.evaluate(probe), expected);
      assert.equal(reads.length, count, "Failures are rate-limited too");
    }
    const count = reads.length;
    await setPhoto("https://pps.whatsapp.net.untrusted.example/photo.png");
    assert.equal(await cors.evaluate(probe), "image-source-unsupported");
    assert.equal(reads.length, count, "Do not request unapproved hosts");
    assert.ok(
      reads.every((read) => new URL(read.url).hostname === "pps.whatsapp.net"),
      "Never follow a redirect",
    );
    const replacement = delayResponse("/pending-replacement.png");
    await setPhoto("https://pps.whatsapp.net/pending-replacement.png");
    const stalePhoto = cors.evaluate(probe);
    await replacement.started;
    await setPhoto("https://pps.whatsapp.net/replacement.png");
    assert.equal(await cors.evaluate(probe), pixels);
    replacement.release();
    assert.equal(
      await stalePhoto,
      "image-not-ready",
      "An older request cannot restore a previous photo",
    );
    const replacementReads = reads.length;
    assert.equal(await cors.evaluate(probe), pixels);
    assert.equal(reads.length, replacementReads, "A stale result must not clear the newer cache");
    const logout = delayResponse("/pending-logout.png");
    await setPhoto("https://pps.whatsapp.net/pending-logout.png");
    const loggedOutPhoto = cors.evaluate(probe);
    await logout.started;
    await cors.evaluate(() => document.querySelector("header").remove());
    assert.equal(await cors.evaluate(probe), "control-not-found");
    assert.equal(
      await cors.evaluate(() => "__eloSelfAvatarCors" in window),
      false,
      "Logout clears the recovery cache",
    );
    logout.release();
    assert.equal(await loggedOutPhoto, "image-not-ready", "Logout discards an in-flight photo");
    await cors.close();
    console.log(
      "PASS: own-avatar pixels, missing/ambiguous profile, contact exclusion, origin restriction, bounded credential-free CORS recovery and no storage writes.",
    );
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});

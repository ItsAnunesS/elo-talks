// Optional browser contract test; native/cofre validation is deliberately separate.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const http = require("node:http"),
  fs = require("node:fs"),
  path = require("node:path"),
  assert = require("node:assert/strict");
(async () => {
  const root = path.resolve(__dirname, "../ui");
  const server = http.createServer((req, res) => {
    const file = path.join(root, req.url === "/" ? "index.html" : req.url);
    if (!file.startsWith(root + path.sep)) {
      res.writeHead(403).end();
      return;
    }
    try {
      res.setHeader(
        "Content-Type",
        file.endsWith(".js")
          ? "text/javascript"
          : file.endsWith(".css")
            ? "text/css"
            : file.endsWith(".svg")
              ? "image/svg+xml"
              : "text/html",
      );
      res.end(fs.readFileSync(file));
    } catch {
      res.writeHead(404).end();
    }
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const browser = await chromium.launch({
    executablePath: process.env.CHROME_BIN,
    headless: true,
    chromiumSandbox: true,
  });
  try {
    const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.addInitScript(() => {
      window.testRequests = [];
      window.testListeners = {};
      window.testAccountStatus = {
        accounts: {
          a: { page: "ready", pendingNotifications: 3 },
          b: { page: "load-failed", pendingNotifications: 12 },
        },
        preferences: {},
      };
      window.testState = JSON.parse(localStorage.getItem("fixture") || "null") || {
        created: true,
        unlocked: false,
        pin: true,
        keys: [{ id: "key-one", name: "My key" }],
        accounts: [],
        idleMinutes: 0,
        version: "0.1.0",
      };
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            window.testRequests.push({ cmd, args });
            if (cmd === "account_snapshot") return structuredClone(window.testAccountStatus);
            if (cmd === "diagnostics")
              return {
                version: "0.1.0",
                accounts: [{ page: "load-failed", pendingNotifications: 12 }],
              };
            if (cmd === "request") {
              if (args.op === "status") return structuredClone(window.testState);
              if (args.op === "notifications") {
                window.testState.notifications = args.data;
                return structuredClone(window.testState);
              }
              if (args.op === "unlock") throw Error("Incorrect credential");
              if (args.op === "account.add") {
                window.testState.accounts.push({ id: "demo", name: args.data.name });
                return structuredClone(window.testState);
              }
            }
            return null;
          },
        },
        event: {
          listen: async (name, handler) => {
            window.testListeners[name] = handler;
            return () => {};
          },
        },
      };
    });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.locator(".wordmark img").evaluate((img) => img.decode());
    assert.equal(await page.locator(".wordmark img").evaluate((img) => img.naturalWidth), 512);
    await page.getByRole("button", { name: "PIN", exact: true }).click();
    await page.getByLabel("Digit 1", { exact: true }).fill("1");
    await page.keyboard.type("23456");
    assert.deepEqual(
      await page.locator(".pin-inputs input").evaluateAll((n) => n.map((n) => n.value)),
      ["1", "2", "3", "4", "5", "6"],
    );
    await page.getByRole("button", { name: "Unlock my space" }).click();
    await page.getByRole("alert").filter({ hasText: "Incorrect credential" }).waitFor();
    assert.equal(await page.locator(".pin-inputs input").first().inputValue(), "");
    await page.screenshot({ path: "/tmp/elo-pin.png" });
    await page.evaluate(() => {
      window.testState.unlocked = true;
      window.testState.accounts = [
        { id: "a", name: "Personal" },
        { id: "b", name: "Work" },
        { id: "c", name: "<img src=x onerror=alert(1)>" },
      ];
      localStorage.setItem("fixture", JSON.stringify(window.testState));
    });
    await page.reload();
    await page.locator("#workspace:not([hidden])").waitFor();
    assert.equal(await page.locator("#account-list img").count(), 0);
    await page.screenshot({ path: "/tmp/elo-workspace.png" });
    await page.evaluate(() => {
      window.originalAccountIcon = document.querySelector("#account-icons button");
      window.originalAccountCard = document.querySelector("#cards button");
    });
    await page.locator('#account-icons [data-account-id="a"]').click();
    await page.locator('#account-icons [data-account-id="a"].selected').waitFor();
    await page.locator('#account-icons [data-account-id="b"]').click();
    await page.locator('#account-icons [data-account-id="b"].selected').waitFor();
    assert.equal(
      await page
        .locator('#account-icons [data-account-id="a"]')
        .evaluate((element) => element.classList.contains("selected")),
      false,
    );
    assert.equal(
      await page.evaluate(
        () =>
          window.originalAccountIcon === document.querySelector("#account-icons button") &&
          window.originalAccountCard === document.querySelector("#cards button"),
      ),
      true,
    );
    await page.locator("#overview-button").click();
    await page.locator("#dashboard:not([hidden])").waitFor();
    await page.getByRole("button", { name: "Add account", exact: true }).click();
    await page.getByLabel("Account name", { exact: true }).fill("Project");
    await page.getByRole("button", { name: "Connect account" }).click();
    await page.getByRole("button", { name: "Project", exact: true }).last().waitFor();
    assert.equal(
      await page.locator('#account-list [data-account-id="b"] .notification-badge').textContent(),
      "12",
    );
    await page.locator('#account-icons [data-account-id="b"]').click();
    await page.getByRole("status").filter({ hasText: "Could not load" }).waitFor();
    await page.getByRole("button", { name: "Retry", exact: true }).click();
    assert.ok(
      await page.evaluate(() =>
        window.testRequests.some(
          (request) => request.cmd === "retry_account" && request.args.id === "b",
        ),
      ),
    );
    await page.evaluate(() =>
      window.testListeners["account-state"]({
        payload: { accounts: { b: { page: "tls-failed", pendingNotifications: 101 } } },
      }),
    );
    assert.equal(
      await page.locator('#account-list [data-account-id="b"] .notification-badge').textContent(),
      "99+",
    );
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByLabel("Do not disturb", { exact: true }).check();
    await page.getByLabel("Mute Work", { exact: true }).check();
    await page.getByRole("button", { name: "Save notification settings" }).click();
    await page.getByRole("status").filter({ hasText: "Notification settings saved" }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.testState.notifications), {
      doNotDisturb: true,
      mutedAccounts: ["b"],
    });
    await page.getByRole("button", { name: "Safe diagnostics", exact: true }).click();
    const diagnosticReport = await page.getByLabel("Diagnostic report").inputValue();
    assert.ok(diagnosticReport.includes("load-failed"));
    assert.ok(!diagnosticReport.includes("Personal") && !diagnosticReport.includes("Work"));
    await page.screenshot({ path: "/tmp/elo-diagnostics.png" });
    await page.getByRole("button", { name: "Close", exact: true }).click();
    await page.setViewportSize({ width: 900, height: 640 });
    await page.screenshot({ path: "/tmp/elo-status-badges.png" });
    assert.deepEqual(errors, []);
    console.log(
      "PASS: credentials, safe labels, account switching, badges, retry, notification preferences and private diagnostics.",
    );
  } finally {
    await browser.close();
    server.close();
  }
})().catch((e) => {
  console.error(e);
  process.exit(1);
});

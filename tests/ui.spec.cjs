// Synthetic browser contracts; native vault validation is separate.
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
              if (args.op === window.testFailOperation) {
                window.testFailOperation = null;
                throw Error("Synthetic save failure");
              }
              if (args.op === window.testHoldOperation)
                await new Promise((resolve) => {
                  window.testReleaseRequest = resolve;
                });
              if (args.op === "status") return structuredClone(window.testState);
              if (args.op === "notifications") {
                window.testState.notifications = args.data;
                return structuredClone(window.testState);
              }
              const organization = window.testState.organization;
              const data = args.data;
              if (args.op === "workspace.image") return window.testImages?.[data.id] || null;
              if (args.op === "workspace.add" || args.op === "workspace.update") {
                const item =
                  args.op === "workspace.add"
                    ? {
                        id: `workspace-${organization.workspaces.length}`,
                        name: data.name,
                        categories: [],
                        imageRevision: null,
                      }
                    : organization.workspaces.find((w) => w.id === data.id);
                item.name = data.name;
                if (data.imagePixels !== undefined) {
                  item.imageRevision = data.imagePixels === null ? null : `image-${Date.now()}`;
                  window.testImages ||= {};
                  window.testImages[item.id] =
                    data.imagePixels === null
                      ? null
                      : "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z/D/PwAG/gL+DHWJ3gAAAABJRU5ErkJggg==";
                }
                if (args.op === "workspace.add") organization.workspaces.push(item);
                return structuredClone(window.testState);
              }
              if (args.op === "category.add") {
                organization.workspaces
                  .find((w) => w.id === data.workspaceId)
                  .categories.push({ id: "operations", name: data.name });
                return structuredClone(window.testState);
              }
              if (args.op === "category.remove") {
                organization.workspaces.find((w) => w.id === data.workspaceId).categories = [];
                for (const placement of Object.values(organization.placements))
                  if (placement.categoryId === data.id) placement.categoryId = null;
                return structuredClone(window.testState);
              }
              if (args.op === "account.update") {
                window.testState.accounts.find((a) => a.id === data.id).name = data.name;
                organization.placements[data.id] = {
                  workspaceId: data.workspaceId,
                  categoryId: data.categoryId,
                  service: "whatsapp",
                };
                return structuredClone(window.testState);
              }
              if (args.op === "unlock") throw Error("Incorrect credential");
              if (args.op === "account.add") {
                const id = `account-${window.testState.accounts.length}`;
                window.testState.accounts.push({ id, name: args.data.name });
                window.testState.organization.placements[id] = {
                  workspaceId: args.data.workspaceId,
                  categoryId: args.data.categoryId,
                  service: "whatsapp",
                };
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
    await page.screenshot({ path: "/tmp/elo-unlock-premium.png" });
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
      window.testState.organization = {
        workspaces: [{ id: "personal", name: "My workspace", categories: [], imageRevision: null }],
        placements: Object.fromEntries(
          window.testState.accounts.map((a) => [
            a.id,
            { workspaceId: "personal", categoryId: null, service: "whatsapp" },
          ]),
        ),
      };
      localStorage.setItem("fixture", JSON.stringify(window.testState));
    });
    await page.reload();
    await page.locator("#workspace:not([hidden])").waitFor();
    assert.equal(await page.locator("#account-list .service-icon").count(), 3);
    assert.equal(await page.locator("#account-list img:not(.service-icon)").count(), 0);
    await page.screenshot({ path: "/tmp/elo-workspace.png" });
    await page.evaluate(() => {
      window.originalAccountIcon = document.querySelector("#account-list button");
      window.originalAccountCard = document.querySelector("#cards button");
    });
    await page.locator('#account-list [data-account-id="a"]').click();
    await page.locator('#account-list [data-account-id="a"].selected').waitFor();
    await page.locator('#account-list [data-account-id="b"]').click();
    await page.locator('#account-list [data-account-id="b"].selected').waitFor();
    assert.equal(
      await page
        .locator('#account-list [data-account-id="a"]')
        .evaluate((element) => element.classList.contains("selected")),
      false,
    );
    assert.equal(
      await page.evaluate(
        () =>
          window.originalAccountIcon === document.querySelector("#account-list button") &&
          window.originalAccountCard === document.querySelector("#cards button"),
      ),
      true,
    );
    await page.locator("#overview-button").click();
    await page.locator("#dashboard:not([hidden])").waitFor();
    await page.getByRole("button", { name: "Add channel", exact: true }).click();
    await page.getByLabel("Account name", { exact: true }).fill("Project");
    await page.screenshot({ path: "/tmp/elo-add-channel-premium.png" });
    await page.getByRole("button", { name: "Connect account" }).click();
    await page.locator('#account-list [data-account-id="account-3"].selected').waitFor();
    await page.getByRole("button", { name: "Project", exact: true }).last().waitFor();
    assert.equal(
      await page.locator('#account-list [data-account-id="b"] .notification-badge').textContent(),
      "12",
    );
    await page.locator('#account-list [data-account-id="b"]').click();
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
    await page.screenshot({ path: "/tmp/elo-settings-premium.png" });
    await page.getByRole("tab", { name: "Notifications", exact: true }).click();
    await page.getByLabel("Do not disturb", { exact: true }).check();
    await page.getByLabel("Mute Work", { exact: true }).check();
    await page.getByRole("button", { name: "Save notification settings" }).click();
    await page.getByRole("status").filter({ hasText: "Notification settings saved" }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.testState.notifications), {
      doNotDisturb: true,
      mutedAccounts: ["b"],
    });
    await page.getByRole("tab", { name: "General", exact: true }).click();
    await page.getByRole("button", { name: "Safe diagnostics", exact: true }).click();
    const diagnosticReport = await page.getByLabel("Diagnostic report").inputValue();
    assert.ok(diagnosticReport.includes("load-failed"));
    assert.ok(!diagnosticReport.includes("Personal") && !diagnosticReport.includes("Work"));
    await page.screenshot({ path: "/tmp/elo-diagnostics.png" });
    await page.getByRole("button", { name: "Close", exact: true }).click();
    await page.setViewportSize({ width: 900, height: 640 });
    await page.screenshot({ path: "/tmp/elo-status-badges.png" });
    await page.getByRole("button", { name: "Overview", exact: false }).click();
    await page.evaluate(() => {
      const originalSetItem = Storage.prototype.setItem;
      Storage.prototype.setItem = () => {
        throw new Error("Avatar storage is forbidden");
      };
      try {
        window.testListeners["account-avatar"]({
          payload: { id: "a", pixels: new Array(64 * 64 * 4).fill(200) },
        });
      } finally {
        Storage.prototype.setItem = originalSetItem;
      }
    });
    await page.locator('#cards [data-avatar-account="a"] img').waitFor();
    assert.equal(
      await page.locator('#account-list [data-account-id="a"] .notification-badge').textContent(),
      "3",
    );
    assert.equal(await page.locator('[data-avatar-account="a"] img').count(), 1);
    await page.evaluate(() =>
      window.testListeners["account-avatar"]({
        payload: { id: "b", pixels: "https://example.com/photo" },
      }),
    );
    assert.equal(await page.locator('[data-avatar-account="b"] img').count(), 0);
    await page.evaluate(() =>
      window.testListeners["account-avatar"]({ payload: { id: "a", pixels: null } }),
    );
    assert.equal(await page.locator('[data-avatar-account="a"] img').count(), 0);
    assert.equal(await page.locator('#cards [data-avatar-account="a"]').textContent(), "PE");
    await page.getByRole("button", { name: "Add workspace", exact: true }).click();
    await page.getByLabel("Workspace name", { exact: true }).fill("Company");
    await page.getByLabel("Workspace image", { exact: true }).setInputFiles({
      name: "icon.png",
      mimeType: "image/png",
      buffer: Buffer.from(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z/D/PwAG/gL+DHWJ3gAAAABJRU5ErkJggg==",
        "base64",
      ),
    });
    await page.locator(".workspace-image-preview:not([hidden])").waitFor();
    await page.locator("#dialog-submit").click();
    await page.locator('#workspace-icons [data-workspace-id="workspace-1"].selected img').waitFor();
    assert.equal(await page.locator("#account-list .account-row").count(), 0);
    assert.equal(
      await page.evaluate(
        () =>
          window.testRequests.find((r) => r.args?.op === "workspace.add").args.data.imagePixels
            .length,
      ),
      32768,
    );
    await page.getByRole("button", { name: "Add category", exact: false }).click();
    await page.getByLabel("Category name").fill("Operations");
    await page.locator("#dialog-submit").click();
    await page.locator(".empty-category").waitFor();
    await page.getByRole("button", { name: "Add channel", exact: true }).click();
    await page.getByLabel("Account name", { exact: true }).fill("Business BR");
    await page.getByLabel("Category", { exact: true }).selectOption("operations");
    await page.locator("#dialog-submit").click();
    await page.locator('.channel-category [data-account-id="account-4"]').waitFor();
    await page.getByRole("button", { name: "My workspace", exact: true }).click();
    await page.getByRole("button", { name: "Manage Project", exact: true }).click();
    await page.getByLabel("Workspace", { exact: true }).selectOption("workspace-1");
    assert.equal(await page.getByLabel("Category", { exact: true }).inputValue(), "");
    await page.locator("#dialog-submit").click();
    await page.getByRole("button", { name: "Company", exact: true }).click();
    await page.locator('#account-list > [data-account-id="account-3"]').waitFor();
    await page.evaluate(() =>
      window.testListeners["account-state"]({
        payload: {
          accounts: {
            a: { page: "ready", pendingNotifications: 5 },
            "account-4": { page: "ready", pendingNotifications: 7 },
          },
        },
      }),
    );
    assert.equal(
      await page.locator('[data-workspace-id="personal"] .notification-badge').textContent(),
      "5",
    );
    assert.equal(
      await page.locator('[data-workspace-id="workspace-1"] .notification-badge').textContent(),
      "7",
    );
    await page.locator(".category-toggle").click();
    await page.evaluate(() => window.testListeners["open-account"]({ payload: "account-4" }));
    await page.locator('.channel-category [data-account-id="account-4"].selected').waitFor();
    assert.equal(await page.locator(".category-toggle").getAttribute("aria-expanded"), "true");
    await page.getByRole("button", { name: "Manage workspace", exact: true }).click();
    assert.equal(
      await page.getByRole("button", { name: "Delete workspace", exact: true }).isDisabled(),
      true,
    );
    await page.getByRole("button", { name: "Close", exact: true }).click();
    await page.evaluate(() => window.testListeners["open-account"]({ payload: "a" }));
    await page.locator('[data-workspace-id="personal"].selected').waitFor();
    await page.locator('#account-list [data-account-id="a"].selected').waitFor();
    await page.getByRole("button", { name: "Company", exact: true }).click();
    await page.screenshot({ path: "/tmp/elo-workspaces-channels.png" });
    await page.getByRole("button", { name: "Manage category Operations", exact: true }).click();
    await page.getByRole("button", { name: "Delete category", exact: true }).click();
    await page.locator('#account-list > [data-account-id="account-4"]').waitFor();
    assert.equal(await page.locator(".channel-category").count(), 0);
    await page.getByRole("button", { name: "Manage workspace", exact: true }).click();
    await page.getByLabel("Workspace image", { exact: true }).setInputFiles({
      name: "unsafe.svg",
      mimeType: "image/svg+xml",
      buffer: Buffer.from("<svg/>"),
    });
    await page.locator("#dialog-error").filter({ hasText: "PNG, JPEG or WebP" }).waitFor();
    await page
      .getByLabel("Workspace image", { exact: true })
      .setInputFiles({ name: "renamed.png", mimeType: "image/png", buffer: Buffer.from("<svg/>") });
    await page.locator("#dialog-error").filter({ hasText: "File contents do not match" }).waitFor();
    await page.getByRole("button", { name: "Remove image", exact: true }).click();
    await page.locator("#dialog-submit").click();
    await page.locator("#dialog").waitFor({ state: "hidden" });
    assert.equal(await page.locator('[data-workspace-id="workspace-1"] img').count(), 0);
    await page.evaluate(() => localStorage.setItem("fixture", JSON.stringify(window.testState)));
    await page.reload();
    await page.getByRole("button", { name: "Company", exact: true }).click();
    assert.equal(await page.locator("#account-list .account-row").count(), 2);
    // Search crosses workspace boundaries and restores the chosen native account.
    await page.getByRole("button", { name: "Find a channel", exact: true }).click();
    assert.equal(
      await page
        .getByLabel("Search channels")
        .evaluate((input) => input === document.activeElement),
      true,
    );
    await page.getByLabel("Search channels").fill("Business");
    assert.equal(await page.locator(".switcher-result").count(), 1);
    await page.screenshot({ path: "/tmp/elo-search-premium.png" });
    await page.getByLabel("Search channels").press("ArrowDown");
    await page.keyboard.press("Enter");
    await page.locator('#account-list [data-account-id="account-4"].selected').waitFor();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("tab", { name: "Security", exact: true }).click();
    await page.getByLabel("Current master password", { exact: true }).fill("synthetic-only-secret");
    await page.getByLabel("Current master password", { exact: true }).press("Enter");
    assert.equal(await page.locator("#dialog").evaluate((dialog) => dialog.open), true);
    await page.getByRole("tab", { name: "General", exact: true }).click();
    await page.getByRole("tab", { name: "Security", exact: true }).click();
    assert.equal(
      await page.getByLabel("Current master password", { exact: true }).inputValue(),
      "",
    );
    await page.screenshot({ path: "/tmp/elo-security-premium.png" });
    await page.keyboard.press("Escape");
    await page.locator("#dialog").waitFor({ state: "hidden" });
    await page.getByRole("button", { name: "Manage Business BR", exact: true }).click();
    await page.getByRole("button", { name: "Remove this account", exact: true }).click();
    await page.getByRole("button", { name: "Confirm remove this account", exact: true }).waitFor();
    await page.getByRole("button", { name: "Keep it", exact: true }).click();
    assert.equal(
      await page.evaluate(
        () => window.testRequests.filter((r) => r.args?.op === "account.remove").length,
      ),
      0,
    );
    await page.getByLabel("Account name", { exact: true }).fill("Business renamed");
    await page.evaluate(() => {
      window.testFailOperation = "account.update";
    });
    await page.locator("#dialog-submit").click();
    await page.locator("#dialog-error").filter({ hasText: "Synthetic save failure" }).waitFor();
    assert.equal(
      await page.getByLabel("Account name", { exact: true }).inputValue(),
      "Business renamed",
    );
    assert.equal(await page.locator("#dialog-submit").isEnabled(), true);
    await page.locator("#dialog-submit").click();
    await page.getByRole("button", { name: "Business renamed", exact: true }).waitFor();
    await page.getByRole("button", { name: "Overview", exact: false }).click();
    for (const width of [900, 1200, 1600]) {
      await page.setViewportSize({ width, height: 800 });
      assert.deepEqual(
        await page.evaluate(() => ({
          sidebar: document.querySelector(".sidebar").getBoundingClientRect().right,
          header: document.querySelector(".topbar").getBoundingClientRect().bottom,
          overflow: document.documentElement.scrollWidth > innerWidth,
        })),
        { sidebar: 304, header: 72, overflow: false },
      );
      await page.screenshot({ path: `/tmp/elo-premium-${width}.png` });
    }
    await page.setViewportSize({ width: 900, height: 640 });
    await page.getByRole("button", { name: "Add workspace", exact: true }).click();
    await page.getByLabel("Workspace name", { exact: true }).fill("Pending workspace");
    await page.evaluate(() => {
      window.testHoldOperation = "workspace.add";
    });
    await page.locator("#dialog-submit").click();
    await page.getByRole("button", { name: "Saving…", exact: true }).waitFor();
    assert.equal(await page.locator("#dialog-cancel").isDisabled(), true);
    await page.evaluate(() =>
      document
        .querySelector("#dialog-form")
        .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })),
    );
    assert.equal(
      await page.evaluate(
        () => window.testRequests.filter((r) => r.args?.op === "workspace.add").length,
      ),
      1,
    );
    await page.evaluate(() => {
      window.testHoldOperation = null;
      window.testReleaseRequest();
    });
    await page.locator("#empty-workspace:not([hidden])").waitFor();
    await page.screenshot({ path: "/tmp/elo-empty-premium.png" });
    assert.deepEqual(errors, []);
    console.log(
      "PASS: credentials, safe labels, account switching, workspaces, raster uploads, category moves, notification badges and private diagnostics.",
    );
  } finally {
    await browser.close();
    server.close();
  }
})().catch((e) => {
  console.error(e);
  process.exit(1);
});

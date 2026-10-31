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
          a: { page: "ready", readiness: "chats", pendingNotifications: 3 },
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
      if (localStorage.getItem("startupScenario")) {
        window.testAccountStatus.accounts.a = { page: "loading", readiness: "preparing" };
        window.testAccountStatus.accounts.b = { page: "ready", readiness: "preparing" };
      }
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            window.testRequests.push({ cmd, args });
            if (cmd === "test_notification" && window.testNotificationError)
              throw Error("Linux notification service unavailable");
            if (
              cmd === "sync_accounts" &&
              localStorage.getItem("startupScenario") &&
              args.accountId === "a"
            )
              throw Error("Synthetic background failure");
            if (cmd === "account_avatar") return window.testAvatars?.[args.id] || null;
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
              if (args.op === "update.preferences")
                return { intervalMinutes: window.testUpdateInterval || 5 };
              if (args.op === "update.configure") {
                window.testUpdateInterval = args.data.intervalMinutes;
                return { intervalMinutes: window.testUpdateInterval };
              }
              if (args.op === "update.startup") return { automatic: false };
              if (args.op === "update.status")
                return window.testUpdateStatus || { phase: "unsupported" };
              if (args.op === "update.check") {
                window.testUpdateStatus = { phase: "current", canCheck: false, retryInSeconds: 60 };
                return window.testUpdateStatus;
              }
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
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-unlock-premium.png" });
    await page.getByRole("button", { name: "Minimize window", exact: true }).click();
    assert.equal(await page.evaluate(() => window.testRequests.at(-1).args.action), "minimize");
    await page.getByRole("button", { name: "Open application menu", exact: true }).click();
    await page.locator("#dialog").waitFor();
    assert.equal(
      await page.getByRole("button", { name: "Quit Elo Talks", exact: true }).count(),
      1,
    );
    await page.locator("#dialog-cancel").click();

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
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-pin.png" });
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
    await page.locator("#account-list").waitFor();
    assert.equal(await page.locator("#account-list .service-icon").count(), 3);
    assert.equal(await page.locator("#account-list img:not(.service-icon)").count(), 0);
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-workspace.png" });
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
    const avatarSource = await page.evaluate(() => {
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = 64;
      canvas.getContext("2d").fillRect(0, 0, 64, 64);
      return canvas.toDataURL("image/png");
    });
    await page.evaluate((source) => {
      window.testAvatars = { a: source };
      window.testListeners["account-avatar"]({ payload: { id: "a", available: true } });
    }, avatarSource);
    await page.locator('[data-avatar-id="a"] img').waitFor();
    assert.equal(await page.locator('[data-avatar-id="b"]').textContent(), "WO");
    await page.evaluate(() => {
      window.testAvatars = {};
      window.testListeners["account-avatar"]({ payload: { id: "a", available: false } });
    });
    assert.equal(await page.locator('[data-avatar-id="a"]').textContent(), "PE");
    await page.getByRole("button", { name: "Add channel", exact: true }).click();
    await page.getByLabel("Account name", { exact: true }).fill("Project");
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-add-channel-premium.png" });
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
    await page.evaluate(() => {
      window.testAccountStatus.accounts.b = { page: "tls-failed", pendingNotifications: 101 };
    });
    const quiet = page.getByRole("button", { name: "Pause desktop notifications", exact: true });
    await quiet.click();
    await page.waitForFunction(
      () => document.getElementById("quiet-button").getAttribute("aria-pressed") === "true",
    );
    assert.equal(
      await page.locator('#account-list [data-account-id="b"] .notification-badge').textContent(),
      "99+",
    );
    await quiet.click();
    await page.waitForFunction(
      () => document.getElementById("quiet-button").getAttribute("aria-pressed") === "false",
    );
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-settings-premium.png" });
    await page.getByRole("tab", { name: "Notifications", exact: true }).click();
    assert.equal(
      await page.getByLabel("Notification content", { exact: true }).inputValue(),
      "private",
    );
    await page.getByLabel("Notification content", { exact: true }).selectOption("preview");
    await page.getByRole("button", { name: "Test Linux notification", exact: true }).click();
    await page
      .getByRole("status")
      .filter({ hasText: "Linux accepted the test notification" })
      .waitFor();
    assert.ok(
      await page.evaluate(() =>
        window.testRequests.some((request) => request.cmd === "test_notification"),
      ),
    );
    await page.evaluate(() => {
      window.testNotificationError = true;
    });
    await page.getByRole("button", { name: "Test Linux notification", exact: true }).click();
    await page
      .getByRole("alert")
      .filter({ hasText: "Linux notification service unavailable" })
      .waitFor();
    await page.evaluate(() => {
      window.testNotificationError = false;
    });
    await page.getByLabel("Do not disturb", { exact: true }).check();
    await page.getByLabel("Mute Work", { exact: true }).check();
    await page.getByRole("button", { name: "Save notification settings" }).click();
    await page.getByRole("status").filter({ hasText: "Notification settings saved" }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.testState.notifications), {
      detail: "preview",
      doNotDisturb: true,
      mutedAccounts: ["b"],
    });
    await page.getByRole("button", { name: "Close", exact: true }).click();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("tab", { name: "Notifications", exact: true }).click();
    assert.equal(
      await page.getByLabel("Notification content", { exact: true }).inputValue(),
      "preview",
    );
    await page.getByRole("tab", { name: "Updates", exact: true }).click();
    assert.equal(
      await page.getByLabel("Check for updates every", { exact: true }).inputValue(),
      "5",
    );
    await page.getByLabel("Check for updates every", { exact: true }).selectOption("15");
    await page.getByRole("button", { name: "Save update interval", exact: true }).click();
    await page.getByRole("status").filter({ hasText: "Update interval saved" }).waitFor();
    assert.equal(await page.evaluate(() => window.testUpdateInterval), 15);
    await page.evaluate(() => {
      window.testUpdateStatus = { phase: "idle", canCheck: true };
      window.dispatchEvent(new CustomEvent("update-state", { detail: window.testUpdateStatus }));
    });
    await page.getByRole("button", { name: "Check now", exact: true }).click();
    await page.locator(".update-status").filter({ hasText: "up to date" }).waitFor();
    assert.equal(
      await page.getByRole("button", { name: "Check now", exact: true }).isDisabled(),
      true,
    );
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-updates-020.png" });
    await page.getByRole("tab", { name: "General", exact: true }).click();
    await page.getByRole("button", { name: "Safe diagnostics", exact: true }).click();
    const diagnosticReport = await page.getByLabel("Diagnostic report").inputValue();
    assert.ok(diagnosticReport.includes("load-failed"));
    assert.ok(!diagnosticReport.includes("Personal") && !diagnosticReport.includes("Work"));
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-diagnostics.png" });
    await page.getByRole("button", { name: "Close", exact: true }).click();
    await page.setViewportSize({ width: 900, height: 640 });
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-status-badges.png" });
    await page.getByRole("button", { name: "Overview", exact: false }).click();
    assert.deepEqual(await page.locator("#cards .card-initials").allTextContents(), [
      "PE",
      "WO",
      "<I",
      "PR",
    ]);
    assert.equal(await page.locator("#cards .card-initials img").count(), 0);
    assert.equal(
      await page.locator('#account-list [data-account-id="a"] .notification-badge').textContent(),
      "3",
    );
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
    await page.evaluate(() =>
      window.testListeners["account-state"]({
        payload: {
          accounts: {
            a: { page: "ready", pendingNotifications: 100, unreadCount: 2 },
            "account-4": { page: "ready", pendingNotifications: 7, unreadCount: 0 },
          },
        },
      }),
    );
    assert.equal(
      await page.locator('[data-workspace-id="personal"] .notification-badge').textContent(),
      "2",
    );
    assert.equal(
      await page.locator('[data-workspace-id="workspace-1"] .notification-badge').isVisible(),
      false,
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
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-workspaces-channels.png" });
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
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-search-premium.png" });
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
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-security-premium.png" });
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
        { sidebar: 304, header: 108, overflow: false },
      );
      await page.screenshot({ animations: "disabled", path: `/tmp/elo-premium-${width}.png` });
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
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-empty-premium.png" });
    await page.evaluate(() => {
      const fixture = JSON.parse(localStorage.getItem("fixture"));
      fixture.lastAccount = "b";
      localStorage.setItem("fixture", JSON.stringify(fixture));
      localStorage.setItem("startupScenario", "delayed");
    });
    await page.reload();
    await page.locator("#startup-screen:not([hidden])").waitFor();
    await page.waitForFunction(
      () => window.testRequests.filter((r) => r.cmd === "sync_accounts").length >= 2,
    );
    assert.deepEqual(
      await page.evaluate(() =>
        window.testRequests
          .filter((r) => r.cmd === "sync_accounts")
          .slice(0, 2)
          .map((r) => r.args.accountId),
      ),
      ["b", "a"],
    );
    await page.waitForFunction(() =>
      window.testRequests.some(
        (r) => r.cmd === "sync_accounts" && r.args.accountId !== "a" && r.args.accountId !== "b",
      ),
    );

    assert.equal(
      await page.evaluate(() =>
        window.testRequests.some((r) => r.cmd === "select_account" && r.args.id),
      ),
      false,
    );
    await page.evaluate(() => {
      window.testAccountStatus.accounts.b.readiness = "qr-code";
    });
    await page.locator('#account-list [data-account-id="b"].selected').waitFor();
    assert.equal(await page.locator("#startup-screen").isVisible(), false);
    assert.equal(await page.evaluate(() => window.testAccountStatus.accounts.a.page), "loading");
    await page.reload();
    await page.getByRole("button", { name: "Continue anyway" }).click();
    await page.locator('#account-list [data-account-id="b"].selected').waitFor();
    assert.equal(await page.locator("#startup-screen").isVisible(), false);
    await page.evaluate(() => localStorage.removeItem("startupScenario"));
    await page.addInitScript(() => {
      const original = window.__TAURI__.core.invoke;
      let update = ["background", "prepare-failure"].includes(
        localStorage.getItem("updateScenario"),
      )
        ? { phase: "available", version: "0.1.99" }
        : { phase: "idle" };
      window.updateOperations = [];
      window.__TAURI__.core.invoke = async (command, args) => {
        if (command === "request" && args.op.startsWith("update.")) {
          window.updateOperations.push(args.op);
          if (args.op === "update.startup") return { automatic: true };
          if (args.op === "update.status") return update;
          if (args.op === "update.check") {
            update =
              localStorage.getItem("updateScenario") === "offline"
                ? { phase: "error", error: "Offline" }
                : { phase: "available", version: "0.1.99" };
            return update;
          }
          if (args.op === "update.prepare")
            update =
              localStorage.getItem("updateScenario") === "prepare-failure"
                ? { phase: "error", error: "Synthetic signature verification failure" }
                : { phase: "ready", version: "0.1.99" };
          return update;
        }
        return original(command, args);
      };
    });
    await page.evaluate(() => {
      const fixture = JSON.parse(localStorage.getItem("fixture"));
      fixture.unlocked = false;
      localStorage.setItem("fixture", JSON.stringify(fixture));
    });
    await page.reload();
    await page.waitForFunction(() => window.updateOperations.includes("update.install"));
    assert.deepEqual(
      await page.evaluate(() => window.updateOperations.filter((op) => op !== "update.status")),
      ["update.startup", "update.check", "update.prepare", "update.install"],
    );
    assert.equal(await page.locator("#startup-screen").isVisible(), true);
    await page.evaluate(() => localStorage.setItem("updateScenario", "offline"));
    await page.reload();
    await page.locator("#startup-screen[hidden]").waitFor({ state: "attached" });
    await page.locator("#unlock-submit").click();
    assert.equal(
      await page.evaluate(() => window.updateOperations.includes("update.install")),
      false,
    );
    await page.evaluate(() => {
      localStorage.setItem("updateScenario", "background");
      const fixture = JSON.parse(localStorage.getItem("fixture"));
      fixture.unlocked = true;
      localStorage.setItem("fixture", JSON.stringify(fixture));
    });
    await page.reload();
    await page.getByRole("button", { name: "Continue anyway" }).click();
    await page
      .getByRole("button", { name: "Restart to update", exact: false })
      .click({ timeout: 20000 });
    await page.waitForFunction(() => window.updateOperations.includes("update.install"));
    assert.equal(
      await page.evaluate(() => window.updateOperations.includes("update.restart")),
      false,
    );
    assert.equal(
      await page.evaluate(() => window.updateOperations.includes("update.prepare")),
      true,
    );
    assert.deepEqual(
      await page.evaluate(() =>
        window.updateOperations.filter((op) => ["update.prepare", "update.install"].includes(op)),
      ),
      ["update.prepare", "update.install"],
    );
    await page.evaluate(() => localStorage.setItem("updateScenario", "prepare-failure"));
    await page.reload();
    await page.getByRole("button", { name: "Continue anyway" }).click();
    await page
      .getByRole("button", { name: "Restart to update", exact: false })
      .click({ timeout: 20000 });
    await page.waitForFunction(() => window.updateOperations.includes("update.prepare"));
    await page.locator("#startup-screen[hidden]").waitFor({ state: "attached" });
    assert.equal(
      await page.evaluate(() => window.updateOperations.includes("update.install")),
      false,
    );
    assert.equal(
      await page.evaluate(() => window.updateOperations.includes("update.restart")),
      false,
    );
    await page.getByRole("button", { name: "Find a channel", exact: true }).click();
    await page.getByLabel("Search channels").fill("Settings");
    await page.getByLabel("Search channels").press("Enter");
    await page.getByRole("tab", { name: "General", exact: true }).waitFor();
    await page.getByRole("button", { name: "Close", exact: true }).click();
    await page.emulateMedia({ reducedMotion: "reduce" });
    assert.equal(
      await page
        .locator(".account-card")
        .first()
        .evaluate((element) => getComputedStyle(element).animationName),
      "none",
    );
    const readinessPage = await browser.newPage();
    const readinessScript = fs.readFileSync(
      path.resolve(__dirname, "../src-tauri/src/readiness.js"),
      "utf8",
    );
    await readinessPage.setContent(
      '<body><div id="pane-side" style="display:none">Private text</div></body>',
    );
    assert.equal(await readinessPage.evaluate(readinessScript), 0);
    await readinessPage.setContent(
      '<body><div data-ref="private-qr"><canvas></canvas></div></body>',
    );
    await readinessPage.evaluate(() => {
      HTMLCanvasElement.prototype.toDataURL = () => {
        throw Error("Must not read QR pixels");
      };
    });
    assert.equal(await readinessPage.evaluate(readinessScript), 2);
    await readinessPage.setContent('<body><div id="pane-side" style="height:20px"></div></body>');
    assert.equal(await readinessPage.evaluate(readinessScript), 1);
    await readinessPage.evaluate(() => {
      const splash = document.createElement("div");
      splash.style.cssText = "position:fixed;inset:0;z-index:999;background:white";
      document.body.append(splash);
    });
    assert.equal(await readinessPage.evaluate(readinessScript), 0);
    const avatarScript = fs.readFileSync(
      path.resolve(__dirname, "../src-tauri/src/avatar.js"),
      "utf8",
    );
    await readinessPage.setContent(
      `<button title="Você" style="position:fixed;left:16px;bottom:16px;width:48px;height:48px"><img width="32" height="32" src="${avatarSource}"></button><button title="You" style="position:fixed;left:400px;bottom:16px;width:48px;height:48px"><img src="${avatarSource}"></button>`,
    );
    await readinessPage
      .locator("img")
      .first()
      .evaluate((img) => img.decode());
    assert.equal(await readinessPage.evaluate(avatarScript), "");
    await readinessPage
      .locator("img")
      .first()
      .evaluate((img) => {
        Object.defineProperty(img, "currentSrc", { value: "https://avatar-cdn.example/self.jpg" });
      });
    assert.equal(
      JSON.parse(await readinessPage.evaluate(avatarScript)).source,
      "https://avatar-cdn.example/self.jpg",
    );
    await readinessPage
      .locator('[title="Você"]')
      .evaluate((button) => button.removeAttribute("title"));
    assert.equal(await readinessPage.evaluate(avatarScript), "");
    await readinessPage.close();
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

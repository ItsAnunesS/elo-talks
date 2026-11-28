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
    const attachmentPage = await browser.newPage();
    const syntheticAttachments = fs.mkdtempSync(
      path.join(require("node:os").tmpdir(), "elo-attachments-"),
    );
    try {
      const image = path.join(syntheticAttachments, "sample.png");
      const archive = path.join(syntheticAttachments, "sample.zip");
      fs.copyFileSync(path.resolve(__dirname, "../src-tauri/icons/32x32.png"), image);
      fs.writeFileSync(archive, "PK synthetic attachment");
      const dropFixture = fs
        .readFileSync(path.resolve(__dirname, "../src-tauri/examples/clipboard_smoke.rs"), "utf8")
        .match(/const HTML: &str = r#"([\s\S]*?)"#;/)[1];
      const nativeDropPage = await browser.newPage();
      try {
        await nativeDropPage.route("https://web.whatsapp.com/**", (route) =>
          route.fulfill({ contentType: "text/html", body: dropFixture }),
        );
        await nativeDropPage.goto("https://web.whatsapp.com/");
        const input = await nativeDropPage.context().newCDPSession(nativeDropPage);
        for (const type of ["dragEnter", "dragOver", "drop"]) {
          await input.send("Input.dispatchDragEvent", {
            type,
            x: 100,
            y: 100,
            data: { items: [], files: [image, archive], dragOperationsMask: 1 },
          });
        }
        await nativeDropPage.waitForFunction(() => window.nativeDrops === 1);
        assert.deepEqual(
          await nativeDropPage.evaluate(() =>
            window.drops.map((files) => files.map((file) => file.name)),
          ),
          [["sample.png", "sample.zip"]],
        );
      } finally {
        await nativeDropPage.close();
      }

      const html =
        '<body><div id="editor" contenteditable style="width:500px;height:300px"></div><button id="send">Send</button><script>window.received=[];window.sent=0;send.onclick=()=>sent++;for(const kind of ["paste","drop"])document.addEventListener(kind,event=>{event.preventDefault();received.push({kind,names:[...(event.clipboardData||event.dataTransfer).files].map(file=>file.name)});});</script>';
      await attachmentPage.route("https://web.whatsapp.com/**", (route) =>
        route.fulfill({ contentType: "text/html", body: html }),
      );
      await attachmentPage.route("https://untrusted.example/**", (route) =>
        route.fulfill({ contentType: "text/html", body: html }),
      );
      await attachmentPage.goto("https://web.whatsapp.com/");
      const cdp = await attachmentPage.context().newCDPSession(attachmentPage);
      const capture = fs.readFileSync(
        path.resolve(__dirname, "../src-tauri/src/attachment_capture.js"),
        "utf8",
      );
      const dispatch = fs.readFileSync(
        path.resolve(__dirname, "../src-tauri/src/attachment_dispatch.js"),
        "utf8",
      );
      const world = async () => {
        const tree = await cdp.send("Page.getFrameTree");
        return (
          await cdp.send("Page.createIsolatedWorld", {
            frameId: tree.frameTree.frame.id,
            worldName: "elo-account-native",
          })
        ).executionContextId;
      };
      let contextId = await world();
      const prepare = async () => {
        await attachmentPage.locator("#editor").click();
        const result = await cdp.send("Runtime.evaluate", { contextId, expression: capture });
        assert.ok(result.result.objectId);
        await cdp.send("DOM.setFileInputFiles", {
          objectId: result.result.objectId,
          files: [image, archive],
        });
        return result.result.objectId;
      };
      const deliver = (objectId, kind) =>
        cdp.send("Runtime.callFunctionOn", {
          objectId,
          functionDeclaration: dispatch,
          awaitPromise: true,
          arguments: [{ value: kind }, { value: [0.2, 0.2] }],
          returnByValue: true,
        });
      assert.equal((await deliver(await prepare(), "paste")).result.value, true);
      assert.equal((await deliver(await prepare(), "drop")).result.value, true);
      assert.deepEqual(await attachmentPage.evaluate(() => received), [
        { kind: "paste", names: ["sample.png", "sample.zip"] },
        { kind: "drop", names: ["sample.png", "sample.zip"] },
      ]);
      const staleFocus = await prepare();
      await attachmentPage.locator("#send").focus();
      assert.equal((await deliver(staleFocus, "paste")).result.value, false);
      assert.equal(await attachmentPage.evaluate(() => sent), 0);
      assert.equal(await attachmentPage.locator('input[type="file"]').count(), 0);
      const staleDocument = await prepare();
      await attachmentPage.goto("https://untrusted.example/");
      await assert.rejects(() => deliver(staleDocument, "paste"));
      contextId = await world();
      await attachmentPage.locator("#editor").click();
      assert.equal(
        (await cdp.send("Runtime.evaluate", { contextId, expression: capture })).result.subtype,
        "null",
      );
      assert.deepEqual(await attachmentPage.evaluate(() => received), []);
    } finally {
      await attachmentPage.close();
      fs.rmSync(syntheticAttachments, { recursive: true, force: true });
    }
    const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
    await page.clock.install();
    const pointerDrag = async (source, target, options = {}) => {
      const from = await source.boundingBox();
      const to = await target.boundingBox();
      assert.ok(from && to);
      const position = options.targetPosition || { x: to.width / 2, y: to.height / 2 };
      await page.mouse.move(from.x + 30, from.y + from.height / 2);
      await page.mouse.down();
      await page.mouse.move(to.x + position.x, to.y + position.y, { steps: 12 });
      await options.beforeDrop?.();
      await page.mouse.up();
    };

    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.addInitScript(() => {
      window.testRequests = [];
      const nativeInterval = window.setInterval;
      window.setInterval = (callback, delay, ...args) => {
        if (delay === 10000) window.testUpdatePoll = callback;
        return nativeInterval(callback, delay, ...args);
      };
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
            if (cmd === "media_permissions") return window.testMediaPermissions || {};
            if (cmd === "reset_media_permission") {
              if (window.testMediaError) throw Error("Could not save device permissions");
              window.testMediaPermissions[args.id][args.device] = args.block ? "block" : "ask";
              return;
            }
            if (cmd === "choose_backup") return window.testBackupPath ?? "/tmp/synthetic.elobackup";
            if (cmd === "test_notification" && window.testNotificationError)
              throw Error("Linux notification service unavailable");
            if (
              cmd === "sync_accounts" &&
              localStorage.getItem("startupScenario") &&
              (args.accountId === "a" ||
                localStorage.getItem("startupScenario") === "priority-failed")
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
              if (args.op === "backup.export" || args.op === "backup.import") {
                window.testState.backup = {
                  phase: "running",
                  operation: args.op.endsWith("export") ? "export" : "import",
                };
                return structuredClone(window.testState.backup);
              }
              if (args.op === "backup.status") {
                if (window.testBackupComplete) {
                  window.testState.created = true;
                  window.testState.pin = false;
                  window.testState.keys = [];
                  window.testState.backup.phase = "complete";
                }
                return structuredClone(window.testState.backup);
              }
              if (args.op === "server.connect") {
                window.testState.server = {
                  phase: "syncing",
                  domain: args.data.domain,
                  username: args.data.username,
                  organizationId: null,
                  organizations: [],
                };
                return structuredClone(window.testState.server);
              }
              if (args.op === "server.status") {
                if (window.testServerComplete)
                  window.testState.server = {
                    ...window.testState.server,
                    phase: "connected",
                    organizations: [
                      {
                        id: "11111111-1111-4111-8111-111111111111",
                        name: "Company",
                        role: "member",
                      },
                    ],
                  };
                return structuredClone(window.testState.server || { phase: "local" });
              }
              if (args.op === "server.select") {
                window.testState.server = {
                  ...window.testState.server,
                  phase: "syncing",
                  organizationId: args.data.organizationId,
                };
                return structuredClone(window.testState.server);
              }
              if (args.op === "status") return structuredClone(window.testState);
              if (args.op === "notifications") {
                window.testState.notifications = args.data;
                window.testAccountStatus.preferences = args.data;
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
              if (args.op === "workspace.reorder") {
                const ordered = data.ids.map((id) =>
                  organization.workspaces.find((workspace) => workspace.id === id),
                );
                organization.workspaces = organization.workspaces.map((workspace) =>
                  data.ids.includes(workspace.id) ? ordered.shift() : workspace,
                );
                return structuredClone(window.testState);
              }
              if (args.op === "account.move") {
                if (window.testMoveError) {
                  window.testMoveError = false;
                  throw Error("Synthetic move failure");
                }
                if (data.beforeId !== undefined) {
                  const accounts = window.testState.accounts;
                  const index = accounts.findIndex((account) => account.id === data.id);
                  const [account] = accounts.splice(index, 1);
                  const target = data.beforeId
                    ? accounts.findIndex((item) => item.id === data.beforeId)
                    : accounts.length;
                  accounts.splice(target, 0, account);
                }
                organization.placements[data.id] = {
                  ...organization.placements[data.id],
                  workspaceId: data.workspaceId,
                  categoryId: data.categoryId,
                };
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
    await page.evaluate(() => {
      localStorage.setItem(
        "fixture",
        JSON.stringify({ ...window.testState, error: "Synthetic busy mount" }),
      );
    });
    await page.reload();
    await page.locator("#lock-recovery").waitFor();
    assert.equal(await page.locator("#unlock-form").isVisible(), false);
    assert.equal(await page.locator("#methods").isVisible(), false);
    assert.equal(await page.locator("#lock-title").textContent(), "Securing your space.");
    await page.getByRole("button", { name: "Finish locking", exact: true }).click();
    assert.equal(await page.locator("#retry-lock").isDisabled(), true);
    assert.equal(await page.locator("#recovery-status").textContent(), "Closing the vault safely…");
    assert.equal(
      await page.locator("#unlock-form").isVisible(),
      false,
      "RPC acknowledgment is not proof of closure",
    );
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-lock-recovery.png" });
    await page.evaluate(() => localStorage.removeItem("fixture"));
    await page.reload();
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

    // Backup workflow remains available while locked and never unlocks accounts.
    await page.getByRole("button", { name: "Encrypted backup", exact: true }).click();
    await page
      .getByLabel("Current master password", { exact: true })
      .fill("original master password");
    await page.getByLabel("Backup password", { exact: true }).fill("different backup password");
    await page.getByLabel("Confirm backup password", { exact: true }).fill("does not match");
    await page.getByRole("button", { name: "Choose destination", exact: true }).click();
    await page.locator("#dialog-error").filter({ hasText: "do not match" }).waitFor();
    assert.equal(
      await page.evaluate(() => window.testRequests.some((r) => r.args?.op === "backup.export")),
      false,
    );
    await page
      .getByLabel("Confirm backup password", { exact: true })
      .fill("different backup password");
    await page.screenshot({ path: "/tmp/elo-backup-export.png", animations: "disabled" });
    await page.setViewportSize({ width: 900, height: 640 });
    await page.screenshot({ path: "/tmp/elo-backup-export-min.png", animations: "disabled" });
    assert.equal(await page.locator("#dialog-submit").isVisible(), true);
    await page.getByRole("button", { name: "Choose destination", exact: true }).click();
    await page.locator("#dialog").waitFor({ state: "hidden" });
    assert.equal(await page.locator("#unlock-submit").isDisabled(), true);
    assert.equal(await page.locator("#backup-button").isDisabled(), true);
    assert.equal(await page.locator('#dialog input[type="password"]').count(), 0);
    assert.equal(await page.evaluate(() => window.testState.unlocked), false);
    await page.evaluate(() => {
      window.testBackupComplete = true;
    });
    await page.locator("#backup-status").filter({ hasText: "saved" }).waitFor();
    assert.equal(await page.locator("#unlock-submit").isDisabled(), false);
    // New installations offer import, and explain the destination credentials.
    await page.evaluate(() => {
      window.testState.created = false;
      window.testState.backup = { phase: "idle" };
      localStorage.setItem("fixture", JSON.stringify(window.testState));
    });
    await page.reload();
    await page.getByRole("button", { name: "Encrypted backup", exact: true }).click();
    await page.getByLabel("Backup password", { exact: true }).fill("different backup password");
    assert.equal(await page.getByLabel("Current master password", { exact: true }).count(), 0);
    await page.screenshot({ path: "/tmp/elo-backup-import-min.png", animations: "disabled" });
    await page.getByRole("button", { name: "Choose backup", exact: true }).click();
    await page.locator("#dialog").waitFor({ state: "hidden" });
    await page.evaluate(() => {
      window.testBackupComplete = true;
    });
    await page.locator("#backup-status").filter({ hasText: "Backup imported" }).waitFor();
    assert.equal(await page.locator("#unlock-submit").textContent(), "Unlock my space →");
    await page.evaluate(() => {
      window.testState.pin = true;
      window.testState.keys = [{ id: "key-one", name: "My key" }];
      window.testState.backup = { phase: "idle" };
      localStorage.setItem("fixture", JSON.stringify(window.testState));
    });
    await page.reload();
    await page.setViewportSize({ width: 1200, height: 800 });

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
    const dragSource = page.locator('.account-row[data-account-id="c"]');
    const dragTarget = page.locator('.account-row[data-account-id="a"]');
    const sourceBox = await dragSource.boundingBox();
    const targetBox = await dragTarget.boundingBox();
    await page.mouse.move(sourceBox.x + 30, sourceBox.y + 10);
    await page.mouse.down();
    await page.mouse.move(targetBox.x + 30, targetBox.y + 2, { steps: 12 });
    assert.equal(await page.locator(".drop-before").count(), 1);
    await page.keyboard.press("Escape");
    await page.mouse.up();
    assert.equal(await page.locator(".is-dragging, .drop-before, .drop-after").count(), 0);
    assert.equal(
      await page.evaluate(() =>
        window.testRequests.some((request) => request.args?.op === "account.move"),
      ),
      false,
    );
    await page.locator('.account-row[data-account-id="b"] .account-name').click();
    await page.locator('.account-row[data-account-id="b"].selected').waitFor();
    await page.locator('.account-row[data-account-id="a"] .account-name').click();
    await page.locator('.account-row[data-account-id="a"].selected').waitFor();
    const selectionCount = await page.evaluate(
      () => window.testRequests.filter((request) => request.cmd === "select_account").length,
    );
    await page
      .locator('.account-row[data-account-id="a"]')
      .dispatchEvent("drop", { dataTransfer: await page.evaluateHandle(() => new DataTransfer()) });
    assert.equal(
      await page.evaluate(() =>
        window.testRequests.some((request) => request.args?.op === "account.move"),
      ),
      false,
    );
    await page.evaluate(() => {
      window.testMoveError = true;
    });
    await pointerDrag(
      page.locator('.account-row[data-account-id="c"]'),
      page.locator('.account-row[data-account-id="a"]'),
      {
        targetPosition: { x: 20, y: 2 },
      },
    );
    await page.getByRole("status").filter({ hasText: "Synthetic move failure" }).waitFor();
    assert.deepEqual(
      await page.evaluate(() => window.testState.accounts.map((account) => account.id)),
      ["a", "b", "c"],
    );
    const channelOrder = () =>
      page
        .locator("#account-list .account-row")
        .evaluateAll((rows) => rows.map((row) => row.dataset.accountId));
    await pointerDrag(
      page.locator('.account-row[data-account-id="c"]'),
      page.locator('.account-row[data-account-id="a"]'),
      {
        targetPosition: { x: 20, y: 2 },
      },
    );
    await page.waitForFunction(
      () => document.querySelector("#account-list .account-row").dataset.accountId === "c",
    );
    assert.deepEqual(await channelOrder(), ["c", "a", "b"]);
    assert.equal(
      await page.evaluate(
        () => window.testRequests.filter((request) => request.cmd === "select_account").length,
      ),
      selectionCount,
    );
    await page.locator('.account-row[data-account-id="c"] .account-name').press("Alt+ArrowDown");
    await page.waitForFunction(
      () => document.querySelector("#account-list .account-row").dataset.accountId === "a",
    );
    await page.locator('.account-row[data-account-id="c"] .account-name').press("Alt+ArrowDown");
    await page.waitForFunction(
      () =>
        document.querySelector("#account-list .account-row:last-child").dataset.accountId === "c",
    );
    assert.deepEqual(await channelOrder(), ["a", "b", "c"]);
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-workspace.png" });
    assert.equal(await page.locator("#channel-service").textContent(), "WhatsApp");
    assert.equal(await page.locator("#channel-avatar").isVisible(), true);
    await page.getByRole("button", { name: "Channel settings", exact: true }).click();
    await page.getByLabel("Account name", { exact: true }).waitFor();
    await page.getByRole("button", { name: "Close", exact: true }).click();

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
    await page.getByRole("button", { name: "Try again", exact: true }).click();
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
    assert.equal(await page.getByLabel("Server domain", { exact: true }).count(), 0);
    assert.equal(await page.getByLabel("Server username", { exact: true }).count(), 0);
    assert.equal(await page.getByLabel("Server password", { exact: true }).count(), 0);
    assert.equal(
      await page.getByRole("button", { name: "Connect and sign in", exact: true }).count(),
      0,
    );
    await page.getByRole("heading", { name: "Encrypted backup", exact: true }).waitFor();
    await page.setViewportSize({ width: 900, height: 640 });
    await page
      .getByRole("button", { name: "Lock to export backup", exact: true })
      .scrollIntoViewIfNeeded();
    await page.screenshot({ animations: "disabled", path: "/tmp/elo-backup-settings-min.png" });
    assert.equal(
      await page.evaluate(() => window.testRequests.some((r) => r.args?.op?.startsWith("server."))),
      false,
    );
    await page.setViewportSize({ width: 1200, height: 800 });
    await page.evaluate(() => {
      window.testState.managedWorkspaces = ["personal"];
    });
    await page.getByRole("button", { name: "Save lock timeout", exact: true }).click();
    await page.getByRole("button", { name: "Close", exact: true }).click();
    assert.equal(
      await page
        .locator('#workspace-icons [data-workspace-id="personal"]')
        .getAttribute("draggable"),
      "false",
    );
    assert.equal(
      await page.locator('#account-list .account-row[data-reorderable="true"]').count(),
      0,
    );
    await page.getByRole("button", { name: "Manage workspace", exact: true }).click();
    await page.getByRole("heading", { name: "Managed by your server", exact: true }).waitFor();
    assert.equal(await page.getByLabel("Workspace name", { exact: true }).count(), 0);
    await page.locator("#dialog-cancel").click();
    await page.evaluate(() => {
      window.testState.managedWorkspaces = [];
    });
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("button", { name: "Save lock timeout", exact: true }).click();

    await page.getByRole("tab", { name: "Notifications", exact: true }).click();
    assert.equal(
      await page.getByLabel("Notification content", { exact: true }).inputValue(),
      "private",
    );
    const senderPhoto = page.getByLabel("Show sender photo in message previews", { exact: true });
    assert.equal(await senderPhoto.isChecked(), false);
    assert.equal(await senderPhoto.isDisabled(), true);
    await page.getByLabel("Notification content", { exact: true }).selectOption("channel");
    assert.equal(await senderPhoto.isDisabled(), true);
    await page.getByLabel("Notification content", { exact: true }).selectOption("preview");
    assert.equal(await senderPhoto.isDisabled(), false);
    await senderPhoto.check();
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
      senderPhoto: true,
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
    assert.equal(await senderPhoto.isChecked(), true);
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
    const workspaceOrder = () =>
      page
        .locator("#workspace-icons .workspace-icon")
        .evaluateAll((items) => items.map((item) => item.dataset.workspaceId));
    await pointerDrag(
      page.locator('[data-workspace-id="workspace-1"]'),
      page.locator('[data-workspace-id="personal"]'),
      { targetPosition: { x: 20, y: 2 } },
    );
    await page.waitForFunction(
      () =>
        document.querySelector("#workspace-icons .workspace-icon").dataset.workspaceId ===
        "workspace-1",
    );
    assert.deepEqual(await workspaceOrder(), ["workspace-1", "personal"]);
    await page.locator('[data-workspace-id="workspace-1"]').press("Alt+ArrowDown");
    await page.waitForFunction(
      () =>
        document.querySelector("#workspace-icons .workspace-icon").dataset.workspaceId ===
        "personal",
    );
    assert.deepEqual(await workspaceOrder(), ["personal", "workspace-1"]);
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
    await page.getByRole("button", { name: "Project", exact: true }).focus();
    const avatarRequests = await page.evaluate(
      () => window.testRequests.filter((request) => request.cmd === "account_avatar").length,
    );
    await page.keyboard.press("Shift+F10");
    await page.keyboard.press("ArrowDown");
    assert.equal(
      await page
        .getByRole("menuitem", { name: "Add to favorites", exact: true })
        .evaluate((button) => button === document.activeElement),
      true,
    );
    await page.keyboard.press("Enter");
    await page.waitForFunction(
      () => document.activeElement?.getAttribute("aria-label") === "Project",
    );
    assert.equal(
      await page.evaluate(
        () => window.testRequests.filter((request) => request.cmd === "account_avatar").length,
      ),
      avatarRequests,
    );

    await page.getByLabel("Show channels", { exact: true }).selectOption("favorites");
    assert.equal(await page.locator("#account-list [data-account-id]:visible").count(), 1);
    assert.equal(await page.locator("#cards [data-account-id]:visible").count(), 1);
    await page.getByLabel("Show channels", { exact: true }).selectOption("all");
    await page.getByRole("button", { name: "Project", exact: true }).click();
    await page.reload();
    await page.getByRole("button", { name: "Continue anyway" }).click();
    await page.locator('#account-list [data-account-id="account-3"].selected').waitFor();
    assert.equal(
      await page
        .locator('#workspace-icons [data-workspace-id="workspace-1"]')
        .getAttribute("aria-pressed"),
      "true",
    );
    await page.locator('#account-list [data-account-id="account-3"]').click({ button: "right" });
    await page.getByRole("menuitem", { name: "Remove from favorites", exact: true }).waitFor();
    await page.keyboard.press("Escape");
    assert.equal(await page.getByRole("menu").count(), 0);
    assert.equal(
      await page.getByRole("button", { name: "Project", exact: true }).getAttribute("aria-current"),
      "page",
    );
    const repeatedTextMutations = await page.evaluate(async () => {
      const { updateAccountStatus } = await import("./account-status.js");
      updateAccountStatus(window.testAccountStatus, "account-3");
      const observer = new MutationObserver(() => {});
      observer.observe(document.body, { subtree: true, childList: true, characterData: true });
      for (let index = 0; index < 25; index++)
        updateAccountStatus(window.testAccountStatus, "account-3");
      const count = observer.takeRecords().length;
      observer.disconnect();
      return count;
    });
    assert.equal(
      repeatedTextMutations,
      0,
      "Unchanged counters must not rewrite text or live regions",
    );
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
    await page.keyboard.press("ArrowUp");
    assert.equal(
      await page
        .getByLabel("Search channels")
        .evaluate((input) => input === document.activeElement),
      true,
    );
    await page.getByLabel("Search channels").press("ArrowDown");
    await page.keyboard.press("Enter");
    await page.locator('#account-list [data-account-id="account-4"].selected').waitFor();
    await page.evaluate(() => {
      window.testMediaPermissions = { "account-4": { microphone: "allow", camera: "ask" } };
    });
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("tab", { name: "Security", exact: true }).click();
    await page.getByRole("button", { name: "Revoke microphone", exact: true }).click();
    await page.getByText("Microphone: Blocked", { exact: true }).waitFor();
    assert.equal(
      await page.getByRole("button", { name: "Revoke microphone", exact: true }).isDisabled(),
      true,
    );
    await page.getByRole("button", { name: "Ask for microphone again", exact: true }).click();
    await page.getByText("Microphone: Ask next time", { exact: true }).waitFor();
    await page.evaluate(() => (window.testMediaError = true));
    await page.getByRole("button", { name: "Revoke camera", exact: true }).click();
    await page.getByText("Could not save device permissions", { exact: true }).waitFor();
    await page.getByText("Camera: Ask next time", { exact: true }).waitFor();
    await page.evaluate(() => (window.testMediaError = false));
    await page.getByRole("tab", { name: "General", exact: true }).click();
    await page.getByLabel("Text size", { exact: true }).selectOption("120");
    assert.equal(await page.locator("html").getAttribute("data-text-size"), "120");
    await page.getByLabel("Text size", { exact: true }).selectOption("100");
    await page.getByLabel("Layout density", { exact: true }).selectOption("compact");
    assert.equal(await page.locator("html").getAttribute("data-density"), "compact");
    await page.getByLabel("Interface animations", { exact: true }).uncheck();
    assert.equal(await page.locator("html").getAttribute("data-motion"), "off");
    await page.getByLabel("Layout density", { exact: true }).selectOption("comfortable");
    await page.getByLabel("Interface animations", { exact: true }).check();
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
      localStorage.removeItem("elo-space");
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
      // The QR screen can be usable before pending resources finish loading.
      window.testAccountStatus.accounts.b.page = "loading";
      window.testAccountStatus.accounts.b.readiness = "qr-code";
    });
    await page.locator('#account-list [data-account-id="b"].selected').waitFor();
    assert.equal(await page.locator("#startup-screen").isVisible(), false);
    assert.equal(await page.evaluate(() => window.testAccountStatus.accounts.a.page), "loading");
    await page.reload();
    await page.getByRole("button", { name: "Continue anyway" }).click();
    await page.locator('#account-list [data-account-id="b"].selected').waitFor();
    assert.equal(await page.locator("#startup-screen").isVisible(), false);
    await page.evaluate(() => localStorage.removeItem("elo-space"));
    await page.reload();
    await page.locator("#startup-screen:not([hidden])").waitFor();
    await page.waitForFunction(() => window.testRequests.some((r) => r.cmd === "account_snapshot"));
    await page.clock.fastForward(41000);
    await page.locator('#account-list [data-account-id="b"].selected').waitFor();
    assert.equal(await page.locator("#startup-screen").isVisible(), false);
    assert.equal(
      await page.evaluate(() => window.testAccountStatus.accounts.b.readiness),
      "preparing",
    );
    await page.evaluate(() => localStorage.removeItem("elo-space"));
    await page.reload();
    await page.locator("#startup-screen:not([hidden])").waitFor();
    await page.evaluate(() => {
      window.testAccountStatus.accounts.b.page = "tls-failed";
    });
    await page.clock.fastForward(41000);
    assert.equal(await page.locator("#startup-screen").isVisible(), true);
    await page
      .getByText("WhatsApp could not load. Continue to retry the account.", { exact: true })
      .waitFor();
    await page.getByRole("button", { name: "Continue anyway" }).click();
    await page.locator('#account-list [data-account-id="b"].selected').waitFor();
    await page.evaluate(() => localStorage.setItem("startupScenario", "priority-failed"));
    await page.reload();
    await page.locator("#startup-screen[hidden]").waitFor({ state: "attached" });
    assert.equal(await page.locator("#dashboard").isVisible(), true);
    await page.waitForFunction(
      () => window.testRequests.filter((request) => request.cmd === "sync_accounts").length >= 2,
    );
    assert.equal(await page.locator("#lock-screen").isVisible(), false);
    await page.evaluate(() => localStorage.removeItem("startupScenario"));
    await page.evaluate(async () => {
      window.testHoldOperation = "update.status";
      const before = window.testRequests.filter(
        (request) => request.args?.op === "update.status",
      ).length;
      const first = window.testUpdatePoll();
      await Promise.resolve();
      await window.testUpdatePoll();
      window.testPollDelta =
        window.testRequests.filter((request) => request.args?.op === "update.status").length -
        before;
      window.testHoldOperation = null;
      window.testReleaseRequest();
      await first;
    });
    assert.equal(
      await page.evaluate(() => window.testPollDelta),
      1,
      "Slow status queries must not overlap",
    );
    await page.addInitScript(() => {
      const original = window.__TAURI__.core.invoke;
      let update =
        localStorage.getItem("updateScenario") === "background"
          ? { phase: "ready", version: "0.1.99" }
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
      false,
    );
    await page.evaluate(() => {
      localStorage.setItem("updateScenario", "prepare-failure");
      const fixture = JSON.parse(localStorage.getItem("fixture"));
      fixture.unlocked = false;
      localStorage.setItem("fixture", JSON.stringify(fixture));
    });
    await page.reload();
    await page.waitForFunction(() => window.updateOperations.includes("update.prepare"));
    await page.locator("#startup-screen[hidden]").waitFor({ state: "attached" });
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
    await page.getByRole("button", { name: "Company", exact: true }).click();
    await page.getByRole("button", { name: "Manage Project", exact: true }).click();
    await page.getByRole("button", { name: "Mute notifications", exact: true }).click();
    await page
      .locator('#account-list [data-account-id="account-3"] .channel-muted:not([hidden])')
      .waitFor();
    await page.getByRole("button", { name: "Manage Project", exact: true }).click();
    await page.getByRole("button", { name: "Unmute notifications", exact: true }).click();
    await page
      .locator('#account-list [data-account-id="account-3"] .channel-muted[hidden]')
      .waitFor({ state: "attached" });
    const beforeMove = await page.evaluate(() =>
      structuredClone(window.testState.organization.placements["account-3"]),
    );
    await pointerDrag(
      page.locator('#account-list [data-account-id="account-3"]'),
      page.locator('#workspace-icons [data-workspace-id="personal"]'),
    );
    await page.locator("#move-feedback:not([hidden])").waitFor();
    assert.equal(
      await page.evaluate(() => window.testState.organization.placements["account-3"].workspaceId),
      "personal",
    );
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await page.locator("#move-feedback[hidden]").waitFor({ state: "attached" });
    assert.deepEqual(
      await page.evaluate(() => window.testState.organization.placements["account-3"]),
      beforeMove,
    );
    await page.evaluate(async () => {
      const loading = await import("./loading-screen.js");
      loading.showUpdate({
        phase: "downloading",
        currentVersion: "0.3.0",
        version: "0.3.1",
        progress: { step: "downloading", downloaded: 25 * 1048576, total: 100 * 1048576 },
      });
    });
    assert.equal(await page.locator("#update-meter").getAttribute("value"), "25");
    assert.equal(await page.locator("#update-version").textContent(), "0.3.0 → 0.3.1");
    assert.equal(await page.locator("#update-detail").textContent(), "25.0 / 100.0 MB");
    await page.evaluate(async () => (await import("./loading-screen.js")).hideLoading());
    await page.evaluate(() => {
      window.testAccountStatus.accounts["account-3"] = {
        page: "ready",
        readiness: "chats",
        unreadCount: 2,
      };
      window.testAccountStatus.accounts["account-4"] = {
        page: "ready",
        readiness: "chats",
        unreadCount: 0,
      };
      window.testListeners["account-state"]({ payload: window.testAccountStatus });
    });
    await page.getByLabel("Show channels", { exact: true }).selectOption("unread");
    assert.equal(await page.locator("#account-list [data-account-id]:visible").count(), 1);
    await page.locator("#overview-button").click();
    await page.reload();
    await page.locator("#startup-screen[hidden]").waitFor({ state: "attached" });
    assert.equal(await page.getByLabel("Show channels", { exact: true }).inputValue(), "unread");
    assert.equal(await page.locator("#dashboard").isVisible(), true);
    assert.equal(
      await page
        .locator('#workspace-icons [data-workspace-id="workspace-1"]')
        .getAttribute("aria-pressed"),
      "true",
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
    for (const code of ["canvas", "svg", "img"]) {
      await readinessPage.setContent(
        `<body><div data-testid="qrcode" style="position:relative;width:160px;height:160px"><${code} width="160" height="160"></${code}><div style="position:absolute;left:60px;top:60px;width:40px;height:40px">Synthetic logo</div></div></body>`,
      );
      assert.equal(await readinessPage.evaluate(readinessScript), 2);
      await readinessPage.evaluate(() => {
        const splash = document.createElement("div");
        splash.style.cssText = "position:fixed;inset:0;z-index:999;background:white";
        document.body.append(splash);
      });
      assert.equal(await readinessPage.evaluate(readinessScript), 0);
    }
    await readinessPage.setContent('<body><canvas width="160" height="160"></canvas></body>');
    assert.equal(await readinessPage.evaluate(readinessScript), 0);
    await readinessPage.setContent('<body><div id="pane-side" style="height:20px"></div></body>');
    assert.equal(await readinessPage.evaluate(readinessScript), 1);
    await readinessPage.evaluate(() => {
      const splash = document.createElement("div");
      splash.style.cssText = "position:fixed;inset:0;z-index:999;background:white";
      document.body.append(splash);
    });
    assert.equal(await readinessPage.evaluate(readinessScript), 0);
    const readinessWatch = fs
      .readFileSync(path.resolve(__dirname, "../src-tauri/src/readiness_watch.js"), "utf8")
      .replace("__READINESS_QUERY__", readinessScript);
    await readinessPage.route("https://web.whatsapp.com/**", (route) =>
      route.fulfill({
        contentType: "text/html",
        body: '<body><div data-testid="qrcode"><canvas width="160" height="160"></canvas></div></body>',
      }),
    );
    await readinessPage.goto("https://web.whatsapp.com/");
    const photoScript = fs.readFileSync(
      path.resolve(__dirname, "../src-tauri/src/notification_photo.js"),
      "utf8",
    );
    const photoSource = await readinessPage.evaluate(() => {
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = 32;
      canvas.getContext("2d").fillRect(0, 0, 32, 32);
      return canvas.toDataURL();
    });
    const photoResult = await readinessPage.evaluate(
      async ({ script, source }) => {
        const resolve = (0, eval)(script);
        const result = await resolve(source);
        const bytes = Uint8Array.from(atob(result.split(",")[1]), (char) => char.charCodeAt(0));
        const view = new DataView(bytes.buffer);
        const invalid = [];
        for (const url of [
          "file:///etc/passwd",
          "https://untrusted.example/photo",
          "http://web.whatsapp.com/photo",
          "https://web.whatsapp.com.evil.example/photo",
        ])
          invalid.push(await resolve(url));
        let fetched = false;
        window.fetch = () => {
          fetched = true;
          throw Error("Must reject before fetch");
        };
        const rejected = await resolve("https://untrusted.example/photo");
        return {
          width: view.getUint32(16),
          height: view.getUint32(20),
          invalid,
          fetched,
          rejected,
        };
      },
      { script: photoScript, source: photoSource },
    );
    assert.deepEqual(photoResult, {
      width: 64,
      height: 64,
      invalid: ["", "", "", ""],
      fetched: false,
      rejected: "",
    });

    await readinessPage.evaluate(() => {
      window.readinessUpdates = [];
      window.sameDocumentMarker = "business-session";
      window.__eloReadiness = (value) => window.readinessUpdates.push(value);
      HTMLCanvasElement.prototype.toDataURL = () => {
        throw Error("Must not inspect QR pixels");
      };
    });
    assert.equal(await readinessPage.evaluate(readinessWatch), true);
    assert.deepEqual(await readinessPage.evaluate(() => window.readinessUpdates), ["2"]);
    await readinessPage.evaluate(() => {
      const navigation = document.createElement("nav");
      navigation.setAttribute("aria-label", "Business tools");
      const chats = document.createElement("section");
      chats.id = "pane-side";
      chats.style.cssText = "width:400px;height:400px";
      document.body.replaceChildren(navigation, chats);
    });
    await readinessPage.waitForFunction(() => window.readinessUpdates.at(-1) === "1");
    assert.equal(await readinessPage.evaluate(() => window.sameDocumentMarker), "business-session");
    const reported = await readinessPage.evaluate(() => window.readinessUpdates.length);
    await readinessPage.evaluate(() => {
      const conversation = document.createElement("div");
      document.getElementById("pane-side").append(conversation);
      for (let index = 0; index < 20; index++) conversation.append(document.createElement("div"));
    });
    await readinessPage.waitForTimeout(300);
    assert.equal(await readinessPage.evaluate(() => window.readinessUpdates.length), reported);
    await readinessPage.evaluate(() => {
      const login = document.createElement("div");
      login.dataset.testid = "qrcode";
      login.append(document.createElement("canvas"));
      document.body.replaceChildren(login);
    });
    await readinessPage.waitForFunction(() => window.readinessUpdates.at(-1) === "2");
    assert.deepEqual(await readinessPage.evaluate(() => window.readinessUpdates), ["2", "1", "2"]);
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
      "PASS: attachment paste/drop with focus/origin isolation and no automatic send, encrypted backup workflows, credentials, safe labels, account switching, workspace/channel ordering, move recovery, raster uploads, category moves, notification badges and private diagnostics.",
    );
  } finally {
    await browser.close();
    server.close();
  }
})().catch((e) => {
  console.error(e);
  process.exit(1);
});

(() => {
  "use strict";
  if (globalThis.location.origin !== __ELO_ACCOUNT_ORIGIN__) return;
  const deliver = globalThis.__eloNotification;
  if (typeof deliver !== "function" || globalThis.Notification?.__elo) return;
  const send = (title, options = {}) => {
    deliver(JSON.stringify({
      title: [...String(title)].slice(0, 400).join(""),
      body: [...String(options.body ?? "")].slice(0, 1200).join(""),
    }));
  };
  class AccountNotification extends EventTarget {
    static get permission() { return "granted"; }
    static get maxActions() { return 0; }
    static get __elo() { return true; }
    static requestPermission(callback) {
      if (callback) queueMicrotask(() => callback("granted"));
      return Promise.resolve("granted");
    }
    constructor(title, options = {}) {
      super();
      this.title = String(title);
      this.body = String(options.body ?? "");
      this.tag = String(options.tag ?? "");
      this.data = options.data ?? null;
      send(title, options);
      queueMicrotask(() => {
        const event = new Event("show");
        this.dispatchEvent(event);
        this.onshow?.(event);
      });
    }
    close() {
      const event = new Event("close");
      this.dispatchEvent(event);
      this.onclose?.(event);
    }
  }
  Object.defineProperty(globalThis, "Notification", { value: AccountNotification });
  if (globalThis.navigator?.permissions) {
    const query = navigator.permissions.query.bind(navigator.permissions);
    Object.defineProperty(navigator.permissions, "query", {
      value(descriptor) {
        if (descriptor?.name !== "notifications") return query(descriptor);
        const status = new EventTarget();
        Object.defineProperty(status, "state", {value: "granted"});
        status.onchange = null;
        return Promise.resolve(status);
      },
    });
  }
  if (globalThis.ServiceWorkerRegistration) {
    Object.defineProperty(ServiceWorkerRegistration.prototype, "showNotification", {
      value(title, options) { send(title, options); return Promise.resolve(); },
    });
  }
})();

const nativeApi = window.__TAURI__;
export const isDesktop = Boolean(nativeApi);

export function invoke(command, arguments_) {
  if (!nativeApi) {
    return Promise.reject(new Error("Open the Elo Talks AppImage to use the vault."));
  }
  return nativeApi.core.invoke(command, arguments_);
}

export function request(operation, data = {}) {
  return invoke("request", { op: operation, data });
}

export function listen(event, handler) {
  return nativeApi.event.listen(event, handler);
}

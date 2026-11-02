(() => {
  const pending = globalThis.eloPaste;
  globalThis.eloPaste = null;
  if (
    location.origin !== "https://web.whatsapp.com" ||
    !document.hasFocus() ||
    !pending ||
    pending.generation !== __GENERATION__ ||
    pending.target !== document.activeElement ||
    !pending.target.isConnected ||
    !pending.target.isContentEditable
  ) {
    return;
  }
  const bytes = Uint8Array.from(atob("__PNG__"), (character) => character.charCodeAt(0));
  const data = new DataTransfer();
  data.items.add(new File([bytes], "Clipboard.png", { type: "image/png" }));
  pending.target.dispatchEvent(
    new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
  );
})();

async function attachFiles(kind, point) {
  const document = this.ownerDocument;
  const deadline = performance.now() + 1000;
  const current = () =>
    !this.eloCancelled &&
    performance.now() < deadline &&
    document.location.origin === "https://web.whatsapp.com" &&
    !document.hidden &&
    document.hasFocus();
  try {
    if (!current() || !this.files.length) return false;
    const data = new DataTransfer();
    for (const file of this.files) data.items.add(file);
    if (kind === "paste") {
      const target = this.eloTarget;
      if (!target?.isConnected || target !== document.activeElement || !target.isContentEditable)
        return false;
      target.dispatchEvent(
        new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
      );
    } else {
      const x = point[0] * innerWidth,
        y = point[1] * innerHeight;
      for (const type of ["dragenter", "dragover", "drop"]) {
        if (!current()) return false;
        const target = document.elementFromPoint(x, y);
        if (!target?.isConnected) return false;
        target.dispatchEvent(
          new DragEvent(type, { dataTransfer: data, bubbles: true, cancelable: true }),
        );
        // Allow the attachment overlay to render before locating the next target.
        if (type !== "drop")
          await Promise.race([
            new Promise((resolve) => requestAnimationFrame(resolve)),
            new Promise((resolve) => setTimeout(resolve, 100)),
          ]);
      }
    }
    return true;
  } finally {
    this.remove();
  }
}

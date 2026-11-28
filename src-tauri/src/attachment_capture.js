(() => {
  if (location.origin !== "https://web.whatsapp.com" || document.hidden || !document.hasFocus())
    return null;
  const input = document.createElement("input");
  input.type = "file";
  input.multiple = true;
  input.eloTarget = document.activeElement;
  return input;
})();

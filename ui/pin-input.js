export function initializePinInputs(container) {
  const inputs = [...container.querySelectorAll("input")];
  inputs.forEach((input, index) => {
    input.addEventListener("input", () => {
      input.value = input.value.replace(/\D/g, "").slice(-1);
      if (input.value && index < inputs.length - 1) inputs[index + 1].focus();
    });
    input.addEventListener("keydown", (event) => {
      if ((event.key === "Backspace" && !input.value) || event.key === "ArrowLeft") {
        if (index > 0) inputs[index - 1].focus();
      } else if (event.key === "ArrowRight" && index < inputs.length - 1) {
        inputs[index + 1].focus();
      }
    });
    input.addEventListener("paste", (event) => {
      event.preventDefault();
      const digits = event.clipboardData.getData("text").replace(/\D/g, "").slice(0, inputs.length);
      inputs.forEach((element, position) => {
        element.value = digits[position] || "";
      });
      inputs[Math.min(digits.length, inputs.length - 1)].focus();
    });
  });
  return inputs;
}

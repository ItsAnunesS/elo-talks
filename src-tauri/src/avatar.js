(() => {
  // Match explicit self-profile controls only; never inspect conversation text.
  const labels = ['Você', 'You', 'Perfil', 'Profile', 'Seu perfil', 'Your profile'];
  const matches = new Map();
  for (const label of labels) {
    for (const node of document.querySelectorAll(`[aria-label="${label}"], [title="${label}"]`)) {
      const control = node.closest('button, [role="button"]') || node;
      const bounds = control.getBoundingClientRect();
      if (bounds.left < 0 || bounds.right > 96 || bounds.top < innerHeight / 2 || bounds.bottom > innerHeight + 1 || bounds.width < 8 || bounds.width > 96 || bounds.height < 8 || bounds.height > 96) continue;
      if (getComputedStyle(control).visibility !== 'visible') continue;
      const images = control.querySelectorAll('img');
      if (images.length !== 1) continue;
      const image = images[0];
      if (!image.complete || image.naturalWidth === 0) continue;
      const source = image.currentSrc;
      if (source && source.length <= 4096) matches.set(source, image);
    }
  }
  if (matches.size !== 1) return '';
  const [source, image] = [...matches][0];
  const url = new URL(source);
  const remoteImage = url.protocol === 'https:' && !url.username && !url.password && (!url.port || url.port === '443');
  if (!remoteImage && !source.startsWith('blob:https://web.whatsapp.com/')) return '';
  let png = null;
  // Blob images belong to the page origin. Use normal canvas rules; never
  // relax CORS or reload a remote image to obtain its pixels.
  if (source.startsWith('blob:https://web.whatsapp.com/')) {
    try {
      const canvas = document.createElement('canvas');
      canvas.width = canvas.height = 64;
      canvas.getContext('2d').drawImage(image, 0, 0, 64, 64);
      const value = canvas.toDataURL('image/png');
      if (value.length <= 44000) png = value;
    } catch { /* Keep initials when the browser denies access. */ }
  }
  return JSON.stringify({source, png});
})()

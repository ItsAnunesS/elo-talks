(() => {
  const labels = ['Você', 'You', 'Perfil', 'Profile', 'Seu perfil', 'Your profile'];
  const known = node => labels.includes(node.getAttribute('aria-label')) || labels.includes(node.getAttribute('title'));
  const candidates = [];
  for (const image of document.querySelectorAll('img')) {
    const r = image.getBoundingClientRect();
    if (r.left < 0 || r.right > 100 || r.top < innerHeight / 2 || r.width < 8 || r.height < 8 || r.width > 96 || r.height > 96) continue;
    const ancestry = [];
    let node = image;
    for (let i = 0; node && i < 5; i++, node = node.parentElement) {
      const b = node.getBoundingClientRect();
      ancestry.push({tag: node.tagName, button:node.matches('button,[role="button"]'), expectedLabel:known(node), width:Math.round(b.width), height:Math.round(b.height)});
    }
    candidates.push({loaded:image.complete && image.naturalWidth > 0, ancestry});
    if (candidates.length === 4) break;
  }
  return {selfImageCandidates:candidates, expectedLabelCount:labels.reduce((n,l)=>n+document.querySelectorAll(`[title="${l}"],[aria-label="${l}"]`).length,0), chatLayout:!!document.querySelector('#pane-side')};
})()

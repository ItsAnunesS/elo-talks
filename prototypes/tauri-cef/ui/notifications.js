(async () => {
  try {
    if (await Notification.requestPermission() !== 'granted') throw new Error();
    const notification = new Notification('Synthetic CEF sender', { body: 'Synthetic CEF private body', tag: 'elo-compatibility-probe' });
    notification.onshow = () => { document.title = 'Native notification shown'; };
    notification.onerror = () => console.log('ELO_CEF_FAIL');
    // The separate D-Bus fixture, not this page, proves OS delivery.
    setTimeout(() => { notification.close(); console.log('ELO_CEF_PASS'); }, 5000);
  } catch { console.log('ELO_CEF_FAIL'); }
})();

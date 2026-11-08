(async () => {
  const peers = [];
  let audio, oscillator, timer, stream;
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const assert = (condition) => { if (!condition) throw new Error('Probe failed'); };
  try {
    assert(isSecureContext && typeof RTCPeerConnection === 'function');
    // The prototype denies device capture; no real microphone or camera is used.
    let denied = false;
    try {
      const unexpected = await navigator.mediaDevices.getUserMedia({ audio: true, video: true });
      unexpected.getTracks().forEach((track) => track.stop());
    } catch (error) { denied = error.name === 'NotAllowedError'; }
    assert(denied);
    const canvas = document.createElement('canvas');
    canvas.width = 160; canvas.height = 120;
    const context = canvas.getContext('2d');
    let frame = 0;
    timer = setInterval(() => {
      context.fillStyle = frame++ % 2 ? 'red' : 'blue';
      context.fillRect(0, 0, 160, 120);
    }, 50);
    stream = canvas.captureStream(20);
    audio = new AudioContext();
    oscillator = audio.createOscillator();
    const destination = audio.createMediaStreamDestination();
    oscillator.connect(destination); oscillator.start();
    stream.addTrack(destination.stream.getAudioTracks()[0]);
    await audio.resume();
    const sender = new RTCPeerConnection({ iceServers: [] });
    const receiver = new RTCPeerConnection({ iceServers: [] });
    peers.push(sender, receiver);
    const remote = new MediaStream();
    document.getElementById('remote').srcObject = remote;
    receiver.ontrack = (event) => remote.addTrack(event.track);
    for (const track of stream.getTracks()) sender.addTrack(track, stream);
    // Wait for ICE gathering before exchanging descriptions; never contact STUN/TURN.
    const gathered = (peer) => new Promise((resolve) => {
      if (peer.iceGatheringState === 'complete') return resolve();
      peer.addEventListener('icegatheringstatechange', () => {
        if (peer.iceGatheringState === 'complete') resolve();
      });
    });
    await sender.setLocalDescription(await sender.createOffer());
    await gathered(sender);
    await receiver.setRemoteDescription(sender.localDescription);
    await receiver.setLocalDescription(await receiver.createAnswer());
    await gathered(receiver);
    await sender.setRemoteDescription(receiver.localDescription);
    let success = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      const stats = [...(await receiver.getStats()).values()];
      const audioReceived = stats.some((s) => s.type === 'inbound-rtp' && s.kind === 'audio' && s.bytesReceived > 0);
      const videoReceived = stats.some((s) => s.type === 'inbound-rtp' && s.kind === 'video' && s.framesDecoded > 0);
      if (audioReceived && videoReceived) { success = true; break; }
      await wait(100);
    }
    assert(success && remote.getTracks().length === 2);
    document.getElementById('result').textContent = 'Passed: denied capture, received audio and decoded video.';
    console.log('ELO_CEF_PASS');
  } catch {
    document.getElementById('result').textContent = 'Failed: media capture policy or audio/video loopback.';
    console.log('ELO_CEF_FAIL');
  } finally {
    peers.forEach((peer) => peer.close());
    clearInterval(timer);
    stream?.getTracks().forEach((track) => track.stop());
    oscillator?.stop();
    await audio?.close();
  }
})();

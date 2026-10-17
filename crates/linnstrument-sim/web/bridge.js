'use strict';
const invoke = window.__TAURI__?.core.invoke;
const subscribers = [];
let latest = null;
let bridgeError = null;
async function send(command) {
  if (!invoke) { document.querySelector('#error').textContent = 'Native bridge unavailable. Run the desktop application.'; return false; }
  try { await invoke('simulator_command', {command}); bridgeError = null; return true; }
  catch(error) { bridgeError = String(error); document.querySelector('#error').textContent = bridgeError; return false; }
}
async function poll() {
  try {
    if (invoke) {
      const raw = await invoke('simulator_snapshot');
      if (raw) {
        latest = JSON.parse(raw);
        document.querySelector('#connection').textContent = latest.connected ? (latest.paused ? 'PAUSED' : latest.external_host ? 'FIRMWARE · OS MIDI' : 'FIRMWARE CONNECTED') : 'DISCONNECTED';
        document.querySelector('#error').textContent = latest.error || bridgeError || '';
        for (const subscriber of subscribers) subscriber(latest);
      }
    }
  } catch(error) { document.querySelector('#connection').textContent = 'DISCONNECTED'; document.querySelector('#error').textContent = String(error); }
  setTimeout(poll, 33); // Snapshot display only; native scheduler owns emulated time.
}
poll();

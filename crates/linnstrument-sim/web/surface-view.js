import {activeMode, describePad, performanceSummary} from './surface-help.js';
import {releaseAllContacts} from './instrument.js';

const surface = document.querySelector('#surface');
const title = document.querySelector('#pad-title');
const description = document.querySelector('#pad-description');
const light = document.querySelector('#pad-light');
const modeButtons = [...document.querySelectorAll('[data-surface-mode]')];
let inspected = null;
let changingMode = false;

function showPadHelp() {
  if (!inspected || !latest) return;
  const help = describePad(latest, inspected.column, inspected.row);
  if (!help) return;
  if (title.textContent !== help.title) title.textContent = help.title;
  if (description.textContent !== help.action) description.textContent = help.action;
  const location = `Column ${inspected.column} · row ${inspected.row} · ${help.light}`;
  if (light.textContent !== location) light.textContent = location;
}

function inspectPointer(event) {
  if (!latest) return;
  const box = surface.getBoundingClientRect();
  const columns = latest.firmware.model === 128 ? 17 : 26;
  inspected = {
    column: Math.min(columns - 1, Math.max(0, Math.floor((event.clientX - box.left) / box.width * columns))),
    row: Math.min(7, Math.max(0, 7 - Math.floor((event.clientY - box.top) / box.height * 8))),
  };
  showPadHelp();
}
surface.addEventListener('pointermove', inspectPointer);
surface.addEventListener('pointerdown', inspectPointer);
surface.addEventListener('focus', () => {
  inspected ??= {column:1, row:0};
  showPadHelp();
});
surface.addEventListener('keydown', event => {
  if (!event.altKey || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key) || !latest) return;
  event.preventDefault();
  inspected ??= {column:1, row:0};
  const columns = latest.firmware.model === 128 ? 17 : 26;
  inspected.column = Math.max(0, Math.min(columns - 1, inspected.column + (event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0)));
  inspected.row = Math.max(0, Math.min(7, inspected.row + (event.key === 'ArrowUp' ? 1 : event.key === 'ArrowDown' ? -1 : 0)));
  showPadHelp();
});

for (const button of modeButtons) button.addEventListener('click', async () => {
  if (!latest?.connected || latest.external_host || changingMode) return;
  changingMode = true;
  try {
    // Release local contact ownership too; a mode change must not leave latched fingers behind.
    if (!await releaseAllContacts()) return;
    const mode = button.dataset.surfaceMode;
    if (mode === 'chords' || mode === 'strumplate') {
      const settings = {...latest.host.performance.harmony, playing:mode === 'strumplate' ? 'strumplate' : 'chord_pads', input_octaves:true, input_anchor:28};
      if (!await send({type:'harmony', settings})) return;
    }
    await send({type:'surface', mode:mode === 'strumplate' ? 'chords' : mode});
  } finally {
    changingMode = false;
  }
});

document.querySelector('#show-tools').onclick = async () => {
  try { await invoke('show_host'); }
  catch (error) { document.querySelector('#error').textContent = String(error); }
};
const deviceOptions = document.querySelector('#device-options');
document.querySelector('#device-options-toggle').onclick = event => {
  deviceOptions.hidden = !deviceOptions.hidden;
  event.currentTarget.setAttribute('aria-expanded', String(!deviceOptions.hidden));
};

for (const id of ['root-note','root-scale']) document.getElementById(id).addEventListener('change', async () => {
  if (!latest?.connected || latest.external_host) return;
  await send({type:'harmony', settings:{...latest.host.performance.harmony,
    tonic:60 + Number(document.getElementById('root-note').value),
    scale:document.getElementById('root-scale').value}});
});

subscribers.push(snapshot => {
  const mode = activeMode(snapshot);
  document.getElementById('chord-key').hidden = snapshot.external_host || !['chords','strumplate'].includes(mode);
  // Native select menus must keep their pending choice while open.
  // Snapshot refreshes otherwise reset keyboard/mouse selection before change.
  const root = document.getElementById('root-note');
  const scale = document.getElementById('root-scale');
  if (document.activeElement !== root) root.value = String(snapshot.host.performance.harmony.tonic % 12);
  if (document.activeElement !== scale) scale.value = snapshot.host.performance.harmony.scale;
  for (const button of modeButtons) {
    button.setAttribute('aria-pressed', String(button.dataset.surfaceMode === mode));
    button.disabled = !snapshot.connected || snapshot.external_host || changingMode;
  }
  document.querySelector('#performance-summary').textContent = performanceSummary(snapshot);
  document.querySelector('#instrument-output').textContent = snapshot.external_host ? 'MIDI connected to external host' : 'MIDI capture · no audio instrument connected';
  showPadHelp();
});

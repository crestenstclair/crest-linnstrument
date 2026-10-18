// Read-only explanations. Pad input and LED rendering remain in the firmware path.
export const degrees = ['I', 'II', 'III', 'IV', 'V', 'VI', 'VII'];
const controlNames = ['Global settings', 'Split', 'Switch 2', 'Switch 1', 'Octave / transpose', 'Volume', 'Preset', 'Per-split settings'];
const gestures = 'Move X for pitch, Y for timbre; wheel or the pressure control changes pressure. Release ends the note.';

export function noteName(key) {
  return ['C', 'C♯', 'D', 'E♭', 'E', 'F', 'F♯', 'G', 'A♭', 'A', 'B♭', 'B'][key % 12] + (Math.floor(key / 12) - 1);
}

export function lightName(rgb) {
  const peak = Math.max(...rgb);
  if (peak === 0) return 'Off';
  const bits = rgb.map(value => value > peak * .4);
  const index = (bits[0] ? 4 : 0) + (bits[1] ? 2 : 0) + (bits[2] ? 1 : 0);
  return ['Off', 'Blue', 'Green', 'Cyan', 'Red', 'Magenta', 'Yellow', 'White'][index];
}

export function describePad(snapshot, column, row) {
  const columns = snapshot?.firmware.model === 128 ? 17 : 26;
  if (!snapshot || !Number.isInteger(column) || !Number.isInteger(row) || column < 0 || column >= columns || row < 0 || row > 7) return null;
  const index = column * 8 + row;
  const light = `${lightName(snapshot.firmware.leds[index] ?? [0, 0, 0])} firmware light`;
  if (!snapshot.connected) return {title:'Device disconnected', action:'Reconnect or reset the firmware worker before playing.', light};
  if (column === 0) return {title:controlNames[row], action:'Firmware control. Press or hold this cell to use the instrument’s own controls; the resulting settings lights remain firmware-driven.', light};
  if (snapshot.external_host) return {title:`Pad ${column}, row ${row}`, action:'An external MIDI host owns this surface. Its current pad assignments are not supplied to this simulator.', light};
  const surface = snapshot.firmware.surface;
  if (surface && !surface.normal) return {title:'Firmware settings', action:'The instrument is showing its settings page. These lights are firmware controls, not the host’s playing or clip layout.', light};
  const state = snapshot.host.performance;
  if (state.mode === 'clips' || (state.mode === 'chords' && state.harmony.input_octaves)) {
    const help = snapshot.host.pad_help.find(pad => pad.column === column && pad.row === row);
    return {...(help ?? {title:'Unassigned pad', action:'No performance action is assigned to this cell in the current layout.'}), light};
  }
  const note = surface?.notes[index];
  if (note == null) return {title:'Firmware-controlled pad', action:'This cell is not an ordinary note in the current firmware configuration. Use its firmware settings to inspect the assignment.', light};
  const highlight = {accent:'Firmware accent-note marker.', scale:'Firmware main-note highlight.', unmarked:'Unmarked chromatic note; it is still playable.', custom:'Custom firmware light pattern.'}[surface.highlights[index]];
  if (state.mode === 'play') return {title:`${noteName(note)} · MIDI ${note}`, action:`${highlight} ${gestures}`, light};
  const harmony = state.harmony;
  if (harmony.playing === 'strumplate') return {title:`Pick a tone of chord ${degrees[harmony.degree]}`, action:`Separate pad presses pick successive chord tones across octaves. A continuous slide bends its existing note; it does not retrigger a harp sweep. ${highlight}`, light};
  const degree = ((note - harmony.input_anchor) % 7 + 7) % 7;
  return {title:`Chord ${degrees[degree]}`, action:`Press one pad to play this scale degree using the selected voicing and articulation. ${gestures} The displayed lights are the firmware’s note markers, not a custom chord-color overlay.`, light};
}

export function activeMode(snapshot) {
  const state = snapshot.host.performance;
  return state.mode === 'chords' && state.harmony.playing === 'strumplate' ? 'strumplate' : state.mode;
}

export function performanceSummary(snapshot) {
  const state = snapshot.host.performance;
  if (snapshot.external_host) return 'External host controls the performance';
  if (state.mode === 'clips') {
    const running = state.slots.filter(slot => slot === 'playing').length;
    return `${running} clips playing · harmony ${degrees[state.effective_harmony.degree]} · ${state.fill ? 'Fill held' : 'Fill released'}`;
  }
  if (state.mode === 'play') return `${state.active_voices} notes active · independent pitch, timbre and pressure`;
  const harmony = state.harmony;
  const articulation = {block:'Together', strum:'Strum', arpeggio:'Arpeggio', humanized:'Humanized'}[harmony.articulation.type];
  return `${noteName(harmony.tonic).replace(/-?\d+$/, '')} ${harmony.scale.replaceAll('_', ' ')} · ${harmony.quality === 'diatonic' || !harmony.quality ? 'Diatonic' : `Chromatic ${harmony.quality}`} · ${harmony.extension.replaceAll('_',' ')} · Inv ${harmony.inversion} · ${harmony.spread ? 'Open' : 'Close'} · ${degrees[harmony.degree]} · ${harmony.playing === 'strumplate' ? 'Manual picking' : articulation} · ${state.active_voices} notes`;
}

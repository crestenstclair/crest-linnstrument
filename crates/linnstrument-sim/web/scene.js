// Parameter capture is a host backend, not a claim about the audible instrument.
const form = document.querySelector('#scene-form');
const rows = document.querySelector('#scene-parameters');
const slider = document.querySelector('#scene-position');
let renderedScene = undefined;
let destinations = [];

function addParameter(parameter) {
  const row = document.createElement('div');
  row.className = 'harmony-fields';
  const destination = document.createElement('select');
  destination.name = 'destination';
  destination.setAttribute('aria-label', 'Parameter destination');
  destination.replaceChildren(...destinations.map(id => new Option(String(id), String(id))));
  destination.value = String(parameter.address.destination);
  row.append(destination);
  for (const [name, label, maximum, value] of [
    ['parameter', 'Parameter reference', 4294967295, parameter.address.parameter],
    ['a', 'State A · normalized value', 65535, parameter.a],
    ['b', 'State B · normalized value', 65535, parameter.b],
  ]) {
    const field = document.createElement('label');
    field.textContent = label;
    const input = document.createElement('input');
    Object.assign(input, {name, type:'number', min:'0', max:String(maximum), step:'1', value:String(value), required:true});
    field.append(input);
    row.append(field);
  }
  const remove = document.createElement('button');
  remove.type = 'button'; remove.textContent = 'Remove parameter';
  remove.onclick = () => row.remove();
  row.append(remove);
  rows.append(row);
}
document.querySelector('#scene-add').onclick = () => {
  if (rows.children.length >= 16 || !destinations.length) return;
  const used = [...rows.querySelectorAll('[name=parameter]')].map(input => Number(input.value));
  let reference = 1; while (used.includes(reference)) reference++;
  addParameter({address:{destination:destinations[0], parameter:reference}, a:0, b:65535});
};
form.onsubmit = event => {
  event.preventDefault(); if (!form.reportValidity()) return;
  const scene = [...rows.children].map(row => ({
    address:{destination:Number(row.querySelector('[name=destination]').value), parameter:Number(row.querySelector('[name=parameter]').value)},
    a:Number(row.querySelector('[name=a]').value), b:Number(row.querySelector('[name=b]').value),
  })).sort((left,right) => left.address.destination-right.address.destination || left.address.parameter-right.address.parameter);
  send({type:'scene_morph', scene:scene.length?scene:null});
};
slider.oninput = () => send({type:'morph_position', position:Number(slider.value)});
for (const [id, position] of [['scene-a',0],['scene-b',65535]]) {
  document.getElementById(id).onclick = () => send({type:'morph_position', position});
}
export function renderScene(host) {
  destinations = [...new Set(host.tracks.map(track => track.instrument.destination))];
  const key = JSON.stringify(host.scene_morph);
  if (key !== renderedScene) {
    rows.replaceChildren();
    for (const parameter of host.scene_morph ?? []) addParameter(parameter);
    renderedScene = key;
  }
  const enabled = !!host.scene_morph;
  slider.disabled = !enabled;
  document.querySelector('#scene-a').disabled = !enabled;
  document.querySelector('#scene-b').disabled = !enabled;
  const position = host.performance.morph_position;
  slider.value = position ?? 0;
  document.querySelector('#scene-position-value').textContent = position === null ? 'No position emitted' : `${(position/65535*100).toFixed(2)}% B`;
  document.querySelector('#parameter-capture').textContent = host.recent_parameters.map(event => `${event.at} · destination ${event.address.destination} · parameter ${event.address.parameter} = ${event.value}`).join('\n');
  document.querySelector('#scene-status').textContent = `${host.parameter_capture_count} captured parameter events · ${enabled?'Clip pads row 3 controls the morph':'Add endpoints, then Apply'}`;
}

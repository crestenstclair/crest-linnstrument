'use strict';
import {defaultBindings,parseBindings} from './keyboard.js';
const canvas=document.querySelector('#surface'), ctx=canvas.getContext('2d');
const pressure=document.querySelector('#pressure'), latch=document.querySelector('#latch');
const contacts=new Map(), pointers=new Map(), keys=new Map();
let nextId=1, selected=null, columns=26;
const labels=['PER SPLIT','PRESET','VOLUME','OCTAVE / TRANSPOSE','SPLIT','SWITCH 1','SWITCH 2','GLOBAL SETTINGS'];
document.querySelector('#controls').replaceChildren(...labels.map(label=>{const div=document.createElement('div');div.textContent=label;return div;}));
function position(event){const box=canvas.getBoundingClientRect();return {x:Math.max(0,Math.min(columns-1,(event.clientX-box.left)/box.width*columns-.5)),y:Math.max(0,Math.min(7,7.5-(event.clientY-box.top)/box.height*8))};}
function active(){return latest?.connected===true;}
function update(contact){if(!active())return;contacts.set(contact.id,contact);selected=contact.id;send({type:'contact',...contact});}
function release(id){contacts.delete(id);send({type:'release',id});if(selected===id)selected=null;}
canvas.addEventListener('pointerdown',event=>{
  if(!active())return;
  canvas.focus();canvas.setPointerCapture(event.pointerId);
  const p=position(event);let contact=[...contacts.values()].find(c=>Math.hypot(c.x-p.x,c.y-p.y)<.28);
  if(!contact)contact={id:nextId++,...p,z:event.pointerType!=='mouse'&&event.pressure>0?event.pressure:Number(pressure.value),latched:latch.checked};
  pointers.set(event.pointerId,contact.id);update(contact);
});
canvas.addEventListener('pointermove',event=>{
  const id=pointers.get(event.pointerId),contact=contacts.get(id);if(!contact)return;
  update({...contact,...position(event),z:event.pointerType!=='mouse'&&event.pressure>0?event.pressure:contact.z});
});
function endPointer(event){const id=pointers.get(event.pointerId);pointers.delete(event.pointerId);if(contacts.has(id)&&!contacts.get(id).latched)release(id);}
canvas.addEventListener('pointerup',endPointer);canvas.addEventListener('pointercancel',endPointer);canvas.addEventListener('lostpointercapture',endPointer);
function setPressure(value){value=Math.max(0,Math.min(1,value));pressure.value=value;document.querySelector('#pressure-value').textContent=`${Math.round(value*100)}%`;const contact=contacts.get(selected);if(contact)update({...contact,z:value});}
canvas.addEventListener('wheel',event=>{event.preventDefault();setPressure(Number(pressure.value)-Math.sign(event.deltaY)*.03);},{passive:false});
pressure.addEventListener('input',()=>setPressure(Number(pressure.value)));
let bindings=defaultBindings(),bindingsLoaded=false;
const bindingText=document.querySelector('#keyboard-settings');
function describeBindings(){const padKeys=Object.keys(bindings.pads).join(' ').toUpperCase();document.querySelector('#keyboard-help').textContent=`${padKeys} hold independent contacts. ${bindings.pressure_down} / ${bindings.pressure_up} adjust pressure. Arrows move selected contact; Shift makes fine moves.`;canvas.setAttribute('aria-label',`LinnStrument sensor surface. Keys ${padKeys}. Arrow keys move selected finger. Pressure keys ${bindings.pressure_down} and ${bindings.pressure_up}.`);}
document.querySelector('#keyboard-form').onsubmit=event=>{event.preventDefault();if(latest?.contacts.length){document.querySelector('#keyboard-status').textContent='Release all contacts before changing keys.';return;}try{const next=parseBindings(bindingText.value,columns);localStorage.setItem('linnstrument-keyboard-v1',JSON.stringify(next));bindings=next;describeBindings();document.querySelector('#keyboard-status').textContent='Keyboard settings saved.';}catch(error){document.querySelector('#keyboard-status').textContent=error.message;}};
document.querySelector('#keyboard-defaults').onclick=()=>{bindingText.value=JSON.stringify(defaultBindings(),null,2);};
window.addEventListener('keydown',event=>{
  if(event.target.matches('input,textarea,select')||event.metaKey||event.ctrlKey||event.altKey||!active())return;
  const key=event.key.toLowerCase(),pad=bindings.pads[key];
  if(pad){event.preventDefault();if(event.repeat||keys.has(event.code))return;const id=nextId++;keys.set(event.code,id);update({id,...pad,z:Number(pressure.value),latched:latch.checked});return;}
  if(['ArrowLeft','ArrowRight','ArrowUp','ArrowDown',bindings.pressure_down,bindings.pressure_up].includes(event.key)){
    event.preventDefault();if(key===bindings.pressure_down||key===bindings.pressure_up){setPressure(Number(pressure.value)+(key===bindings.pressure_up?.03:-.03));return;}
    const contact=contacts.get(selected);if(!contact)return;const step=event.shiftKey?.02:.1;
    update({...contact,x:Math.max(0,Math.min(columns-1,contact.x+(event.key==='ArrowRight'?step:event.key==='ArrowLeft'?-step:0))),y:Math.max(0,Math.min(7,contact.y+(event.key==='ArrowUp'?step:event.key==='ArrowDown'?-step:0)))});
  }
});
window.addEventListener('keyup',event=>{const id=keys.get(event.code);keys.delete(event.code);if(contacts.has(id)&&!contacts.get(id).latched)release(id);});
function loseFocus(){pointers.clear();keys.clear();for(const [id,c] of contacts)if(!c.latched)contacts.delete(id);send({type:'release_momentary'});for(const number of [1,2])send({type:'foot_switch',number,pressed:false});}
window.addEventListener('blur',loseFocus);document.addEventListener('visibilitychange',()=>{if(document.hidden)loseFocus();});
for(const [id,number] of [['foot-left',1],['foot-right',2]]){const button=document.getElementById(id);button.onpointerdown=event=>{button.setPointerCapture(event.pointerId);send({type:'foot_switch',number,pressed:true});};for(const type of ['pointerup','pointercancel','lostpointercapture'])button.addEventListener(type,()=>send({type:'foot_switch',number,pressed:false}));button.onkeydown=event=>{if((event.key===' '||event.key==='Enter')&&!event.repeat)send({type:'foot_switch',number,pressed:true});};button.onkeyup=event=>{if(event.key===' '||event.key==='Enter')send({type:'foot_switch',number,pressed:false});};}
for(const type of ['export','verify'])document.getElementById(`${type}-run`).onclick=()=>send({type:`${type}_run`,path:document.querySelector('#run-path').value});
document.querySelector('#release').onclick=()=>{contacts.clear();keys.clear();pointers.clear();send({type:'release_all'});};
document.querySelector('#example').onclick=()=>send({type:'play_example'});
document.querySelector('#run-scenario').onclick=()=>send({type:'run_scenario',path:document.querySelector('#scenario-path').value});
document.querySelector('#pause').onclick=()=>send({type:'pause',paused:!latest?.paused});
document.querySelector('#step').onclick=()=>send({type:'step',micros:10000});
document.querySelector('#reset').onclick=()=>{contacts.clear();send({type:'reset'});};
function draw(snapshot){
  columns=snapshot.firmware.model===128?17:26;canvas.width=columns*42;canvas.height=8*42;
  ctx.fillStyle='#222a27';ctx.fillRect(0,0,canvas.width,canvas.height);
  snapshot.firmware.leds.forEach((rgb,index)=>{
    const x=Math.floor(index/8)*42,y=(7-index%8)*42;
    ctx.fillStyle='#666e62';ctx.beginPath();ctx.roundRect(x+2,y+2,38,38,4);ctx.fill();
    if(rgb.some(v=>v>0)){ctx.fillStyle=`rgb(${rgb.join(',')})`;ctx.shadowBlur=10;ctx.shadowColor=ctx.fillStyle;ctx.beginPath();ctx.arc(x+21,y+21,5,0,Math.PI*2);ctx.fill();ctx.shadowBlur=0;}
  });
  if(document.querySelector('#markers').checked)for(const c of snapshot.contacts){const x=(c.x+.5)*42,y=(7.5-c.y)*42;ctx.strokeStyle=c.id===selected?'#fff':'#c5eb8b';ctx.lineWidth=2;ctx.setLineDash(c.latched?[3,3]:[]);ctx.beginPath();ctx.arc(x,y,10+c.z*5,0,Math.PI*2);ctx.stroke();ctx.setLineDash([]);ctx.fillStyle='#fff';ctx.font='10px monospace';ctx.fillText(String(c.id),x+12,y-12);}
}
const contactButtons=new Map();
function renderContactButtons(snapshot){
  const ids=new Set(snapshot.contacts.map(c=>c.id));for(const [id,button]of contactButtons)if(!ids.has(id)){button.remove();contactButtons.delete(id);}
  for(const c of snapshot.contacts){let button=contactButtons.get(c.id);if(!button){button=document.createElement('button');button.onclick=()=>{const current=latest.contacts.find(item=>item.id===c.id);if(!current)return;selected=c.id;contacts.set(c.id,current);pressure.value=current.z;};button.ondblclick=()=>release(c.id);contactButtons.set(c.id,button);document.querySelector('#contact-list').append(button);}button.textContent=`${c.id}${c.latched?' LATCHED':''}`;button.className=c.id===selected?'selected':'';}
}
subscribers.push(snapshot=>{
  if(!snapshot.connected){contacts.clear();pointers.clear();keys.clear();}
  document.querySelector('#scenario-status').textContent=snapshot.scenario_steps_remaining?`${snapshot.scenario_steps_remaining} native script steps remaining`:'';
  document.querySelector('#run-status').textContent=snapshot.run_full?'Buffer full: reset before new capture':`${snapshot.run_steps} steps captured${snapshot.verified_steps===null?'':` · ${snapshot.verified_steps} replay steps verified`}`;
  document.querySelector('#model').textContent=snapshot.firmware.model;
  document.querySelector('#revision').textContent=`54bb1c9 · sensor SPI / native C++`;
  document.querySelector('#mode').textContent=snapshot.host.performance.mode==='clips'?'CLIPS · USER FIRMWARE MODE':snapshot.host.performance.mode==='chords'?'CHORDS · FIRMWARE MPE ±48 ST':'PLAY · MPE ±48 ST';
  document.querySelector('#fingers').textContent=`${snapshot.contacts.length} FINGERS`;
  document.querySelector('#pause').textContent=snapshot.paused?'Resume':'Pause';document.querySelector('#step').disabled=!snapshot.paused;
  draw(snapshot);
  if(!bindingsLoaded){bindingsLoaded=true;try{const stored=localStorage.getItem('linnstrument-keyboard-v1');if(stored)bindings=parseBindings(stored,columns);}catch(error){document.querySelector('#keyboard-status').textContent=`Using defaults: ${error.message}`;}bindingText.value=JSON.stringify(bindings,null,2);describeBindings();}
  renderContactButtons(snapshot);
  const f=snapshot.firmware;document.querySelector('#diagnostics').textContent=`Firmware  ${f.firmware}\nTime      ${f.at_us} µs\nADC reads ${f.sensor_reads}\nSelections ${f.sensor_selects}\nLED writes ${f.led_writes}\nContacts  ${JSON.stringify(snapshot.contacts)}`;
  document.querySelector('#midi').textContent='FIRMWARE OUTPUT (serial completion µs)\n'+snapshot.recent_firmware_midi.slice(-72).map(([at,b])=>`${at} ${b.toString(16).padStart(2,'0')}`).join(' ')+'\n\nHOST OUTPUT (channels reallocated)\n'+snapshot.host.recent_midi.map(m=>`${m.at}  ${m.message.bytes.slice(0,m.message.len).map(b=>b.toString(16).padStart(2,'0')).join(' ')}`).join('\n');
});

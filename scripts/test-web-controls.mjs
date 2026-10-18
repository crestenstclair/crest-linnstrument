import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const source=readFileSync(new URL('../crates/linnstrument-sim/web/keyboard.js',import.meta.url),'utf8');
const {parseBindings,defaultBindings,layoutBindings}=await import(`data:text/javascript,${encodeURIComponent(source)}`);
assert.deepEqual(parseBindings(JSON.stringify(defaultBindings()),17),defaultBindings());
const custom={pads:{q:{x:1,y:0},w:{x:16,y:7}},pressure_down:'-',pressure_up:'='};
assert.deepEqual(parseBindings(JSON.stringify(custom),17),custom);
for(const invalid of [null,[],{...custom,unexpected:1},{...custom,pressure_up:'-'},{...custom,pressure_down:'q'},{...custom,pads:{Q:{x:1,y:0}}},{...custom,pads:{a:{x:17,y:0}}},{...custom,pads:{a:{x:1,y:-1}}},{...custom,pads:{a:{x:1,y:0,z:.5}}}])assert.throws(()=>parseBindings(JSON.stringify(invalid),17));
assert.throws(()=>parseBindings(' '.repeat(4097),26));
console.log('Keyboard binding validation passed for both geometries and invalid input.');

const holdSource=readFileSync(new URL('../crates/linnstrument-sim/web/hold-control.js',import.meta.url),'utf8');
const {bindHoldControl}=await import(`data:text/javascript,${encodeURIComponent(holdSource)}`);
const button=new EventTarget(),window=new EventTarget(),changes=[];
button.setPointerCapture=()=>{};
let focused=false;button.focus=()=>{focused=true;};
bindHoldControl(button,window,held=>changes.push(held));
function dispatch(target,type,properties={}){
  target.dispatchEvent(Object.assign(new Event(type,{cancelable:true}),properties));
}
dispatch(button,'click');assert.equal(focused,true);
dispatch(button,'keydown',{key:' ',repeat:true});
assert.deepEqual(changes,[]); // Held key returning after focus loss cannot retrigger.
dispatch(button,'pointerdown',{button:0,pointerId:1});
dispatch(button,'keydown',{key:' ',repeat:false});
dispatch(button,'keydown',{key:' ',repeat:true});
dispatch(button,'pointerup',{pointerId:1});
assert.deepEqual(changes,[true]); // keyboard still held
dispatch(button,'keyup',{key:' '});
assert.deepEqual(changes,[true,false]);
dispatch(button,'pointerdown',{button:0,pointerId:2});
dispatch(button,'lostpointercapture',{pointerId:2});
dispatch(button,'keydown',{key:'Enter',repeat:false});
dispatch(window,'blur');
dispatch(button,'keydown',{key:'Enter',repeat:true});
dispatch(button,'keyup',{key:'Enter'});
assert.deepEqual(changes,[true,false,true,false,true,false]);
button.disabled=true;
dispatch(button,'pointerdown',{button:0,pointerId:3});
dispatch(button,'keydown',{key:' ',repeat:false});
assert.equal(changes.length,6);
console.log('Momentary Fill preserves overlapping holds and releases on focus/capture loss.');

const helpSource=readFileSync(new URL('../crates/linnstrument-sim/web/surface-help.js',import.meta.url),'utf8');
const {describePad,activeMode,performanceSummary}=await import(`data:text/javascript,${encodeURIComponent(helpSource)}`);
const snapshot={connected:true,external_host:false,firmware:{model:128,leds:Array.from({length:136},()=>[0,0,0]),surface:{normal:true,user_mode:false,notes:Array(136).fill(null),highlights:Array(136).fill('unmarked')}},host:{performance:{mode:'play',active_voices:0,harmony:{input_anchor:60,degree:0,playing:'chord_pads',tonic:60,scale:'major',articulation:{type:'block'}}},pad_help:[{column:1,row:0,title:'Clip 1',action:'Recorded (blue). Press to launch.'}]}};
snapshot.firmware.surface.notes[8]=72; // Firmware-transposed C5, not a default grid assumption.
snapshot.firmware.surface.highlights[8]='accent';
snapshot.firmware.leds[8]=[0,0,241];
const original=JSON.stringify(snapshot);
assert.equal(describePad(snapshot,1,0).title,'C5 · MIDI 72');
assert.match(describePad(snapshot,1,0).action,/accent-note/);
assert.equal(describePad(snapshot,1,0).light,'Blue firmware light');
assert.equal(describePad(snapshot,17,0),null);
assert.equal(describePad(snapshot,1,-1),null);
assert.equal(describePad(snapshot,1.5,0),null);
assert.equal(JSON.stringify(snapshot),original,'inspection never changes performance state');
snapshot.host.performance.mode='chords';
assert.equal(describePad(snapshot,1,0).title,'Chord VI'); // (72 - 60) mod 7.
snapshot.firmware.surface.notes[8]=59;
assert.equal(describePad(snapshot,1,0).title,'Chord VII');
snapshot.host.performance.harmony.playing='strumplate';
assert.equal(activeMode(snapshot),'strumplate');
assert.match(describePad(snapshot,1,0).action,/Separate pad presses/);
snapshot.host.performance.mode='clips';
assert.equal(describePad(snapshot,1,0).title,'Clip 1');
assert.match(describePad(snapshot,1,0).action,/Recorded/);
assert.equal(describePad(snapshot,16,1).title,'Unassigned pad');
snapshot.firmware.surface.normal=false;
assert.equal(describePad(snapshot,1,0).title,'Firmware settings');
snapshot.external_host=true;
assert.match(describePad(snapshot,1,0).action,/external MIDI host/);
assert.equal(performanceSummary(snapshot),'External host controls the performance');
snapshot.connected=false;
assert.equal(describePad(snapshot,1,0).title,'Device disconnected');
snapshot.firmware.model=200;
assert.notEqual(describePad(snapshot,25,7),null);
assert.equal(describePad(snapshot,26,7),null);
console.log('Surface descriptions follow firmware pitch, host assignments, geometry and connection state without mutation.');

for (const [columns,start] of [[17,10],[26,14]]) {
  const mapped=layoutBindings(defaultBindings(),true,columns);
  assert.deepEqual(mapped.pads.a,{x:start,y:4});
  assert.deepEqual(mapped.pads.j,{x:start+6,y:4});
  assert.deepEqual(mapped.pads.k,{x:start,y:5});
  assert.deepEqual(layoutBindings(defaultBindings(),false,columns),defaultBindings());
  const remapped={...defaultBindings(),pads:{a:{x:10,y:6}}};
  assert.equal(layoutBindings(remapped,true,columns),remapped,'explicit physical map is never overwritten by mode defaults');
}
snapshot.connected=true;snapshot.external_host=false;snapshot.firmware.surface.normal=true;
snapshot.host.performance.mode='chords';snapshot.host.performance.harmony.input_octaves=true;
snapshot.host.pad_help=[{column:14,row:5,title:'Chord I · C4',action:'Press this degree.',label:'I 4'}];
assert.equal(describePad(snapshot,14,5).title,'Chord I · C4');
assert.equal(describePad(snapshot,5,5).title,'Unassigned pad');
console.log('Dedicated chord help and mode-aware default keys preserve custom coordinate mappings.');

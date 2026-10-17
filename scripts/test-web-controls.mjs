import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const source=readFileSync(new URL('../crates/linnstrument-sim/web/keyboard.js',import.meta.url),'utf8');
const {parseBindings,defaultBindings}=await import(`data:text/javascript,${encodeURIComponent(source)}`);
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
dispatch(button,'keyup',{key:'Enter'});
assert.deepEqual(changes,[true,false,true,false,true,false]);
button.disabled=true;
dispatch(button,'pointerdown',{button:0,pointerId:3});
dispatch(button,'keydown',{key:' ',repeat:false});
assert.equal(changes.length,6);
console.log('Momentary Fill preserves overlapping holds and releases on focus/capture loss.');

// Pure validation shared by the instrument and headless regression checks.
export function defaultBindings() {
  return {pads:Object.fromEntries([... 'asdfghjk'].map((key,index)=>[key,{x:3+index,y:2}])),pressure_down:'[',pressure_up:']'};
}
export function parseBindings(text,columns) {
  if(text.length>4096)throw new Error('Keyboard settings exceed 4096 characters.');
  const value=JSON.parse(text);
  if(!value||typeof value!=='object'||Array.isArray(value)||Object.keys(value).sort().join(',')!=='pads,pressure_down,pressure_up')throw new Error('Expected pads, pressure_down and pressure_up.');
  if(!value.pads||typeof value.pads!=='object'||Array.isArray(value.pads))throw new Error('Pads must map keys to {x,y} contacts.');
  const entries=Object.entries(value.pads);
  if(!entries.length||entries.length>32)throw new Error('Assign between 1 and 32 pad keys.');
  for(const [key,position]of entries){
    if(!/^[a-z0-9]$/.test(key))throw new Error('Pad keys must be lowercase letters or digits.');
    if(!position||typeof position!=='object'||Array.isArray(position)||Object.keys(position).sort().join(',')!=='x,y')throw new Error('Each pad needs exactly x and y.');
    if(!Number.isFinite(position.x)||!Number.isFinite(position.y)||position.x<0||position.x>columns-1||position.y<0||position.y>7)throw new Error('Pad coordinates exceed this instrument.');
  }
  for(const key of [value.pressure_down,value.pressure_up])if(typeof key!=='string'||!/^[a-z0-9\[\]\-+=;,./]$/.test(key)||Object.hasOwn(value.pads,key))throw new Error('Pressure keys must be distinct printable keys, separate from pads.');
  if(value.pressure_down===value.pressure_up)throw new Error('Pressure keys must differ.');
  return value;
}

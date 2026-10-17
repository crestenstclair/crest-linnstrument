// One momentary control may have a pointer and keyboard held concurrently.
// Releasing either source leaves the other active; losing focus releases both.
export function bindHoldControl(button, window, change) {
  const sources=new Set();
  // WebKit on macOS does not focus buttons after a mouse/AX click by default.
  // Keep the advertised click-then-Space keyboard path available there too.
  button.addEventListener('click',()=>button.focus());
  function update(source,held) {
    const previous=sources.size>0;
    if(held)sources.add(source);else sources.delete(source);
    if(previous!==(sources.size>0))change(sources.size>0);
  }
  function clear() {
    if(!sources.size)return;
    sources.clear();change(false);
  }
  button.addEventListener('pointerdown',event=>{
    if(event.button!==0||button.disabled)return;
    button.setPointerCapture(event.pointerId);
    update(`pointer:${event.pointerId}`,true);
  });
  for(const type of ['pointerup','pointercancel','lostpointercapture'])
    button.addEventListener(type,event=>update(`pointer:${event.pointerId}`,false));
  button.addEventListener('keydown',event=>{
    if(![' ','Enter'].includes(event.key)||button.disabled)return;
    event.preventDefault();
    if(!event.repeat)update(`key:${event.key}`,true);
  });
  button.addEventListener('keyup',event=>{
    if(![' ','Enter'].includes(event.key))return;
    event.preventDefault();update(`key:${event.key}`,false);
  });
  button.addEventListener('blur',clear);
  window.addEventListener('blur',clear);
  return clear;
}

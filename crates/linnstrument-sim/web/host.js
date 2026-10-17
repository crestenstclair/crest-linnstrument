import {bindHoldControl} from './hold-control.js';
'use strict';let selected=0;let selectedNote=null;let displayedNotes=[];
const fillButton=document.querySelector('#fill');
bindHoldControl(fillButton,window,held=>send({type:'fill',held}));
const muteButton=document.querySelector('#mute');let heldMuteSlot=null;
const clearMute=bindHoldControl(muteButton,window,held=>{
  if(held){heldMuteSlot=selected;send({type:'clip_muted',slot:heldMuteSlot,muted:true});}
  else if(heldMuteSlot!==null){const slot=heldMuteSlot;heldMuteSlot=null;send({type:'clip_muted',slot,muted:false});}
});
function renderMute(state){
  muteButton.disabled=!['playing','queued'].includes(state.slots[selected]);
  if(muteButton.disabled)clearMute();
  const muted=state.muted_slots[selected];
  muteButton.setAttribute('aria-pressed',String(muted));
  muteButton.textContent=muted?'Selected clip muted':'Hold mute selected';
}
const conditionForm=document.querySelector('#condition-form');
conditionForm.onsubmit=event=>{
  event.preventDefault();if(!selectedNote||!conditionForm.reportValidity())return;
  const fields=new FormData(conditionForm),type=fields.get('condition');
  const condition=type==='every'?{type,period:Number(fields.get('period')),offset:Number(fields.get('iteration'))-1}:{type};
  send({type:'note_condition',slot:selected,note:selectedNote,condition});
};
function selectNote(note){
  selectedNote=note.id;
  document.querySelector('#note-key').value=note.key;
  document.querySelector('#note-start').value=note.start;
  document.querySelector('#note-duration').value=note.duration;
  conditionForm.elements.condition.value=note.condition.type;
  if(note.condition.type==='every'){
    conditionForm.elements.period.value=note.condition.period;
    conditionForm.elements.iteration.value=note.condition.offset+1;
  }
}
for(const [id,direction] of [['previous-note',-1],['next-note',1]])
  document.getElementById(id).onclick=()=>{
    if(!displayedNotes.length)return;
    const index=displayedNotes.findIndex(note=>noteKey(note.id)===noteKey(selectedNote));
    selectNote(displayedNotes[(index<0?0:index+direction+displayedNotes.length)%displayedNotes.length]);
  };
const commands={play:{type:'mode',clips:false},clips:{type:'mode',clips:true},chords:{type:'surface',mode:'chords'},finish:{type:'finish_record'},cancel:{type:'cancel_record'},stop:{type:'stop'}};
for(const [id,command]of Object.entries(commands))document.getElementById(id).onclick=()=>send(command);
document.querySelector('#stop-slot').onclick=()=>send({type:'stop_slot',slot:selected});
document.querySelector('#record').onclick=()=>send({type:'begin_record',slot:selected});
document.querySelector('#launch-quantum').onchange=event=>send({type:'launch_quantum',ticks:Number(event.target.value)});
for(const type of ['save','open'])document.getElementById(type).onclick=()=>send({type,path:document.querySelector('#path').value});
document.querySelector('#overdub').onclick=()=>send({type:'begin_overdub',slot:selected});
document.querySelector('#retro').onclick=()=>send({type:'capture_recent',slot:selected,ticks:3840000});
document.querySelector('#undo').onclick=()=>send({type:'undo'});
document.querySelector('#duplicate').onclick=()=>send({type:'duplicate',from:selected,to:(selected+1)%8});
document.querySelector('#apply-edit').onclick=()=>{if(selectedNote)send({type:'edit_note',slot:selected,note:selectedNote,start:Number(document.querySelector('#note-start').value),duration:Number(document.querySelector('#note-duration').value),key:Number(document.querySelector('#note-key').value)});};
const piano=document.querySelector('#piano'),pencil=piano.getContext('2d');
function noteKey(note){return JSON.stringify(note);}
function renderNotes(clip){
  pencil.clearRect(0,0,800,140);displayedNotes=[];
  document.querySelector('#condition-state').textContent='Select a note using the canvas or Previous / Next.';
  if(!clip)return;
  const active=new Map();for(const event of clip.events){const id=noteKey(event.note);if(event.gesture.type==='on')active.set(id,event);if(event.gesture.type==='off'&&active.has(id)){const on=active.get(id);displayedNotes.push({id:on.note,key:on.gesture.key,start:on.at,duration:event.at-on.at});active.delete(id);}}
  for(const note of displayedNotes)note.condition=clip.conditions?.find(entry=>noteKey(entry.note)===noteKey(note.id))?.condition??{type:'always'};
  const current=displayedNotes.find(note=>noteKey(note.id)===noteKey(selectedNote));
  document.querySelector('#condition-state').textContent=current?`Note ${current.key}: ${current.condition.type==='every'?`loop ${current.condition.offset+1} of ${current.condition.period}`:current.condition.type.replace('_',' ')}`:'Select a note using the canvas or Previous / Next.';
  if(!displayedNotes.length)return;
  const low=Math.min(...displayedNotes.map(n=>n.key))-1,high=Math.max(...displayedNotes.map(n=>n.key))+2;
  for(const note of displayedNotes){note.rect={x:note.start/clip.length*800,y:140-(note.key-low+1)/(high-low)*140,w:Math.max(3,note.duration/clip.length*800),h:Math.min(18,140/(high-low)-2)};const r=note.rect;pencil.fillStyle=noteKey(selectedNote)===noteKey(note.id)?'#f2dec0':'#b5db83';pencil.fillRect(r.x,r.y,r.w,r.h);}
}
piano.onclick=event=>{const box=piano.getBoundingClientRect(),x=(event.clientX-box.left)/box.width*800,y=(event.clientY-box.top)/box.height*140;const note=displayedNotes.find(n=>x>=n.rect.x&&x<=n.rect.x+n.rect.w&&y>=n.rect.y&&y<=n.rect.y+n.rect.h);if(note)selectNote(note);};
const sourceSelect=document.querySelector('#harmony-source');let sourceSlots=-1;
sourceSelect.onchange=()=>send({type:'harmony_source',source:sourceSelect.value==='selected'?{type:'selected'}:{type:'clip',slot:Number(sourceSelect.value)}});
function renderHarmonySource(state){
  if(sourceSlots!==state.slots.length){
    sourceSelect.replaceChildren(new Option('Live / selected chord','selected'),...state.slots.map((_,slot)=>new Option(`Clip ${String(slot+1).padStart(2,'0')}`,String(slot))));
    sourceSlots=state.slots.length;
  }
  sourceSelect.value=state.harmony_source.type==='clip'?String(state.harmony_source.slot):'selected';
  const current=state.effective_harmony;
  document.querySelector('#harmony-source-state').textContent=`Follower harmony: ${['I','II','III','IV','V','VI','VII'][current.degree]} · tonic MIDI ${current.tonic}`;
}
const harmonyForm=document.querySelector('#harmony-form');let harmonySettings=null;let renderedHarmony='';let renderedTracks='';
function sendHarmony(degree){
  harmonyForm.elements.arpeggio_gate.setCustomValidity('');
  if(!harmonySettings||!harmonyForm.reportValidity())return;
  const fields=new FormData(harmonyForm), settings={...harmonySettings,degree:degree??harmonySettings.degree};
  for(const name of ['tonic','inversion','register','input_anchor'])settings[name]=Number(fields.get(name));
  for(const name of ['scale','playing','extension','suspension','expression'])settings[name]=fields.get(name);
  for(const name of ['borrowed','spread','voice_leading'])settings[name]=fields.has(name);
  settings.bass_track=fields.get('bass_track')===''?null:Number(fields.get('bass_track'));
  const articulation=fields.get('articulation');
  if(articulation==='block')settings.articulation={type:'block'};
  else if(articulation==='humanized')settings.articulation={type:'humanized',max_delay:Number(fields.get('humanize_delay')),velocity_variation:Number(fields.get('humanize_velocity')),seed:Number(fields.get('humanize_seed'))};
  else if(articulation.startsWith('arpeggio_'))settings.articulation={type:'arpeggio',direction:articulation.slice(9),interval:Number(fields.get('strum_interval')),gate:Number(fields.get('arpeggio_gate'))};
  else settings.articulation={type:'strum',direction:articulation,interval:Number(fields.get('strum_interval'))};
  const gate=harmonyForm.elements.arpeggio_gate;
  gate.setCustomValidity(settings.articulation.type==='arpeggio'&&settings.articulation.gate>settings.articulation.interval?'Gate must not exceed step spacing.':'');
  if(!harmonyForm.reportValidity())return;
  send({type:'harmony',settings});
}
harmonyForm.addEventListener('input',()=>harmonyForm.elements.arpeggio_gate.setCustomValidity(''));
harmonyForm.onsubmit=event=>{event.preventDefault();sendHarmony();};
const degreeButtons=['I','II','III','IV','V','VI','VII'].map((label,degree)=>{const button=document.createElement('button');button.type='button';button.textContent=label;button.onclick=()=>sendHarmony(degree);return button;});
document.querySelector('#degrees').replaceChildren(...degreeButtons);
document.querySelector('#learn-chord').onclick=()=>{
  const root=document.querySelector('#learned-root');
  if(!harmonySettings||!root.reportValidity())return;
  send({type:'learn_chord',slot:selected,degree:harmonySettings.degree,root:root.value===''?null:Number(root.value)});
};
document.querySelector('#clear-chord').onclick=()=>{if(harmonySettings)send({type:'clear_learned_chord',degree:harmonySettings.degree});};
function showBeatDuration(select,ticks){
  const value=String(ticks);
  for(const option of [...select.options])if(option.dataset.session)option.remove();
  if(![...select.options].some(option=>option.value===value)){const option=new Option(`${value}/960000 beat (session)`,value);option.dataset.session='true';select.add(option);}
  select.value=value;
}
function renderHarmony(state,tracks){
  harmonySettings=state.harmony;
  const trackKey=JSON.stringify(tracks);
  if(trackKey!==renderedTracks){const select=harmonyForm.elements.bass_track;select.replaceChildren(new Option('Off',''),...tracks.map(t=>new Option(t.name,String(t.id))));renderedTracks=trackKey;}
  const key=JSON.stringify({...state.harmony,degree:0});
  if(key!==renderedHarmony){
    for(const [name,value]of Object.entries(state.harmony)){const field=harmonyForm.elements[name];if(!field)continue;if(field.type==='checkbox')field.checked=value;else field.value=value??'';}
    const articulation=state.harmony.articulation;
    harmonyForm.elements.articulation.value=['block','humanized'].includes(articulation.type)?articulation.type:articulation.type==='arpeggio'?`arpeggio_${articulation.direction}`:articulation.direction;
    if(['strum','arpeggio'].includes(articulation.type))showBeatDuration(harmonyForm.elements.strum_interval,articulation.interval);
    if(articulation.type==='arpeggio')showBeatDuration(harmonyForm.elements.arpeggio_gate,articulation.gate);
    if(articulation.type==='humanized'){
      harmonyForm.elements.humanize_delay.value=articulation.max_delay;
      harmonyForm.elements.humanize_velocity.value=articulation.velocity_variation;
      harmonyForm.elements.humanize_seed.value=articulation.seed;
    }
    renderedHarmony=key;
  }
  for(const [degree,button]of degreeButtons.entries()){
    button.setAttribute('aria-pressed',String(degree===state.harmony.degree));
    button.textContent=['I','II','III','IV','V','VI','VII'][degree]+(state.harmony.learned[degree]?' · learned':'');
  }
  const learned=state.harmony.learned[state.harmony.degree];
  document.querySelector('#learned-state').textContent=learned?`Learned pitches: ${learned.notes.join(' · ')} · root ${learned.root}`:'Selected degree uses diatonic harmony.';
  document.querySelector('#learn-chord').disabled=state.mode!=='play';
  document.querySelector('#clear-chord').disabled=!learned;
  const notes=state.last_voicing?state.last_voicing.notes.slice(0,state.last_voicing.count).join(' · '):'Play a chord';
  document.querySelector('#voicing').textContent=`Last rendered chord: ${notes} · selected degree ${state.harmony.degree+1}`;
}
const patternForm=document.querySelector('#pattern-form');
patternForm.onsubmit=event=>{event.preventDefault();if(!patternForm.reportValidity())return;const fields=new FormData(patternForm),settings={};for(const [name,value]of fields)settings[name]=name==='follow_harmony'?true:Number(value);settings.follow_harmony=fields.has('follow_harmony');send({type:'generate',slot:selected,settings});};
for(const [id,command]of Object.entries({'follow-on':{type:'follow_harmony',enabled:true},'follow-off':{type:'follow_harmony',enabled:false},freeze:{type:'freeze_harmony'}}))document.getElementById(id).onclick=()=>send({...command,slot:selected});
const outputForm=document.querySelector('#output-form');let outputRoutes=[];let outputKey='';let outputDestinations='';
function displayOutput(){const route=outputRoutes.find(r=>String(r.destination)===outputForm.elements.destination.value);if(!route)return;const p=route.profile;outputForm.elements.profile.value=p.type;outputForm.elements.channel.value=(p.channel??p.main??0)+1;outputForm.elements.first.value=(p.first??1)+1;outputForm.elements.last.value=(p.last??15)+1;outputForm.elements.bend.value=p.bend_semitones??48;outputForm.elements.voices.value=p.voices??15;outputForm.elements.expression.value=p.expression??'reject_unsupported';}
outputForm.elements.destination.onchange=displayOutput;
outputForm.onsubmit=event=>{event.preventDefault();if(!outputForm.reportValidity())return;const fields=new FormData(outputForm),channel=Number(fields.get('channel'))-1,type=fields.get('profile');let profile;if(type==='mpe')profile={type,main:channel,first:Number(fields.get('first'))-1,last:Number(fields.get('last'))-1,bend_semitones:Number(fields.get('bend'))};else if(type==='polyphonic')profile={type,channel,voices:Number(fields.get('voices')),expression:fields.get('expression')};else profile={type,channel,bend_semitones:Number(fields.get('bend'))};send({type:'output_profile',destination:Number(fields.get('destination')),profile});};
function renderOutputs(host){outputRoutes=host.outputs;const destinations=JSON.stringify(outputRoutes.map(r=>r.destination));if(destinations!==outputDestinations){outputForm.elements.destination.replaceChildren(...outputRoutes.map(r=>new Option(String(r.destination),String(r.destination))));outputDestinations=destinations;}
  const key=JSON.stringify(outputRoutes);if(key!==outputKey){displayOutput();outputKey=key;}
  const route=outputRoutes.find(r=>String(r.destination)===outputForm.elements.destination.value);if(!route)return;const c=route.capabilities;document.querySelector('#output-state').textContent=`Destination ${route.destination}: ${c.sounding_voices} sounding voices · ${c.held_notes} held notes · independent pitch ${c.independent_pitch?'yes':'no'} / timbre ${c.independent_timbre?'yes':'no'} / pressure ${c.independent_pressure?'yes':'no'} · host total voice budget ${host.performance.voice_capacity}`;
}
let clipButtons=[];let currentStates=[];
function renderClips(states,muted){
  currentStates=states;
  if(clipButtons.length!==states.length){
    clipButtons=states.map((_,slot)=>{const button=document.createElement('button');const label=document.createElement('strong'),status=document.createElement('span');button.append(label,status);button.onclick=()=>{if(selected!==slot){selected=slot;selectedNote=null;return;}const state=currentStates[slot];if(state==='recorded')send({type:'launch',slot});else if(state==='playing'||state==='queued')send({type:'stop_slot',slot});};return button;});
    document.querySelector('#grid').replaceChildren(...clipButtons);
  }
  states.forEach((state,slot)=>{const button=clipButtons[slot];button.className=`clip ${state}`;button.setAttribute('aria-pressed',String(selected===slot));const label=`${String(slot+1).padStart(2,'0')}${selected===slot?' ·':''}`;if(button.firstChild.textContent!==label)button.firstChild.textContent=label;const status=state.toUpperCase()+(muted[slot]?' · MUTED':'');if(button.lastChild.textContent!==status)button.lastChild.textContent=status;});
}
subscribers.push(snapshot=>{
  if(snapshot.external_host){document.querySelector('#transport').textContent='OS MIDI DEVICE MODE · Connect external host to Simulator performance / feedback ports';for(const element of document.querySelectorAll('button,input,select'))element.disabled=true;return;}
  const state=snapshot.host.performance;
  fillButton.setAttribute('aria-pressed',String(state.fill));
  fillButton.textContent=state.fill?'Fill held':'Hold Fill';
  document.querySelector('#launch-quantum').value=String(snapshot.host.launch_quantum);
  renderNotes(snapshot.host.clips[selected]);
  renderHarmony(state,snapshot.host.tracks);
  renderHarmonySource(state);
  renderOutputs(snapshot.host);
  const selectedClip=snapshot.host.clips[selected],origin=selectedClip?.pattern_origin?.at(-1);
  document.querySelector('#pattern-state').textContent=selectedClip?`${selectedClip.follow?'FOLLOWING SELECTED/LIVE CHORD':'FIXED PITCHES'}${origin?` · seed ${origin.settings.seed} · ${origin.settings.pulses}/${origin.settings.steps} pulses · algorithm ${origin.algorithm_version}`:''}`:'Select an empty or stopped clip slot.';
  renderClips(state.slots,state.muted_slots);
  renderMute(state);
  document.querySelector('#transport').textContent=`${(state.at/960000).toFixed(3)} beats · 120 BPM · ${state.active_voices} voices · ${snapshot.host.capture_count} captured MIDI messages · ${state.mode.toUpperCase()}`;
  document.querySelector('#capture').textContent=snapshot.host.recent_midi.map(m=>`${String(m.at).padStart(10)}  ${m.message.bytes.slice(0,m.message.len).map(b=>b.toString(16).padStart(2,'0')).join(' ')}`).join('\n');
});

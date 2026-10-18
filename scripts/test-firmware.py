#!/usr/bin/env python3
"""Behavior assertions through sensor SPI and upstream firmware, never MIDI injection."""
import json
import pathlib
import subprocess
import tempfile

ROOT=pathlib.Path(__file__).resolve().parents[1]
class Device:
    def __init__(self, model=200, start_us=None, flash=None):
        self.process=subprocess.Popen([str(ROOT/'target/firmware-native/linnstrument-worker'),str(model),str(flash or ROOT/f'target/firmware-native/ideal-{model}.bin')]+([] if start_us is None else ['--start-us',str(start_us)]),stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
        self.trace=[json.loads(self.process.stdout.readline())]
        self.status=0; self.data=[]; self.states={}; self.notes={}
        self.send('advance 1000000')
    def send(self, command):
        self.process.stdin.write(command+'\n'); self.process.stdin.flush()
        line=self.process.stdout.readline()
        assert line, f'worker exited ({self.process.wait(timeout=5)}) on {command}; previous command: {self.trace[-1].get("command", "boot")}'
        snapshot=json.loads(line); self.trace.append({'command':command,'snapshot':snapshot})
        messages=self.parse(snapshot['midi'])
        return snapshot,messages
    def parse(self, raw):
        messages=[]
        for _,byte in raw:
            if byte>=0xf8: continue
            if byte&0x80: self.status=byte; self.data=[]; continue
            self.data.append(byte)
            needed=1 if self.status&0xf0 in [0xc0,0xd0] else 2
            if len(self.data)<needed: continue
            message=[self.status,*self.data]; self.data=[]; messages.append(message)
            channel=self.status&15; kind=self.status&0xf0
            if kind==0x90 and message[2]: self.notes[channel]=message[1]
            if kind==0x80 or kind==0x90 and not message[2]: self.notes.pop(channel,None)
            if kind==0xe0: self.states[channel,'x']=message[1]+(message[2]<<7)
            if kind==0xb0 and message[1]==74: self.states[channel,'y']=message[2]
            if kind==0xd0: self.states[channel,'z']=message[1]
        return messages
    def close(self):
        self.process.stdin.close(); assert self.process.wait(timeout=5)==0

def independent_expression(model):
    device=Device(model)
    for command in ['contact 1 4 2 0.5','contact 2 9 3 0.5','contact 3 14 4 0.5','advance 300000','advance 300000']:
        device.send(command)
    assert len(device.notes)==3, device.notes
    # Default row-fourths layout: row 0 starts at MIDI 30; column 1 is root.
    surface=device.trace[-1]['snapshot']['surface']
    assert surface['normal'] and not surface['user_mode']
    assert len(surface['notes'])==(17 if model==128 else 26)*8
    assert len(surface['highlights'])==len(surface['notes'])
    assert surface['notes'][:8]==[None]*8
    assert [surface['notes'][c*8+r] for c,r in [(4,2),(9,3),(14,4)]]==[43,53,63]
    expected={43,53,63}
    assert set(device.notes.values())==expected, device.notes
    channels={note:channel for channel,note in device.notes.items()}
    assert all(channel!=0 for channel in channels.values())
    tests=[('contact 1 4.25 2 0.5',channels[43],'x',20),('contact 2 9 3.3 0.5',channels[53],'y',10),('contact 3 14 4 0.7',channels[63],'z',5)]
    for command,channel,dimension,minimum in tests:
        before=dict(device.states)
        device.send(command); device.send('advance 200000')
        after=device.states
        assert after[channel,dimension]-before[channel,dimension]>=minimum, (command,before,after)
        for other in channels.values():
            for axis in ['x','y','z']:
                if (other,axis)!=(channel,dimension):
                    assert abs(after.get((other,axis),0)-before.get((other,axis),0))<=1,(command,other,axis,before,after)
    device.send('release_all'); device.send('advance 200000'); assert not device.notes,device.notes
    # User mode and LED round trip must traverse firmware's MIDI parser and SPI.
    device.send('midi 176 99 1 176 98 117 176 6 0 176 38 1'); device.send('advance 200000')
    device.send('midi 176 20 5 176 21 2 176 22 1'); frame,_=device.send('advance 100000')
    assert frame['surface']['normal'] and frame['surface']['user_mode']
    assert all(note is None for note in frame['surface']['notes'])
    red=frame['leds'][5*8+2]; assert red[0]>200 and red[1:]==[0,0],red
    device.send('contact 9 5 2 0.5'); _,messages=device.send('advance 200000')
    assert any(m[0]==0x92 and m[1]==5 and m[2]>0 for m in messages),messages
    device.send('release_all'); device.send('advance 200000')
    device.send('midi 176 99 1 176 98 117 176 6 0 176 38 0'); device.send('advance 200000')
    device.send('contact 10 4 2 0.5'); device.send('advance 200000'); assert set(device.notes.values())=={43},device.notes
    device.send('release_all'); final,_=device.send('advance 200000'); assert not device.notes
    assert final['sensor_reads']>10000 and final['sensor_selects']>10000 and final['led_writes']>1000
    device.close(); return device.trace

def slide_and_wrap():
    for start in [None, 0xffff0000]:
        device=Device(200,start)
        device.send('contact 1 4 2 0.5');device.send('advance 200000')
        assert set(device.notes.values())=={43}
        channel=next(iter(device.notes))
        for step in range(1,21):
            device.send(f'contact 1 {4+step/10} 2.2 0.55')
            _,messages=device.send('advance 30000')
            assert not any(m[0]&0xf0 in [0x80,0x90] for m in messages),messages
        assert set(device.notes.values())=={43}
        # Two semitones at +/-48 bend range: 2/48*8192, within filtering/ADC tolerance.
        assert abs(device.states[channel,'x']-(8192+2*8192/48))<20
        device.send('release_all');device.send('advance 200000');assert not device.notes
        device.close()
    print('continuous two-cell slide and 32-bit microsecond wrap PASS')


def nrpn(device, parameter, value):
    data=[176,99,parameter>>7,176,98,parameter&127,176,6,value>>7,176,38,value&127,176,101,127,176,100,127]
    device.send('midi '+' '.join(map(str,data)))
    device.send('advance 100000')

def capacity_pedals_and_persistence(model):
    device=Device(model)
    attacks=[]
    # Distinct columns avoid the stock sensor ghost-rectangle rejection rule.
    for finger in range(16):
        device.send(f'contact {finger} {finger+1} {finger%4*2} 0.5')
        _,messages=device.send('advance 150000')
        attacks.extend(m for m in messages if m[0]&0xf0==0x90 and m[2]>0)
        assert not any(m[0]&0xf0==0x80 for m in messages)
    assert len(attacks)==16
    assert [m[0]&15 for m in attacks]==list(range(1,16))+[1]
    # Upstream rotates back onto channel 2 at exhaustion without stealing an old
    # note. This is intentionally distinct from the library's rejection policy.
    device.send('release_all');_,messages=device.send('advance 300000')
    releases=[m for m in messages if m[0]&0xf0==0x80]
    assert sorted((m[0]&15,m[1]) for m in releases)==sorted((m[0]&15,m[1]) for m in attacks)
    device.send('switch 2 1');_,messages=device.send('advance 200000')
    assert [176,64,127] in messages
    device.send('switch 2 0');_,messages=device.send('advance 200000')
    assert [176,64,0] in messages
    # Persist a semitone-independent octave change through upstream storeSettings
    # (CC23 stores the custom pattern and all settings), then boot that flash.
    nrpn(device,36,6)
    device.send('midi 176 23 0');device.send('advance 200000')
    with tempfile.TemporaryDirectory(prefix='crest-flash-') as directory:
        flash=pathlib.Path(directory)/'settings.bin'
        device.send(f'save {flash}');device.close()
        restored=Device(model,flash=flash)
        restored.send('contact 1 4 2 0.5');restored.send('advance 200000')
        assert set(restored.notes.values())=={55},restored.notes
        restored.send('release_all');restored.send('advance 200000');restored.close()
    erased=Device(model,flash='erased')
    frame,_=erased.send('advance 100000')
    assert frame['sensor_reads']>0 and frame['led_writes']>0
    erased.close()
    print(f'{model}: capacity/reuse, foot-switch sustain, persisted octave and erased-flash boot PASS')

def concurrent_expression(model):
    device=Device(model)
    positions=[(4,2),(9,3),(14,4)]
    for finger,(x,y) in enumerate(positions): device.send(f'contact {finger} {x} {y} 0.5')
    device.send('advance 300000')
    owners=dict(device.notes);before=dict(device.states)
    for step in range(1,11):
        for finger,(x,y) in enumerate(positions):
            device.send(f'contact {finger} {x+step*.025} {y+step*.025} {0.5+step*.02}')
        _,messages=device.send('advance 50000')
        assert not any(m[0]&0xf0 in [0x80,0x90] for m in messages),messages
    assert owners==device.notes
    for channel in owners:
        for axis,minimum in [('x',20),('y',10),('z',5)]:
            assert device.states[channel,axis]-before[channel,axis]>=minimum
    device.send('release_all');device.send('advance 200000');assert not device.notes
    device.close()
    print(f'{model}: all three contacts moving X/Y/Z concurrently PASS')

def right_edge_expression(model):
    device=Device(model)
    column=16 if model==128 else 25
    # The last calibration sector has no right neighbor. Its interpolation
    # weight is zero, so Y must still span its left sector without division UB.
    device.send(f'contact 1 {column} 3 0.5');device.send('advance 300000')
    assert set(device.notes.values())=={30+column-1+3*5},device.notes
    channel=next(iter(device.notes))
    before=dict(device.states)
    assert abs(before[channel,'y']-64)<=2,before
    device.send(f'contact 1 {column} 3.25 0.5');device.send('advance 200000')
    assert abs(device.states[channel,'y']-95)<=3,device.states
    for axis in ['x','z']:
        assert abs(device.states[channel,axis]-before[channel,axis])<=1
    device.send('release_all');device.send('advance 200000');assert not device.notes
    device.close()
    print(f'{model}: right-edge calibrated timbre and independent X/Z PASS')

def control_column_octave(model):
    device=Device(model)
    # Short control tap latches the firmware's own octave settings screen.
    device.send('contact 1 0 4 0.5');frame,_=device.send('advance 200000')
    assert frame['leds'][4][2]>200
    assert not frame['surface']['normal']
    assert all(note is None for note in frame['surface']['notes'])
    device.send('release 1');device.send('advance 100000')
    device.send('contact 2 9 4 0.5');device.send('advance 150000')  # +12
    device.send('release 2');device.send('advance 100000')
    assert not device.notes,device.notes
    device.send('contact 3 0 4 0.5');device.send('advance 200000')
    device.send('release 3');frame,_=device.send('advance 100000')
    assert frame['leds'][4]==[0,0,0]
    assert frame['surface']['normal'] and frame['surface']['notes'][4*8+2]==55
    device.send('contact 4 4 2 0.5');device.send('advance 200000')
    assert set(device.notes.values())=={55},device.notes
    device.send('release 4');device.send('advance 100000')
    # Holding the control for >500 ms instead returns to Play on release.
    device.send('contact 5 0 4 0.5');device.send('advance 600000')
    device.send('contact 6 8 4 0.5');device.send('advance 150000')  # octave zero
    device.send('release_all');device.send('advance 200000')
    frame,_=device.send('advance 50000')  # fresh multiplexed frame after release
    assert frame['leds'][4]==[0,0,0]
    device.send('contact 7 4 2 0.5');device.send('advance 200000')
    assert set(device.notes.values())=={43},device.notes
    device.send('release_all');device.send('advance 200000');assert not device.notes
    device.close()
    print(f'{model}: control-column short tap, octave selection and timed hold PASS')

def split_mpe_profile(model):
    device=Device(model)
    # Standard RPN: lower seven members, upper seven members, +/-24 semitones.
    for channel,parameter,value in [(0,6,7),(15,6,7),(0,0,24)]:
        status=176+channel
        data=[status,101,parameter>>7,status,100,parameter&127,status,6,value,
              status,38,0,status,101,127,status,100,127]
        device.send('midi '+' '.join(map(str,data)));device.send('advance 100000')
    nrpn(device,202,9);nrpn(device,200,1)
    device.send('contact 1 4 2 0.5');device.send('contact 2 14 4 0.5')
    device.send('advance 300000')
    assert set(device.notes.values())=={43,63},device.notes
    channels={note:channel for channel,note in device.notes.items()}
    left,right=channels[43],channels[63]
    assert 1<=left<=7 and 8<=right<=14,channels
    before=dict(device.states)
    device.send('contact 1 4.25 2 0.5');device.send('contact 2 14 4.3 0.5')
    device.send('advance 300000')
    assert abs(device.states[left,'x']-(8192+.25*8192/24))<5
    assert device.states[right,'y']-before[right,'y']>=30
    for channel,axis in [(left,'y'),(left,'z'),(right,'x'),(right,'z')]:
        assert abs(device.states[channel,axis]-before[channel,axis])<=1
    device.send('release_all');device.send('advance 200000');assert not device.notes
    device.close()
    print(f'{model}: lower/upper MPE splits, main/member separation and +/-24 bend PASS')

def repeated_pitch_and_vibrato(model):
    device=Device(model)
    # Row-fourths layout: these separate pads both play MIDI 43.
    device.send('contact 1 4 2 0.5');device.send('advance 200000')
    first=next(iter(device.notes))
    device.send('contact 2 9 1 0.5');device.send('advance 200000')
    assert list(device.notes.values())==[43,43],device.notes
    second=next(channel for channel in device.notes if channel!=first)
    before=dict(device.states);bends=[]
    for offset in [-.05,0,.05,0]*3:
        device.send(f'contact 1 {4+offset} 2 0.5')
        _,messages=device.send('advance 100000')
        assert not any(m[0]&0xf0 in [0x80,0x90] for m in messages)
        bends.append(device.states[first,'x'])
        for axis in ['x','y','z']:
            assert abs(device.states[second,axis]-before[second,axis])<=1
    assert max(bends)-min(bends)>=10,bends
    device.send('release 1');_,messages=device.send('advance 200000')
    assert device.notes=={second:43},device.notes
    assert any(m[0]==0x80+first and m[1]==43 for m in messages),messages
    device.send('contact 2 9 1 0.75');device.send('advance 200000')
    assert device.states[second,'z']>before[second,'z']+5
    device.send('release 2');device.send('advance 200000');assert not device.notes
    device.close()
    print(f'{model}: repeated-pitch ownership, small vibrato and independent release PASS')

if __name__=='__main__':
    slide_and_wrap()
    for model in [128,200]:
        right_edge_expression(model)
        control_column_octave(model)
        split_mpe_profile(model)
        repeated_pitch_and_vibrato(model)
        capacity_pedals_and_persistence(model)
        concurrent_expression(model)
        first=independent_expression(model); second=independent_expression(model)
        assert first==second,'gesture replay differs'
        path=ROOT/f'target/firmware-native/acceptance-{model}.json'
        path.write_text(json.dumps(first,separators=(',',':'))+'\n')
        print(f'{model}: independent X/Y/Z, stock MPE, user-mode pads, firmware LED round trip, deterministic replay PASS')

#!/usr/bin/env python3
"""Regenerate compile-time native layout checks from the pinned ARM build."""
import json
import pathlib
import re
import subprocess
ROOT=pathlib.Path(__file__).resolve().parents[1]
data=next(item for item in json.loads((ROOT/'target/firmware-arm/compile_commands.json').read_text()) if item['file'].endswith('/linnstrument-firmware.ino.cpp'))
args=data['arguments'];source_path=pathlib.Path(data['file'])
source=(ROOT/'third_party/linnstrument-firmware/linnstrument-firmware.ino').read_text()
types=['Configuration','DeviceSettings','GlobalSettings','SplitSettings','PresetSettings','SequencerProject','TouchInfo','CalibrationSample','CalibrationY']
probes={f'size_{name}':f'sizeof({name})' for name in types}
probes['unsigned_char']='(char(-1)>0?1:0)'
for name in types[:6]:
    body=re.search(r'struct '+name+r' \{(.*?)\n\};',source,re.S).group(1)
    for line in body.splitlines():
        field=re.match(r'(?:[\w]+\s+)+([\w]+)(?:\[[^;]+)?;',line.split('//')[0].strip())
        if field: probes[f'offset_{name}_{field[1]}']=f'__builtin_offsetof({name},{field[1]})'
probe='\n'.join(f'extern "C" {{ char probe_{name}[1+({expression})]; }}' for name,expression in probes.items())
directory=ROOT/'target/arm-layout';directory.mkdir(parents=True,exist_ok=True)
path=directory/'layout.cpp';path.write_text(source_path.read_text()+'\n'+probe)
command=[str(path) if arg==str(source_path) else arg for arg in args]
command[command.index('-o')+1]=str(directory/'layout.o')
command.append('-I'+str(source_path.parent))
subprocess.run(command,check=True)
nm=pathlib.Path(args[0]).with_name('arm-none-eabi-nm')
symbols=subprocess.check_output([str(nm),'-S',str(directory/'layout.o')],text=True)
values={}
for line in symbols.splitlines():
    match=re.match(r'\w+ ([0-9a-f]+) \w probe_(\w+)$',line)
    if match: values[match[2]]=int(match[1],16)-1
assert len(values)==len(probes)
text='// Generated from pinned unmodified ARM sketch using GCC 4.8.3 / SAM 1.6.11.\n'+'\n'.join(f'static_assert(({expression})=={values[name]}, "ARM {name}");' for name,expression in probes.items())+'\n'
expected=ROOT/'firmware/native/arm-layout.inc'
assert expected.read_text()==text,'ARM layout changed; review before regenerating arm-layout.inc'
print(f'{len(values)} ARM/native layout checks verified')

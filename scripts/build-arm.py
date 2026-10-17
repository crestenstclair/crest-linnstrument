#!/usr/bin/env python3
"""Build the pinned, unmodified upstream sketch with its original SAM/GCC pair."""
import hashlib
import pathlib
import platform
import subprocess
import tarfile
import urllib.request
ROOT=pathlib.Path(__file__).resolve().parents[1]
TOOLS=ROOT/'target/tools';TOOLS.mkdir(parents=True,exist_ok=True)
platform_name={('Darwin','arm64'):'macOS_ARM64',('Darwin','x86_64'):'macOS_64bit',('Linux','x86_64'):'Linux_64bit'}.get((platform.system(),platform.machine()))
if not platform_name: raise SystemExit('ARM recipe currently supports Linux x86_64 and macOS')
archive=f'arduino-cli_1.3.1_{platform_name}.tar.gz'
expected=dict(line.split()[::-1] for line in (ROOT/'firmware/arduino-cli-checksums.txt').read_text().splitlines())[archive]
path=TOOLS/archive
if not path.exists(): urllib.request.urlretrieve('https://downloads.arduino.cc/arduino-cli/'+archive,path)
assert hashlib.sha256(path.read_bytes()).hexdigest()==expected,'Arduino CLI checksum mismatch'
with tarfile.open(path) as package:
    member=package.getmember('arduino-cli')
    with (TOOLS/'arduino-cli').open('wb') as output: output.write(package.extractfile(member).read())
(TOOLS/'arduino-cli').chmod(0o755)
config=TOOLS/'arduino-cli.yaml'
config.write_text(f'directories:\n  data: {ROOT}/target/arduino/data\n  downloads: {ROOT}/target/arduino/downloads\n  user: {ROOT}/target/arduino/user\n')
command=[str(TOOLS/'arduino-cli'),'--config-file',str(config)]
for args in [['core','update-index'],['core','install','arduino:sam@1.6.11'],['compile','--fqbn','arduino:sam:arduino_due_x','--libraries',str(ROOT/'third_party/linnstrument-firmware/libraries'),'--build-path',str(ROOT/'target/firmware-arm'),str(ROOT/'third_party/linnstrument-firmware')]]:
    subprocess.run(command+args,check=True,cwd=ROOT)
subprocess.run(['python3',str(ROOT/'scripts/verify-arm-layout.py')],check=True,cwd=ROOT)

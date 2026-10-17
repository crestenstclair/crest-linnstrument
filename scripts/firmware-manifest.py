#!/usr/bin/env python3
import hashlib,json,pathlib,re
root=pathlib.Path(__file__).resolve().parents[1]
header=(root/'target/firmware-native/generated/identity.h').read_text()
identity=dict(re.findall(r'#define (SIM_\w+) "(\w+)"',header))
for model in [128,200]:
    path=root/f'target/firmware-native/ideal-{model}.bin'
    manifest={'version':1,'firmware':'54bb1c9d355b1075b474ade0a0246b80ff7f3524','build_hash':identity['SIM_BUILD_HASH'],'patch_hash':identity['SIM_PATCH_HASH'],'flash_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'model':model,'profile':'lower MPE: main 1, members 2–16, pitch bend +/-48 semitones, CC74, Channel Pressure','calibration':'synthetic ideal upstream defaults; not a measured sensor','seed':1}
    path.with_suffix('.json').write_text(json.dumps(manifest,indent=2)+'\n')

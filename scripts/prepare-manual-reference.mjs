import {readFileSync,writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const source=readFileSync(root+'crates/linnstrument-sim/web/surface-help.js','utf8');
const {describePad,performanceSummary}=await import(`data:text/javascript,${encodeURIComponent(source)}`);
const result={};
for(const mode of ['play','chords','strumplate','clips']){
 const snapshot=JSON.parse(readFileSync(root+`target/manual/surfaces/${mode==='clips'?'clips-playing':mode}.json`));
 const pads=[];
 for(let column=0;column<26;column++)for(let row=0;row<8;row++)pads.push({column,row,rgb:snapshot.firmware.leds[column*8+row],...describePad(snapshot,column,row)});
 result[mode]={summary:performanceSummary(snapshot),firmware:snapshot.firmware.firmware,build_hash:snapshot.firmware.build_hash,pads};
}
writeFileSync(root+'docs/media/surface-reference.json',JSON.stringify(result,null,2)+'\n');

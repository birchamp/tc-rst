// tc4_exe_imports_no_vcruntime (card G3, tC4 #284): the Windows tc4.exe imports no Visual C++
// runtime DLL, so it starts on a computer without the redistributable. Reads the PE import table.
//
//   node pe-imports.mjs <tc4.exe>
import fs from 'node:fs';

const file = process.argv[2];
const b = fs.readFileSync(file);
const pe = b.readUInt32LE(0x3c);
if (b.toString('latin1', pe, pe + 4) !== 'PE\0\0') throw new Error(`${file} is not a PE file`);
const sections = b.readUInt16LE(pe + 6);
const optional = pe + 24;
const magic = b.readUInt16LE(optional);
const dirs = optional + (magic === 0x20b ? 112 : 96);
const importRva = b.readUInt32LE(dirs + 8);
const table = optional + b.readUInt16LE(pe + 20);
const offset = (rva) => {
  for (let i = 0; i < sections; i++) {
    const s = table + i * 40;
    const va = b.readUInt32LE(s + 12);
    const size = Math.max(b.readUInt32LE(s + 8), b.readUInt32LE(s + 16));
    if (rva >= va && rva < va + size) return rva - va + b.readUInt32LE(s + 20);
  }
  throw new Error(`RVA ${rva} is in no section`);
};
const names = [];
for (let at = offset(importRva); b.readUInt32LE(at + 12) !== 0; at += 20) {
  const name = offset(b.readUInt32LE(at + 12));
  names.push(b.toString('latin1', name, b.indexOf(0, name)));
}
const runtime = names.filter((n) => /^(vcruntime|msvcp)\d*.*\.dll$|^api-ms-win-crt-/i.test(n));
console.log(`${file} imports: ${names.join(', ')}`);
if (runtime.length) {
  console.log(`FAIL tc4_exe_imports_no_vcruntime: ${runtime.join(', ')}`);
  process.exitCode = 1;
} else console.log('ok tc4_exe_imports_no_vcruntime');

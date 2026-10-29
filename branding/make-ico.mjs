// Pack PNG representations made from icon-1024.png into a Windows icon.
// Usage: node branding/make-ico.mjs <directory with 16.png,32.png,48.png,256.png> <output.ico>
import fs from 'node:fs';
import path from 'node:path';
const [source, output] = process.argv.slice(2);
const sizes = [16, 32, 48, 256];
const images = sizes.map((size) => fs.readFileSync(path.join(source, `${size}.png`)));
const header = Buffer.alloc(6 + 16 * sizes.length);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(sizes.length, 4);
let offset = header.length;
for (let i = 0; i < sizes.length; i++) {
  const at = 6 + 16 * i;
  header[at] = header[at + 1] = sizes[i] % 256;
  header.writeUInt16LE(1, at + 4);
  header.writeUInt16LE(32, at + 6);
  header.writeUInt32LE(images[i].length, at + 8);
  header.writeUInt32LE(offset, at + 12);
  offset += images[i].length;
}
fs.writeFileSync(output, Buffer.concat([header, ...images]));

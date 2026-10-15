// The gallery imports tC4's design system straight from its checkout, so what it renders is
// the vendored DS at that commit, not a copy. TC4_ROOT points at another checkout.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const TC4 = path.resolve(process.env.TC4_ROOT ?? '/home/user/unfoldingword/translationcore4');

export default defineConfig({
  root: HERE,
  base: './',
  plugins: [react()],
  resolve: {
    alias: { '@ds': path.join(TC4, 'src/ds'), '@tc4': TC4 },
    // The DS files sit in tC4's tree, next to tC4's own node_modules. Without dedupe their
    // `import React from 'react'` would load a second React and every hook would throw.
    dedupe: ['react', 'react-dom'],
  },
  server: { fs: { allow: [HERE, path.join(TC4, 'src/ds'), path.join(TC4, 'public/assets')] } },
  // gallery-spec.json is bundled whole (about 600 kB), which is fine for a local page.
  build: { outDir: path.join(HERE, 'dist'), emptyOutDir: true, sourcemap: false, chunkSizeWarningLimit: 2000 },
  logLevel: 'warn',
});

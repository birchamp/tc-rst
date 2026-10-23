import init, { run } from './loopback.js';
await init();
// `Array.from`, so Playwright gets a plain array back.
window.__run = async (o) => Array.from(await run(o.clicks, o.warmup, o.size, o.self));
window.__ready = true;

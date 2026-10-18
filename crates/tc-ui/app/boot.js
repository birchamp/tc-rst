// The UI's entry (APP-DESIGN 5.2, 6.8). The page's CSP refuses inline scripts, so the page
// loads this file. It loads the wasm and starts the app with the build folder it was served
// from, where the catalogs are. Unit A4b adds, before the import, the capture listener that
// writes each keystroke to the draft buffer, and the panic recovery (AD30).
import init, { run_app } from './tc_ui.js';

await init();
run_app(new URL('./', import.meta.url).pathname);

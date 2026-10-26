// The installed app's entry point. Electronite runs `resources/app` with no arguments, so this
// names the shell's folders from where the app is installed, then hands over to the shell
// (docs/contracts/SHELL.md item 1, docs/RELEASE.md "Where things live").
const path = require('node:path');
const { app } = require('electron');

// One name on every system, so the user-data folder (Chromium's profile and the shell's single-
// instance lock) is the same whichever installer put the app there.
app.setName('translationCore4');
const home = path.join(process.resourcesPath, 'tc4');
const exe = path.join(home, process.platform === 'win32' ? 'tc4.exe' : 'tc4');
// tc4's own folder beside Chromium's, never inside the install folder: an uninstall or an
// update replaces the install folder and must leave resources, settings and drafts alone.
const appData = path.join(app.getPath('userData'), 'app-data');

process.argv = [
  process.argv[0],
  '--tc4', exe,
  '--ui', path.join(home, 'ui'),
  '--app-data', appData,
  '--bundle', path.join(home, 'bundle'),
];
require('./shell/main.cjs');

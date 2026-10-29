'use strict';

// `window.tc4Shell` (SHELL.md item 5). A sandboxed preload can require only `electron`, so the
// channels are written out here; bridge.test.cjs holds them equal to bridge.cjs's BRIDGE.
const { contextBridge, ipcRenderer } = require('electron');

const invoke = (channel) => (...args) => ipcRenderer.invoke(channel, ...args);
const send = (channel) => (...args) => ipcRenderer.send(channel, ...args);

// The titleBar mark lets the page's header move the window where it has no native bar
// (tC4 #214, #639, #642).
const titleBar = process.platform === 'darwin' ? 'hidden' : 'overlay';
window.addEventListener('DOMContentLoaded', () => {
  document.documentElement.dataset.titleBar = titleBar;
});

contextBridge.exposeInMainWorld('tc4Shell', {
  bufferChanged: send('shell:bufferChanged'),
  chooseFile: invoke('shell:chooseFile'),
  chooseFolder: invoke('shell:chooseFolder'),
  feedbackSend: invoke('shell:feedbackSend'),
  // `fn` answers whether everything is saved; an answer that is not `true`, or that throws,
  // counts as unsaved, so the shell asks the person.
  onCloseRequested: (fn) => {
    ipcRenderer.removeAllListeners('shell:closeRequested');
    ipcRenderer.on('shell:closeRequested', async (_event, id) => {
      let clean = false;
      try {
        clean = (await fn()) === true;
      } catch (_) {
        clean = false;
      }
      ipcRenderer.send('shell:closeAnswer', id, clean);
    });
    ipcRenderer.send('shell:closeRequested:register');
  },
  onDownloadDone: (fn) => {
    const relay = (_event, result) => fn(result);
    ipcRenderer.on('shell:downloadDone', relay);
    return () => ipcRenderer.removeListener('shell:downloadDone', relay);
  },
  openExternal: invoke('shell:openExternal'),
  platform: process.platform,
  printPdf: invoke('shell:printPdf'),
  setCanClose: send('shell:setCanClose'),
  takeBuffered: invoke('shell:takeBuffered'),
});

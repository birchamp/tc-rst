'use strict';

// The spike's preload: the part of the shell bridge of APP-DESIGN §5.7 that G0 needs
// (`printPdf`, `onDownloadDone`, `bufferChanged`, `takeBuffered`, `platform`), plus a
// `spike` object through which the page reports what it measured.
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('tc4Shell', {
  platform: process.platform,
  printPdf: (url) => ipcRenderer.invoke('shell:printPdf', url),
  onDownloadDone: (listener) => {
    const relay = (_event, result) => listener(result);
    ipcRenderer.on('download:done', relay);
    return () => ipcRenderer.removeListener('download:done', relay);
  },
  // AD21: one asynchronous message per change; the main process appends it to its mirror.
  bufferChanged: (entry) => ipcRenderer.send('buffer:changed', entry),
  takeBuffered: () => ipcRenderer.invoke('buffer:take'),
  spike: {
    config: () => ipcRenderer.invoke('spike:config'),
    event: (name, data) => ipcRenderer.send('spike:event', name, data),
    report: (results) => ipcRenderer.invoke('spike:report', results),
    filterCounts: () => ipcRenderer.invoke('spike:filterCounts'),
    childWindow: () => ipcRenderer.invoke('spike:childWindow'),
    capture: (rect) => ipcRenderer.invoke('spike:capture', rect),
  },
});

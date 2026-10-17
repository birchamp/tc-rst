// The Electronite spike's page (G0, docs/APP-DESIGN.md §10). It runs in the Electronite shell
// (spikes/electronite/shell) and, for the tests in this container, in Chromium through
// Playwright. It probes each kind of request the browser makes by itself, times the aligner's
// round trip, and plays the parts of the supervision cases that need a page.
'use strict';

(() => {
  const shell = window.tc4Shell || null;
  const query = new URLSearchParams(location.search);
  const DRAFTS = 'tc4.drafts.v2';
  const FIELD = 'spike-project|spike-id|PSA 119:176';
  const KEY_EVERY_MS = 50;

  // §5.2: the token comes in the URL fragment, is kept in sessionStorage, and the page sends it
  // in a header on its own requests. Under protocol.handle there is no token: the shell adds it.
  function readToken() {
    const match = /(?:^#|&)t=([^&]+)/.exec(location.hash);
    if (match) {
      try {
        sessionStorage.setItem('tc4.token', match[1]);
      } catch (_) {
        // sessionStorage can be refused; the token still works for this page.
      }
      history.replaceState(null, '', location.pathname + location.search);
      return match[1];
    }
    try {
      return sessionStorage.getItem('tc4.token');
    } catch (_) {
      return null;
    }
  }
  const token = readToken();

  function apiHeaders() {
    const headers = { 'X-Tc-Api': '1', 'Content-Type': 'application/json' };
    if (token) headers['X-Tc-Token'] = token;
    return headers;
  }

  async function api(path, body) {
    const response = await fetch(path, {
      method: 'POST',
      headers: apiHeaders(),
      body: JSON.stringify(body || {}),
    });
    if (!response.ok) throw new Error(`${path} answered ${response.status}`);
    return response.status === 204 ? null : response.json();
  }

  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  function withTimeout(promise, ms, what) {
    return Promise.race([
      promise,
      sleep(ms).then(() => {
        throw new Error(`${what}: no answer in ${ms} ms`);
      }),
    ]);
  }

  function setStatus(text) {
    document.getElementById('status').textContent = text;
  }

  function shellEvent(name, data) {
    if (shell && shell.spike) shell.spike.event(name, data || {});
  }

  async function config() {
    if (shell && shell.spike) return shell.spike.config();
    return {
      mode: query.get('mode') || 'browser',
      case: query.get('case') || 'run',
      clicks: Number(query.get('clicks') || 200),
      warmup: Number(query.get('warmup') || 20),
    };
  }

  // Nearest-rank statistics, the way foldbench reports them.
  function stats(samples) {
    const sorted = samples.slice().sort((a, b) => a - b);
    const n = sorted.length;
    if (n === 0) return { n: 0 };
    const at = (q) => sorted[Math.min(n - 1, Math.floor(n * q))];
    const round = (v) => Math.round(v * 100) / 100;
    return {
      n,
      min: round(sorted[0]),
      median: round(at(0.5)),
      p95: round(at(0.95)),
      max: round(sorted[n - 1]),
      mean: round(sorted.reduce((a, b) => a + b, 0) / n),
    };
  }

  // ---------------------------------------------------------------- probes

  async function serverHits(probe) {
    const reply = await api('/api/v1/spike/log', { probe });
    return reply.hits;
  }

  async function waitHits(probe, ms) {
    const end = performance.now() + (ms || 5000);
    for (;;) {
      const hits = await serverHits(probe);
      if (hits.some((h) => h.status < 400) || performance.now() > end) return hits;
      await sleep(100);
    }
  }

  function describeHits(hits) {
    if (!hits.length) return 'the server saw no request';
    const counts = {};
    for (const h of hits) {
      const key = `${h.status} token ${h.token}`;
      counts[key] = (counts[key] || 0) + 1;
    }
    return Object.entries(counts)
      .map(([k, v]) => `${v}× ${k}`)
      .join(', ');
  }

  function serverOk(hits) {
    return hits.some((h) => h.token === 'ok' && h.status >= 200 && h.status < 300);
  }

  // A probe passes when the page saw the load succeed and the server saw it with the token.
  function verdict(pageOk, hits, pageDetail) {
    return { pass: Boolean(pageOk) && serverOk(hits), detail: `${pageDetail}; server: ${describeHits(hits)}` };
  }

  const skip = (why) => ({ pass: null, detail: `skipped: ${why}` });

  function loadImage(src, parent) {
    return new Promise((resolve) => {
      const img = new Image();
      img.onload = () => resolve({ ok: true, img });
      img.onerror = () => resolve({ ok: false, img });
      img.src = src;
      parent.append(img);
    });
  }

  const probes = [
    {
      name: 'document',
      what: 'index.html, loaded by the window itself',
      run: async () => ({ pass: true, detail: 'this page is running' }),
    },
    {
      name: 'script',
      what: 'a <script src>',
      run: async () => {
        const hits = await waitHits('script');
        return verdict(true, hits, 'this script is running');
      },
    },
    {
      name: 'stylesheet',
      what: 'a <link rel=stylesheet>',
      run: async () => {
        const hits = await waitHits('stylesheet');
        const styled = getComputedStyle(document.querySelector('.note')).color === 'rgb(170, 51, 51)';
        return verdict(styled, hits, styled ? 'its rules apply' : 'its rules do not apply');
      },
    },
    {
      name: 'fetch',
      what: "the page's own fetch, with the header it sets",
      run: async () => {
        const hello = await api('/api/v1/spike/hello?probe=fetch');
        const hits = await waitHits('fetch');
        return verdict(Boolean(hello.instance), hits, `hello from instance ${hello.instance}`);
      },
    },
    {
      name: 'image',
      what: 'an <img>',
      run: async () => {
        const { ok, img } = await loadImage('/media/obs/01.png?probe=image', document.getElementById('gallery'));
        const hits = await waitHits('image');
        return verdict(ok, hits, ok ? `loaded, ${img.naturalWidth}×${img.naturalHeight}` : 'error event');
      },
    },
    {
      name: 'iframe',
      what: 'a help page in an <iframe>, with its own style sheet and pictures',
      run: async () => {
        const frame = document.createElement('iframe');
        const loaded = new Promise((resolve) => {
          frame.onload = resolve;
          setTimeout(resolve, 8000);
        });
        frame.src = '/help/page.html?probe=iframe';
        document.getElementById('help-slot').append(frame);
        await loaded;
        let shown = false;
        let pictures = 0;
        try {
          shown = Boolean(frame.contentDocument.getElementById('help-ok'));
          await sleep(300);
          pictures = [...frame.contentDocument.images].filter((i) => i.complete && i.naturalWidth > 0).length;
        } catch (_) {
          shown = false;
        }
        const hits = await waitHits('iframe');
        const css = await waitHits('iframe-css');
        const imgs = await waitHits('iframe-img');
        const all = serverOk(hits) && serverOk(css) && serverOk(imgs);
        return {
          pass: shown && pictures === 4 && all,
          detail: `${shown ? 'the help page shows' : 'no help page in the frame'}, ${pictures} of 4 pictures; ` +
            `server: page ${describeHits(hits)}; style sheet ${describeHits(css)}; pictures ${describeHits(imgs)}`,
        };
      },
    },
    {
      name: 'font',
      what: 'a web font from @font-face',
      run: async () => {
        let faces = [];
        let error = '';
        try {
          faces = await withTimeout(document.fonts.load('16px "ProbeFont"'), 8000, 'font');
        } catch (e) {
          error = String(e);
        }
        const loaded = faces.length > 0 && faces.every((f) => f.status === 'loaded');
        const hits = await waitHits('font');
        return verdict(loaded, hits, loaded ? `${faces.length} face loaded` : `not loaded ${error}`);
      },
    },
    {
      name: 'css-url',
      what: 'a picture named by url() in a style sheet',
      run: async () => {
        document.getElementById('css-url-target').className = 'probe-bg';
        const hits = await waitHits('css-url', 5000);
        return verdict(true, hits, 'the element is styled');
      },
    },
    {
      name: 'event-stream',
      what: 'the event stream, read with fetch and the header (AD10)',
      run: async () => {
        const times = [];
        const start = performance.now();
        const controller = new AbortController();
        let status = 0;
        try {
          const response = await fetch('/api/v1/events?probe=events&every=100', {
            headers: apiHeaders(),
            signal: controller.signal,
          });
          status = response.status;
          if (response.ok) {
            const reader = response.body.getReader();
            const decoder = new TextDecoder();
            let buffer = '';
            const deadline = start + 3000;
            while (times.length < 5 && performance.now() < deadline) {
              const left = deadline - performance.now();
              const chunk = await Promise.race([reader.read(), sleep(left).then(() => ({ done: true }))]);
              if (chunk.done) break;
              buffer += decoder.decode(chunk.value, { stream: true });
              let end;
              while ((end = buffer.indexOf('\n\n')) >= 0) {
                buffer = buffer.slice(end + 2);
                times.push(performance.now() - start);
              }
            }
          }
        } catch (e) {
          return { pass: false, detail: `error ${e}` };
        } finally {
          controller.abort();
        }
        const fmt = (v) => (v === undefined ? 'never' : `${Math.round(v)} ms`);
        // Five frames 100 ms apart must arrive one by one; a stream held back until it ends
        // would deliver none.
        const pass = status === 200 && times.length >= 5 && times[0] < 1000 && times[4] < 2500;
        return { pass, detail: `HTTP ${status}, ${times.length} frames, the first at ${fmt(times[0])}, the fifth at ${fmt(times[4])}` };
      },
    },
    {
      name: 'eventsource',
      what: 'EventSource (informational: AD10 does not use it)',
      informational: true,
      run: async () => {
        const outcome = await new Promise((resolve) => {
          const source = new EventSource('/api/v1/events?probe=eventsource&every=100');
          const timer = setTimeout(() => {
            source.close();
            resolve('no frame in 3 s');
          }, 3000);
          source.onmessage = () => {
            clearTimeout(timer);
            source.close();
            resolve('a frame arrived');
          };
          source.onerror = () => {
            clearTimeout(timer);
            source.close();
            resolve('error event');
          };
        });
        const hits = await serverHits('eventsource');
        return {
          pass: outcome === 'a frame arrived',
          detail: `${outcome}; server: ${describeHits(hits)}. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)`,
        };
      },
    },
    {
      name: 'download',
      what: 'an <a download> link',
      run: async () => {
        if (!shell) return skip('needs the shell');
        const done = new Promise((resolve) => {
          const off = shell.onDownloadDone((result) => {
            off();
            resolve(result);
          });
          setTimeout(() => resolve(null), 10000);
        });
        const link = document.createElement('a');
        link.href = '/download/7/probe.txt?probe=download';
        link.download = 'probe.txt';
        document.body.append(link);
        link.click();
        link.remove();
        const result = await done;
        const hits = await waitHits('download');
        const ok = Boolean(result) && result.state === 'completed' && result.bytes > 0;
        return verdict(ok, hits, result ? `${result.state}, ${result.bytes} bytes saved` : 'no download event in 10 s');
      },
    },
    {
      name: 'print',
      what: "the shell's hidden print window and printToPDF",
      run: async () => {
        if (!shell) return skip('needs the shell');
        const result = await withTimeout(shell.printPdf('/print/1?probe=print'), 20000, 'print');
        const hits = await waitHits('print');
        const ok = Boolean(result) && result.status === 200 && result.bytes > 0;
        return verdict(ok, hits, result ? `HTTP ${result.status}, a PDF of ${result.bytes} bytes` : 'no answer');
      },
    },
    {
      name: 'child-window',
      what: 'a same-origin window.open, kept in the app (§5.7, A-33)',
      run: async () => {
        if (!shell) return skip('needs the shell');
        window.open('/help/child.html?probe=child', '_blank');
        const hits = await waitHits('child', 8000);
        const opened = await shell.spike.childWindow();
        return verdict(opened && opened.status === 200, hits, opened ? `the child window got HTTP ${opened.status}` : 'no child window');
      },
    },
    {
      name: 'api-hop',
      what: 'no /api/ request passes through the main process',
      run: async () => {
        if (!shell) return skip('needs the shell');
        const seen = await shell.spike.filterCounts();
        if (seen.mode === 'protocol') {
          return { pass: null, detail: `every request crosses the main process under protocol.handle (${seen.total} so far)` };
        }
        const api = seen.byPath.api || 0;
        return {
          pass: seen.mode === 'filter' ? api === 0 : null,
          detail: `the filter (${seen.patternForm}) saw ${api} /api/ request(s) and ${seen.total} in all: ${JSON.stringify(seen.byPath)}`,
        };
      },
    },
  ];

  async function runProbes() {
    const results = [];
    const table = document.getElementById('probes');
    for (const probe of probes) {
      setStatus(`Probe: ${probe.name}`);
      let outcome;
      try {
        outcome = await withTimeout(probe.run(), 30000, probe.name);
      } catch (e) {
        outcome = { pass: false, detail: `error: ${e && e.message ? e.message : e}` };
      }
      const row = { name: probe.name, what: probe.what, informational: Boolean(probe.informational), ...outcome };
      results.push(row);
      const line = document.createElement('div');
      line.className = row.pass === true ? 'probe-pass' : row.pass === false ? 'probe-fail' : '';
      line.textContent = `${row.name}: ${row.pass === true ? 'passes' : row.pass === false ? 'fails' : 'skipped'}: ${row.detail}`;
      table.append(line);
    }
    return results;
  }

  // ---------------------------------------------------------------- the aligner's round trip

  let clickSeq = 0;
  let pending = null;

  function render(reply) {
    const bank = document.createDocumentFragment();
    for (const word of reply.view.bank) {
      const span = document.createElement('span');
      span.className = word.id === reply.armed ? 'word armed' : 'word';
      span.dataset.occurrence = String(word.occurrence);
      span.textContent = word.text;
      bank.append(span);
    }
    document.getElementById('bank').replaceChildren(bank);
    const cards = document.createDocumentFragment();
    for (const card of reply.view.cards) {
      const div = document.createElement('div');
      div.className = 'card';
      const source = document.createElement('div');
      source.className = 'source';
      source.textContent = card.source.map((w) => w.text).join(' ');
      const target = document.createElement('div');
      target.className = 'target';
      target.textContent = card.target.map((w) => w.text).join(' ');
      div.append(source, target);
      cards.append(div);
    }
    document.getElementById('cards').replaceChildren(cards);
  }

  document.getElementById('click').addEventListener('click', async () => {
    const settle = pending;
    pending = null;
    try {
      const response = await fetch('/api/v1/align/click', {
        method: 'POST',
        headers: apiHeaders(),
        body: JSON.stringify({ word: clickSeq++ % 40 }),
      });
      if (!response.ok) throw new Error(`align/click answered ${response.status}`);
      const bytes = Number(response.headers.get('content-length') || 0);
      const reply = await response.json();
      const replied = performance.now();
      render(reply);
      const rendered = performance.now();
      if (settle) settle.resolve({ replied, rendered, bytes });
    } catch (e) {
      if (settle) settle.reject(e);
    }
  });

  // The start of the next frame, in which the new DOM is painted; null if no frame came in
  // 200 ms (a hidden or covered window).
  function nextFrame() {
    return new Promise((resolve) => {
      let settled = false;
      requestAnimationFrame(() => {
        if (!settled) {
          settled = true;
          resolve(performance.now());
        }
      });
      setTimeout(() => {
        if (!settled) {
          settled = true;
          resolve(null);
        }
      }, 200);
    });
  }

  async function roundTrips(count, warmup, label) {
    const button = document.getElementById('click');
    const reply = [];
    const rendered = [];
    const frame = [];
    let frameTimeouts = 0;
    let bytes = 0;
    for (let i = 0; i < warmup + count; i++) {
      const done = new Promise((resolve, reject) => {
        pending = { resolve, reject };
      });
      const start = performance.now();
      button.click();
      const result = await withTimeout(done, 10000, 'align/click');
      const painted = await nextFrame();
      if (painted === null) frameTimeouts++;
      if (i >= warmup) {
        reply.push(result.replied - start);
        rendered.push(result.rendered - start);
        frame.push((painted === null ? performance.now() : painted) - start);
        bytes = result.bytes;
      }
      if (i % 20 === 0) setStatus(`Round trip (${label}): ${i} of ${warmup + count}`);
      // A person's clicks are not back to back.
      await sleep(20);
    }
    return {
      label,
      clicks: count,
      warmup,
      replyBytes: bytes,
      frameTimeouts,
      reply: stats(reply),
      rendered: stats(rendered),
      frame: stats(frame),
      raw: { reply, rendered, frame },
    };
  }

  // Reads the event stream with fetch until the signal aborts, as the app does all the time.
  async function holdEventStream(signal, every, onFrame) {
    try {
      const response = await fetch(`/api/v1/events?every=${every}`, { headers: apiHeaders(), signal });
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      for (;;) {
        const { value, done } = await reader.read();
        if (done) return;
        buffer += decoder.decode(value, { stream: true });
        let end;
        while ((end = buffer.indexOf('\n\n')) >= 0) {
          buffer = buffer.slice(end + 2);
          onFrame();
        }
      }
    } catch (_) {
      // Aborted at the end of the measurement.
    }
  }

  // OBS pictures and a help page loading: a story's 15 pictures every 500 ms, the help page
  // every second, and the event stream open, so the aligner's request competes for the six
  // connections HTTP/1.1 allows (RT-14).
  function startLoad() {
    const load = { pictures: 0, pictureErrors: 0, helpPages: 0, events: 0 };
    let running = true;
    let seq = 0;
    const gallery = document.getElementById('gallery');
    gallery.replaceChildren();
    const story = () => {
      if (!running) return;
      const frames = [];
      for (let k = 0; k < 15; k++) {
        const img = new Image();
        img.onload = () => load.pictures++;
        img.onerror = () => load.pictureErrors++;
        img.src = `/media/obs/${String((seq % 50) + 1).padStart(2, '0')}.png?r=${seq}`;
        seq++;
        frames.push(img);
      }
      gallery.replaceChildren(...frames);
    };
    story();
    const pictureTimer = setInterval(story, 500);
    const frame = document.createElement('iframe');
    document.getElementById('help-slot').replaceChildren(frame);
    const helpTimer = setInterval(() => {
      frame.src = `/help/page.html?r=${seq++}`;
      load.helpPages++;
    }, 1000);
    const controller = new AbortController();
    holdEventStream(controller.signal, 250, () => load.events++);
    return () => {
      running = false;
      clearInterval(pictureTimer);
      clearInterval(helpTimer);
      controller.abort();
      return load;
    };
  }

  // ---------------------------------------------------------------- Graphite (informational)

  async function graphite() {
    const span = document.getElementById('graphite-text');
    const text = span.textContent;
    span.scrollIntoView({ block: 'center' });
    let loaded = false;
    try {
      const faces = await withTimeout(document.fonts.load('32px "AwamiProbe"', text), 8000, 'graphite font');
      loaded = faces.length > 0;
    } catch (_) {
      loaded = false;
    }
    await nextFrame();
    await nextFrame();
    const box = span.getBoundingClientRect();
    const canvas = document.createElement('canvas');
    const context = canvas.getContext('2d');
    context.font = '32px "AwamiProbe"';
    const metrics = context.measureText(text);
    const result = {
      fontLoaded: loaded,
      text,
      boxWidth: Math.round(box.width * 10) / 10,
      boxHeight: Math.round(box.height * 10) / 10,
      canvasWidth: Math.round(metrics.width * 10) / 10,
      ascent: Math.round(metrics.actualBoundingBoxAscent * 10) / 10,
      descent: Math.round(metrics.actualBoundingBoxDescent * 10) / 10,
    };
    if (shell && shell.spike) {
      result.screenshot = await shell.spike.capture({
        x: Math.floor(box.left) - 8,
        y: Math.floor(box.top) - 8,
        width: Math.ceil(box.width) + 16,
        height: Math.ceil(box.height) + 16,
      });
    }
    return result;
  }

  // ---------------------------------------------------------------- the cases

  async function report(results) {
    window.__spikeResults = results;
    if (shell && shell.spike) await shell.spike.report(results);
    setStatus('Done. This window closes by itself.');
  }

  async function runCase(cfg) {
    const results = {
      case: 'run',
      mode: cfg.mode,
      userAgent: navigator.userAgent,
      origin: location.origin,
      tokenFromFragment: Boolean(token),
      probes: await runProbes(),
      roundTrip: [],
    };
    setStatus('Round trip, alone');
    results.roundTrip.push(await roundTrips(cfg.clicks, cfg.warmup, 'alone'));
    setStatus('Round trip, while pictures and a help page load');
    const stop = startLoad();
    await sleep(500);
    const loaded = await roundTrips(cfg.clicks, cfg.warmup, 'loading');
    loaded.load = stop();
    results.roundTrip.push(loaded);
    setStatus('Graphite');
    results.graphite = await graphite();
    await report(results);
  }

  async function homeCase() {
    const hello = await api('/api/v1/spike/hello');
    setStatus(`Home: server ${hello.instance} on port ${hello.port}`);
    shellEvent('home-loaded', { instance: hello.instance, port: hello.port, serverPid: hello.pid });
    holdEventStream(new AbortController().signal, 1000, () => {});
  }

  function storeDraft(entry) {
    try {
      const all = JSON.parse(localStorage.getItem(DRAFTS) || '{}');
      all[FIELD] = entry;
      localStorage.setItem(DRAFTS, JSON.stringify(all));
    } catch (_) {
      // A refused localStorage is what the shell's mirror is for (AD21).
    }
  }

  // Types one letter every 50 ms on a schedule fixed from t0, so the number of keystrokes
  // made before any moment is known from the clock alone.
  async function typeCase() {
    const area = document.getElementById('draft');
    area.focus();
    area.addEventListener('input', () => {
      const entry = { text: area.value, n: area.value.length, at: Date.now() };
      storeDraft(entry);
      if (shell) shell.bufferChanged({ key: FIELD, ...entry });
      fetch('/api/v1/spike/typed', {
        method: 'POST',
        headers: apiHeaders(),
        body: JSON.stringify({ field: FIELD, n: entry.n, at: entry.at }),
        keepalive: true,
      }).catch(() => {});
    });
    const t0 = Date.now();
    shellEvent('typing-started', { t0, every: KEY_EVERY_MS });
    setStatus('Typing');
    for (let i = 1; i <= 6000; i++) {
      const wait = t0 + i * KEY_EVERY_MS - Date.now();
      if (wait > 0) await sleep(wait);
      area.value += String.fromCharCode(97 + (i % 26));
      area.dispatchEvent(new Event('input', { bubbles: true }));
    }
  }

  async function inspectCase() {
    let stored = null;
    try {
      const all = JSON.parse(localStorage.getItem(DRAFTS) || 'null');
      const entry = all && all[FIELD];
      stored = entry ? { n: entry.n, at: entry.at, fields: Object.keys(all).length } : { n: 0, fields: all ? Object.keys(all).length : 0 };
    } catch (e) {
      stored = { error: String(e) };
    }
    const mirrored = shell ? await shell.takeBuffered() : [];
    const mine = mirrored.find((e) => e.key === FIELD);
    await report({
      case: 'inspect',
      origin: location.origin,
      localStorage: stored,
      mirror: mine ? { n: mine.n, at: mine.at, entries: mirrored.length } : { n: 0, entries: mirrored.length },
    });
  }

  async function main() {
    const cfg = await config();
    document.title = `tC4 spike: ${cfg.mode} ${cfg.case}`;
    try {
      if (cfg.case === 'run') await runCase(cfg);
      else if (cfg.case === 'type') await typeCase();
      else if (cfg.case === 'inspect') await inspectCase();
      else await homeCase();
    } catch (e) {
      setStatus(`Failed: ${e && e.message ? e.message : e}`);
      shellEvent('page-error', { message: String(e && e.message ? e.message : e) });
      await report({ case: cfg.case, error: String(e && e.message ? e.message : e) });
    }
  }

  main();
})();

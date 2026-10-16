// Two modes.
//   ?case=<id>  One case alone, its frame at the top-left corner of the page. capture.mjs sizes
//               the viewport to the frame, so a Layer's fixed scrim covers exactly the frame and
//               an anchored popover measures the same window the frame is. The case mounts only
//               when window.__mount() is called (or with &auto=1), so the capture can make its
//               pointer click first (see capture.mjs).
//   (none)      Every case, grouped by component, for browsing. Cases that open a layer, need
//               an interaction or set the document language load in an iframe of the first mode.
import React from 'react';
import { createRoot } from 'react-dom/client';
import '@ds/styles.css';
import spec from '../gallery-spec.json';
import { Tree } from './render.jsx';

function Frame({ c, mounted = true, inline = false }) {
  const { width, height, padding, background } = c.frame;
  return (
    <div data-case={c.id} tabIndex={inline ? undefined : -1}
      lang={inline ? c.document.lang : undefined} dir={inline ? c.document.dir : undefined}
      style={{ width, height, padding, background, overflow: 'hidden', boxSizing: 'border-box' }}>
      {mounted ? <Tree tree={c.render ?? { $el: c.component, props: c.props }} initial={c.cells} /> : null}
    </div>
  );
}

const nextFrame = () => new Promise((r) => requestAnimationFrame(() => r()));

async function loadAllFonts() {
  if (document.readyState !== 'complete') await new Promise((r) => window.addEventListener('load', r, { once: true }));
  await Promise.all(Array.from(document.fonts, (f) => f.load().catch(() => null)));
  await document.fonts.ready;
}

async function single(id, auto) {
  const c = spec.cases.find((x) => x.id === id);
  const root = createRoot(document.getElementById('root'));
  if (!c) {
    root.render(<p>No case {id}</p>);
    return;
  }
  document.documentElement.lang = c.document.lang;
  document.documentElement.dir = c.document.dir;
  document.body.style.background = c.frame.background;
  await loadAllFonts();
  const mount = async () => {
    root.render(<Frame c={c} />);
    await nextFrame();
    await nextFrame();
    window.__galleryMounted = true;
  };
  window.__mount = mount;
  root.render(<Frame c={c} mounted={false} />);
  await nextFrame();
  window.__galleryBooted = true;
  if (auto) await mount();
}

function Browse() {
  const groups = [];
  for (const c of spec.cases) {
    const last = groups[groups.length - 1];
    if (last && last.component === c.component) last.cases.push(c);
    else groups.push({ component: c.component, cases: [c] });
  }
  return (
    <div style={{ padding: 24 }}>
      <h1>tC4 design-system gallery</h1>
      <p>{spec.cases.length} cases from {spec.source.ds} at {spec.source.commit.slice(0, 7)}.
        Cases in an iframe open a layer, need an interaction (shown here without it) or set the document language.</p>
      <nav style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 24 }}>
        {groups.map((g) => <a key={g.component} href={'#' + g.component}>{g.component}</a>)}
      </nav>
      {groups.map((g) => (
        <section key={g.component} id={g.component} style={{ marginBottom: 40 }}>
          <h2>{g.component}</h2>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 20, alignItems: 'flex-start' }}>
            {g.cases.map((c) => (
              <figure key={c.id} style={{ margin: 0 }}>
                <figcaption style={{ font: '12px ui-monospace, monospace', marginBottom: 4 }}>{c.state}</figcaption>
                <div style={{ outline: '1px dashed rgba(1,66,99,.25)' }}>
                  {c.isolate
                    ? <iframe title={c.id} src={'?case=' + encodeURIComponent(c.id) + '&auto=1'} loading="lazy"
                        style={{ display: 'block', border: 0, width: c.frame.width, height: c.frame.height }} />
                    : <Frame c={c} inline />}
                </div>
              </figure>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}

const params = new URLSearchParams(window.location.search);
if (params.has('case')) single(params.get('case'), params.get('auto') === '1');
else createRoot(document.getElementById('root')).render(<Browse />);

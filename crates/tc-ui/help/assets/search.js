/* tC4 Help search — hand-rolled, no dependencies, works from file:// and http://.
 * The index is window.TC4_SEARCH_INDEX (search-index.js, written by build.py). */
(function () {
  'use strict';
  var INDEX = (window.TC4_SEARCH_INDEX && window.TC4_SEARCH_INDEX.docs) || [];

  function fold(s) {
    return String(s || '').toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '');
  }
  function stem(w) {
    if (w.length > 5 && /ing$/.test(w)) return w.slice(0, -3);
    if (w.length > 4 && /ed$/.test(w)) return w.slice(0, -2);
    if (w.length > 4 && /ies$/.test(w)) return w.slice(0, -3) + 'y';
    if (w.length > 3 && /s$/.test(w) && !/ss$/.test(w)) return w.slice(0, -1);
    return w;
  }
  function tokens(s) {
    return fold(s).split(/[^a-z0-9\u0370-\u03ff\u0590-\u05ff]+/).filter(Boolean);
  }
  // A few words translators use for the same thing.
  var SYN = {
    bridge: ['span'], bridged: ['span'], span: ['bridge'], combine: ['span', 'join'], join: ['span'],
    offline: ['network', 'online'], internet: ['online', 'offline', 'network'], network: ['offline', 'online'],
    download: ['install', 'source'], resource: ['source', 'help'], helps: ['note', 'word', 'question'],
    publish: ['export', 'pdf'], print: ['pdf', 'export'], backup: ['export', 'burrito'],
    comment: ['note'], flag: ['bookmark'], bookmark: ['flag'], star: ['bookmark'],
    delete: ['remove'], rename: ['name', 'setting'], font: ['script', 'setting'],
    story: ['obs', 'stories'], stories: ['obs', 'story'], obs: ['story', 'stories'],
    greek: ['original'], hebrew: ['original'], gateway: ['checking', 'language'],
    undo: ['cancel'], crash: ['recover', 'recovery'], error: ['failed', 'problem']
  };

  function prep(d) {
    if (d._p) return d._p;
    d._p = {
      title: tokens(d.title).map(stem),
      head: tokens((d.headings || []).join(' ') + ' ' + d.section).map(stem),
      label: tokens((d.labels || []).join(' ') + ' ' + (d.where || '')).map(stem),
      body: tokens(d.summary + ' ' + d.text).map(stem),
      flatTitle: fold(d.title), flatBody: fold(d.summary + ' ' + d.text + ' ' + (d.labels || []).join(' '))
    };
    return d._p;
  }
  function countMatch(list, q) {
    var n = 0;
    for (var i = 0; i < list.length; i++) {
      var t = list[i];
      if (t === q) n += 2; else if (q.length >= 3 && t.indexOf(q) === 0) n += 1;
    }
    return n;
  }
  function termScore(p, q) {
    var s = countMatch(p.title, q) * 12 + countMatch(p.head, q) * 5 + countMatch(p.label, q) * 4;
    var b = countMatch(p.body, q);
    return s + Math.min(b, 20);
  }

  function search(query) {
    var raw = tokens(query);
    if (!raw.length) return [];
    var terms = raw.map(stem);
    var phrase = fold(query).trim();
    var results = [];
    INDEX.forEach(function (d) {
      var p = prep(d), total = 0, hitAll = true;
      terms.forEach(function (q) {
        var s = termScore(p, q);
        if (!s && SYN[q]) SYN[q].forEach(function (alt) { s = Math.max(s, termScore(p, stem(alt)) * 0.5); });
        if (!s) hitAll = false;
        total += s;
      });
      if (!total) return;
      if (raw.length > 1 && phrase) {
        if (p.flatTitle.indexOf(phrase) >= 0) total += 40;
        if (p.flatBody.indexOf(phrase) >= 0) total += 15;
      }
      if (hitAll) total *= 2;
      if (d.kind !== 'feature') total *= 0.6; // release notes rank below how-to pages
      results.push({ doc: d, score: total, all: hitAll });
    });
    var anyAll = results.some(function (r) { return r.all; });
    if (anyAll) results = results.filter(function (r) { return r.all; });
    results.sort(function (a, b) { return b.score - a.score; });
    return results;
  }

  function escapeHtml(s) {
    return String(s).replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; });
  }
  function snippet(d, query) {
    var text = d.summary + ' — ' + d.text;
    var flat = fold(text);
    var terms = tokens(query);
    var phrase = fold(query).trim();
    var at = phrase ? flat.indexOf(phrase) : -1;
    if (at < 0) {
      for (var i = 0; i < terms.length && at < 0; i++) {
        var q = terms[i].length > 4 ? stem(terms[i]) : terms[i];
        at = flat.indexOf(q);
      }
    }
    if (at < 0) at = 0;
    var start = Math.max(0, at - 70), end = Math.min(text.length, at + 150);
    var s = (start > 0 ? '… ' : '') + text.slice(start, end) + (end < text.length ? ' …' : '');
    var out = escapeHtml(s);
    terms.forEach(function (t) {
      if (t.length < 2) return;
      var q = t.length > 4 ? stem(t) : t;
      out = out.replace(new RegExp('(' + q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '[a-z]*)', 'gi'), '<mark>$1</mark>');
    });
    return out;
  }

  window.TC4Search = { search: search, snippet: snippet };

  // ---- header box: live results
  var input = document.getElementById('q');
  var box = document.getElementById('search-results');
  var active = -1;
  function render(q) {
    if (!box) return;
    if (!q.trim()) { box.hidden = true; box.innerHTML = ''; return; }
    var res = search(q).slice(0, 8);
    active = -1;
    if (!res.length) {
      box.innerHTML = '<div class="sr-empty">No topics match “' + escapeHtml(q) + '”. Try another word, such as align, check, story or offline.</div>';
    } else {
      box.innerHTML = res.map(function (r) {
        return '<a role="option" href="' + r.doc.url + '"><span class="sr-title">' + escapeHtml(r.doc.title) +
          '</span><span class="sr-sec">' + escapeHtml(r.doc.section) + '</span><span class="sr-snip">' + snippet(r.doc, q) + '</span></a>';
      }).join('') + '<a href="search.html?q=' + encodeURIComponent(q) + '"><span class="sr-sec">See all results →</span></a>';
    }
    box.hidden = false;
  }
  if (input) {
    input.addEventListener('input', function () { render(input.value); });
    input.addEventListener('keydown', function (e) {
      var links = box ? box.querySelectorAll('a') : [];
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        if (!links.length) return;
        active = (active + (e.key === 'ArrowDown' ? 1 : -1) + links.length) % links.length;
        links.forEach(function (a, i) { a.classList.toggle('active', i === active); });
      } else if (e.key === 'Enter' && active >= 0 && links[active]) {
        e.preventDefault();
        window.location.href = links[active].getAttribute('href');
      } else if (e.key === 'Escape') {
        box.hidden = true;
      }
    });
    document.addEventListener('click', function (e) { if (box && !box.contains(e.target) && e.target !== input) box.hidden = true; });
  }
  document.addEventListener('keydown', function (e) {
    if (e.key === '/' && document.activeElement && !/INPUT|TEXTAREA/.test(document.activeElement.tagName)) {
      e.preventDefault();
      if (input) input.focus();
    }
  });

  // ---- mobile menu
  var btn = document.getElementById('menu-btn');
  if (btn) btn.addEventListener('click', function () {
    var open = document.body.classList.toggle('nav-open');
    btn.setAttribute('aria-expanded', open ? 'true' : 'false');
  });

  // ---- search.html: full results
  var list = document.getElementById('search-page-results');
  if (list) {
    var q = new URLSearchParams(window.location.search).get('q') || '';
    var q2 = document.getElementById('q2');
    if (q2) q2.value = q;
    if (input) input.value = q;
    var res = q ? search(q) : [];
    var sum = document.getElementById('search-summary');
    if (sum) sum.textContent = q ? (res.length + ' topic' + (res.length === 1 ? '' : 's') + ' for “' + q + '”') : 'Type a word or two, such as “align”, “bookmark” or “verse span”.';
    list.innerHTML = res.map(function (r) {
      return '<li><a href="' + r.doc.url + '">' + escapeHtml(r.doc.title) + '</a> <span class="muted">· ' + escapeHtml(r.doc.section) +
        '</span><br><span>' + snippet(r.doc, q) + '</span></li>';
    }).join('');
  }
})();

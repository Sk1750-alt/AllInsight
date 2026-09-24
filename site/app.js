/*
  AllInsight — site behaviour.

  No dependencies, no third-party requests. Every continuous animation runs
  only while its element is on screen, and prefers-reduced-motion shows every
  visual in its finished state instead.
*/

(() => {
  'use strict';

  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const easeOut = (t) => 1 - Math.pow(1 - t, 3);

  /* Animate a number from 0 to `to` over `ms`, calling `draw` each frame. */
  const tween = (to, ms, draw, from = 0) => {
    if (reduced) { draw(to); return; }
    const start = performance.now();
    const step = (now) => {
      const t = clamp((now - start) / ms, 0, 1);
      draw(from + (to - from) * easeOut(t));
      if (t < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  };

  /* Run `enter` once when `el` scrolls into view. */
  const once = (el, enter, margin = '0px 0px -15% 0px') => {
    if (!el) return;
    if (reduced || !('IntersectionObserver' in window)) { enter(); return; }
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) { io.disconnect(); enter(); }
    }, { rootMargin: margin });
    io.observe(el);
  };

  /* Toggle `live` state while `el` is visible. */
  const whileVisible = (el, onChange) => {
    if (!el || !('IntersectionObserver' in window)) return;
    new IntersectionObserver((entries) => {
      entries.forEach((e) => onChange(e.isIntersecting));
    }).observe(el);
  };

  /* ── Reveal on scroll ─────────────────────────────────────────── */
  $$('.reveal').forEach((el) => once(el, () => el.classList.add('in'), '0px 0px -8% 0px'));

  /* ── Nav: hairline, dark mode over the dark section, progress, section ── */
  {
    const nav = $('#nav');
    const bar = $('#progress');
    const dark = $('#privacy');
    const links = $$('.nav-links a');
    const sections = links.map((a) => $(a.getAttribute('href')));
    let queued = false;

    const frame = () => {
      queued = false;
      const y = scrollY;
      nav.classList.toggle('is-stuck', y > 8);
      const max = document.documentElement.scrollHeight - innerHeight;
      bar.style.transform = `scaleX(${max > 0 ? clamp(y / max, 0, 1) : 0})`;
      if (dark) {
        const r = dark.getBoundingClientRect();
        nav.classList.toggle('is-dark', r.top < 52 && r.bottom > 52);
      }
      let here = -1;
      sections.forEach((s, i) => { if (s && s.getBoundingClientRect().top < innerHeight * 0.4) here = i; });
      links.forEach((a, i) => a.classList.toggle('is-here', i === here));
      gates();
    };
    addEventListener('scroll', () => { if (!queued) { queued = true; requestAnimationFrame(frame); } }, { passive: true });
    addEventListener('resize', frame, { passive: true });

    /* ── Safety gates: a file travels the rail as the page scrolls ── */
    const gatesEl = $('#gates');
    const items = $$('.gates-list li');
    const verdict = $('#gatesVerdict');
    function gates() {
      if (!gatesEl) return;
      const r = gatesEl.getBoundingClientRect();
      const p = reduced ? 1 : clamp((innerHeight * 0.6 - r.top) / (r.height - 60), 0, 1);
      gatesEl.style.setProperty('--p', `${(p * 100).toFixed(2)}%`);
      items.forEach((li, i) => li.classList.toggle('passed', p >= (i + 0.5) / items.length));
      verdict.classList.toggle('shown', p > 0.97);
    }

    frame();
  }

  /* ── Hero dial ─────────────────────────────────────────────────── */
  {
    const ticks = $('#dialTicks');
    const arc = $('#dialArc');
    const value = $('#dialValue');
    const SCORE = 86;
    const R = 214;
    const C = 2 * Math.PI * R;
    const N = 100;
    const ns = 'http://www.w3.org/2000/svg';

    for (let i = 0; i < N; i++) {
      const a = (i / N) * Math.PI * 2 - Math.PI / 2;
      const major = i % 10 === 0;
      const r1 = 246;
      const r2 = major ? 270 : 260;
      const line = document.createElementNS(ns, 'line');
      line.setAttribute('x1', (300 + r1 * Math.cos(a)).toFixed(1));
      line.setAttribute('y1', (300 + r1 * Math.sin(a)).toFixed(1));
      line.setAttribute('x2', (300 + r2 * Math.cos(a)).toFixed(1));
      line.setAttribute('y2', (300 + r2 * Math.sin(a)).toFixed(1));
      if (major) line.classList.add('major');
      ticks.appendChild(line);
    }
    const tickEls = Array.from(ticks.children);

    arc.style.strokeDasharray = `${C}`;
    arc.style.strokeDashoffset = `${C}`;

    const draw = (v) => {
      arc.style.strokeDashoffset = `${C * (1 - v / 100)}`;
      value.textContent = Math.round(v);
      const lit = Math.round(v);
      tickEls.forEach((t, i) => t.classList.toggle('lit', i < lit));
    };
    setTimeout(() => tween(SCORE, 2200, draw), reduced ? 0 : 1100);

    // Chips count up with the dial.
    $$('.chip-v').forEach((el) => {
      const to = Number(el.dataset.count);
      setTimeout(() => tween(to, 1600, (v) => { el.textContent = `${Math.round(v)}${el.dataset.suffix || ''}`; }), reduced ? 0 : 1600);
    });

    // The instrument drifts up a little slower than the page: depth, not
    // decoration.
    const inst = $('#instrument');
    if (!reduced && inst) {
      let q = false;
      addEventListener('scroll', () => {
        if (q) return;
        q = true;
        requestAnimationFrame(() => {
          q = false;
          const y = Math.min(scrollY, innerHeight * 1.2);
          inst.style.transform = `translateY(${(y * 0.12).toFixed(1)}px)`;
        });
      }, { passive: true });
    }
  }

  /* ── Privacy wall: packets run only while visible ──────────────── */
  {
    const wall = $('#wall');
    whileVisible(wall, (on) => wall.classList.toggle('is-live', on));
  }

  /* ── Story: the step nearest the middle drives the stage ───────── */
  {
    const steps = $$('.step');
    const scenes = $$('.scene');
    let current = 1;
    const typed = new WeakSet();

    const typeLine = (el) => {
      if (!el || typed.has(el)) return;
      typed.add(el);
      const text = el.dataset.text || '';
      if (reduced) { el.textContent = text; el.classList.add('done'); return; }
      let i = 0;
      const tick = () => {
        el.textContent = text.slice(0, ++i);
        if (i < text.length) setTimeout(tick, 18 + Math.random() * 22);
        else el.classList.add('done');
      };
      setTimeout(tick, 350);
    };

    const show = (n) => {
      if (n === current) return;
      current = n;
      steps.forEach((s) => s.classList.toggle('is-on', Number(s.dataset.step) === n));
      scenes.forEach((s) => s.classList.toggle('is-on', Number(s.dataset.scene) === n));
      if (n === 2) typeLine($('.type-line'));
      if (n === 3) {
        const num = $('.reclaim-num');
        tween(Number(num.dataset.to), 1800, (v) => { num.textContent = v.toFixed(2); });
      }
    };

    if ('IntersectionObserver' in window) {
      const io = new IntersectionObserver((entries) => {
        entries.forEach((e) => { if (e.isIntersecting) show(Number(e.target.dataset.step)); });
      }, { rootMargin: '-45% 0px -45% 0px' });
      steps.forEach((s) => io.observe(s));
    }
  }

  /* ── Big numbers ───────────────────────────────────────────────── */
  $$('.stat-n').forEach((el) => {
    const to = Number(el.dataset.count);
    once(el, () => tween(to, 1400, (v) => { el.textContent = Math.round(v); }));
  });

  /* ── Living tiles ──────────────────────────────────────────────── */
  {
    // Drive ring fills to its reading.
    const fill = $('#ringFill');
    const ringValue = $('#ringValue');
    const RC = 2 * Math.PI * 84;
    fill.style.strokeDasharray = `${RC}`;
    fill.style.strokeDashoffset = `${RC}`;
    once($('[data-live="drive"]'), () => tween(93, 1800, (v) => {
      fill.style.strokeDashoffset = `${RC * (1 - v / 100)}`;
      ringValue.textContent = `${Math.round(v)}%`;
    }));

    // A processor trace that keeps drawing while it is on screen.
    const line = $('#sparkLine');
    const area = $('#sparkArea');
    const cpu = $('#cpuValue');
    const W = 300;
    const H = 90;
    const pts = Array.from({ length: 40 }, (_, i) => 18 + 8 * Math.sin(i / 3));
    const render = () => {
      const step = W / (pts.length - 1);
      const d = pts.map((v, i) => `${i ? 'L' : 'M'}${(i * step).toFixed(1)},${(H - (v / 100) * H).toFixed(1)}`).join('');
      line.setAttribute('d', d);
      area.setAttribute('d', `${d}L${W},${H}L0,${H}Z`);
      cpu.textContent = Math.round(pts[pts.length - 1]);
    };
    render();
    let timer = null;
    whileVisible($('[data-live="perf"]'), (on) => {
      if (reduced) return;
      if (on && !timer) {
        timer = setInterval(() => {
          const last = pts[pts.length - 1];
          const burst = Math.random() < 0.06 ? 14 : 0;
          pts.push(clamp(last + (Math.random() - 0.5) * 7 + burst - (last > 30 ? 5 : 0), 4, 60));
          pts.shift();
          render();
        }, 700);
      } else if (!on && timer) {
        clearInterval(timer);
        timer = null;
      }
    });

    // Two files, one content: they slide together, then apart again.
    const dupes = $('.dupes');
    let dupeTimer = null;
    whileVisible($('[data-live="dupes"]'), (on) => {
      if (reduced) { dupes.classList.add('merged'); return; }
      if (on && !dupeTimer) {
        dupes.classList.add('merged');
        dupeTimer = setInterval(() => dupes.classList.toggle('merged'), 2600);
      } else if (!on && dupeTimer) {
        clearInterval(dupeTimer);
        dupeTimer = null;
      }
    });

    // The assistant thinks, then answers, once.
    const chat = $('.chat');
    const answer = $('.answer');
    once($('[data-live="ai"]'), () => {
      const text = answer.dataset.text;
      const finish = () => { chat.classList.add('answered'); };
      if (reduced) { finish(); answer.textContent = text; return; }
      setTimeout(() => {
        finish();
        let i = 0;
        const tick = () => {
          answer.textContent = text.slice(0, ++i);
          if (i < text.length) setTimeout(tick, 16 + Math.random() * 24);
        };
        tick();
      }, 1400);
    });
  }

  /* ── Download details from downloads.json ─────────────────────── */
  {
    const size = (n) => {
      if (!Number.isFinite(n)) return null;
      const units = ['B', 'KB', 'MB', 'GB'];
      let i = 0;
      let v = n;
      while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
      return `${v.toFixed(i >= 2 ? 1 : 0)} ${units[i]}`;
    };
    const set = (sel, val) => { const el = $(sel); if (el && val) el.textContent = val; };
    const fill = (entry, id) => {
      if (!entry) return;
      const a = $(`#dl${id}`);
      if (a && entry.file) { a.href = `/downloads/${entry.file}`; set(`#dl${id}Name`, entry.file); }
      set(`#dl${id}Size`, size(entry.bytes));
      set(`#hash${id}`, entry.sha256);
    };

    fetch('downloads.json', { cache: 'no-cache' })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((d) => {
        set('#dlVersion', d.version);
        set('#heroVersion', d.version && d.version.replace(/\.0$/, ''));
        set('#dlLicence', d.licence);
        fill(d.installer, 'Installer');
        fill(d.portable, 'Portable');
        if (d.linux) {
          fill(d.linux.deb, 'Deb');
          fill(d.linux.rpm, 'Rpm');
          fill(d.linux.appimage, 'AppImage');
        }
        if (d.installer && d.installer.file) set('#dlInstallerCmd', d.installer.file);
      })
      .catch(() => { /* markup fallbacks stand */ });
  }
})();

/*
  AllInsight — site behaviour.

  No dependencies, no third-party requests. Motion is slow and sparing: the
  mark draws itself on load (pure CSS), sections settle into place, a rule
  fills as the safety checks scroll past, and the app icon builds itself once. prefers-reduced-motion shows everything in
  its finished state.
*/

(() => {
  'use strict';

  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const easeOut = (t) => 1 - Math.pow(1 - t, 4);

  /* Animate a number from 0 to `to` over `ms`, calling `draw` each frame. */
  const tween = (to, ms, draw) => {
    if (reduced) { draw(to); return; }
    const start = performance.now();
    const step = (now) => {
      const t = clamp((now - start) / ms, 0, 1);
      draw(to * easeOut(t));
      if (t < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  };

  /* Run `enter` once when `el` scrolls into view. */
  const once = (el, enter, margin = '0px 0px -10% 0px') => {
    if (!el) return;
    if (reduced || !('IntersectionObserver' in window)) { enter(); return; }
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) { io.disconnect(); enter(); }
    }, { rootMargin: margin });
    io.observe(el);
  };

  /* ── Reveal on scroll ─────────────────────────────────────────── */
  $$('.reveal').forEach((el) => once(el, () => el.classList.add('in'), '0px 0px -8% 0px'));

  /* ── Nav state, current section and the safety rule ────────────── */
  {
    const nav = $('#nav');
    const dark = $('#privacy');
    const links = $$('.nav-links a');
    const sections = links.map((a) => $(a.getAttribute('href')));
    const gatesEl = $('#gates');
    const items = $$('.gates-list li');
    let queued = false;

    const frame = () => {
      queued = false;
      if (nav) {
        nav.classList.toggle('is-stuck', scrollY > 8);
        if (dark) {
          const r = dark.getBoundingClientRect();
          nav.classList.toggle('is-dark', r.top < 32 && r.bottom > 32);
        }
      }
      let here = -1;
      sections.forEach((s, i) => { if (s && s.getBoundingClientRect().top < innerHeight * 0.4) here = i; });
      links.forEach((a, i) => a.classList.toggle('is-here', i === here));

      if (gatesEl) {
        const r = gatesEl.getBoundingClientRect();
        const p = reduced ? 1 : clamp((innerHeight * 0.62 - r.top) / r.height, 0, 1);
        gatesEl.style.setProperty('--p', `${(p * 100).toFixed(2)}%`);
        items.forEach((li, i) => li.classList.toggle('passed', p >= (i + 0.6) / items.length));
      }
    };
    addEventListener('scroll', () => { if (!queued) { queued = true; requestAnimationFrame(frame); } }, { passive: true });
    addEventListener('resize', frame, { passive: true });
    frame();
  }

  /* ── App icon: builds itself when the download section arrives ── */
  {
    const icon = $('#appIcon');
    if (icon) once(icon, () => icon.classList.add('built'), '0px 0px -20% 0px');
  }

  /* ── Privacy horizon: the point of light runs only while visible ── */
  {
    const horizon = $('#horizon');
    if (horizon && !reduced && 'IntersectionObserver' in window) {
      new IntersectionObserver((entries) => {
        entries.forEach((e) => horizon.classList.toggle('is-live', e.isIntersecting));
      }).observe(horizon);
    }
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

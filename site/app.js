/*
  AllInsight — site behaviour, shared by every page.

  No dependencies, no third-party requests. Each piece below looks for its
  own markup and does nothing on pages that do not have it. Everything that
  moves pauses while off screen or in a background tab, and
  prefers-reduced-motion shows each piece in its finished state instead.
*/

(() => {
  'use strict';

  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const easeOut = (t) => 1 - Math.pow(1 - t, 4);
  const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

  /* Animate a number from `from` to `to` over `ms`, calling `draw` each frame. */
  const tween = (from, to, ms, draw) => {
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
  const once = (el, enter, margin = '0px 0px -10% 0px') => {
    if (!el) return;
    if (reduced || !('IntersectionObserver' in window)) { enter(); return; }
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) { io.disconnect(); enter(); }
    }, { rootMargin: margin });
    io.observe(el);
  };

  /* Call `fn(true|false)` as `el` enters and leaves the screen. */
  const whileVisible = (el, fn) => {
    if (!('IntersectionObserver' in window)) { fn(true); return; }
    new IntersectionObserver((entries) => entries.forEach((e) => fn(e.isIntersecting))).observe(el);
  };

  /* ── Reveal on scroll ─────────────────────────────────────────── */
  $$('.reveal').forEach((el) => once(el, () => el.classList.add('in'), '0px 0px -8% 0px'));

  /* ── Current page in the navigation ───────────────────────────── */
  {
    const here = location.pathname.replace(/\.html$/, '').replace(/\/index$/, '/') || '/';
    $$('.nav-links a, .menu a, .foot-links a').forEach((a) => {
      const path = new URL(a.href, location.href).pathname.replace(/\.html$/, '');
      if (path === here) a.setAttribute('aria-current', 'page');
    });
  }

  /* ── Phone menu ───────────────────────────────────────────────── */
  {
    const toggle = $('.nav-toggle');
    const menu = $('#menu');
    if (toggle && menu) {
      const set = (open) => {
        toggle.setAttribute('aria-expanded', String(open));
        toggle.textContent = open ? 'Close' : 'Menu';
        menu.hidden = !open;
        document.documentElement.classList.toggle('menu-open', open);
      };
      toggle.addEventListener('click', () => set(menu.hidden));
      menu.addEventListener('click', (e) => { if (e.target.closest('a')) set(false); });
      addEventListener('keydown', (e) => { if (e.key === 'Escape' && !menu.hidden) set(false); });
      matchMedia('(min-width: 861px)').addEventListener('change', (e) => { if (e.matches) set(false); });
    }
  }

  /* ── Nav state and the safety rule ────────────────────────────── */
  {
    const nav = $('#nav');
    const darks = $$('.dark');
    const gatesEl = $('#gates');
    const items = $$('.gates-list li');
    let queued = false;

    const frame = () => {
      queued = false;
      if (nav) {
        nav.classList.toggle('is-stuck', scrollY > 8);
        nav.classList.toggle('is-dark', darks.some((d) => {
          const r = d.getBoundingClientRect();
          return r.top < 32 && r.bottom > 32;
        }));
      }
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

  /* ── Counting numbers: <span data-count="14"> ─────────────────── */
  $$('[data-count]').forEach((el) => {
    const to = Number(el.dataset.count);
    const from = Number(el.dataset.from || 0);
    const dec = Number(el.dataset.decimals || 0);
    const fmt = (v) => v.toLocaleString('en', { minimumFractionDigits: dec, maximumFractionDigits: dec });
    el.textContent = fmt(from);
    once(el, () => tween(from, to, 1800, (v) => { el.textContent = fmt(v); }));
  });

  /* ── App icon: builds itself when it arrives ──────────────────── */
  {
    const icon = $('#appIcon');
    if (icon) once(icon, () => icon.classList.add('built'), '0px 0px -20% 0px');
  }

  /* ── Privacy horizon: the point of light runs only while visible ── */
  {
    const horizon = $('#horizon');
    if (horizon && !reduced) whileVisible(horizon, (v) => horizon.classList.toggle('is-live', v));
  }

  /* ── Priorities light up in order ─────────────────────────────── */
  {
    const pr = $('#priority');
    if (pr) once(pr, () => $$('span', pr).forEach((s, i) => setTimeout(() => s.classList.add('on'), reduced ? 0 : 350 * i)));
  }

  /* ── Storage Map demo: a squarified treemap that drills down ──── */
  {
    const box = $('#treemap');
    if (box) {
      const levels = [
        { path: 'C:\\', items: [['Videos', 96.4], ['Applications', 71.2], ['Games', 54.0], ['Downloads', 38.0], ['Windows', 31.5], ['Pictures', 22.7], ['Documents', 18.2], ['Caches', 11.4, 1], ['Other', 9.8]] },
        { path: 'C:\\Users\\you\\Videos', items: [['Recordings', 41.3], ['Projects', 28.9], ['Phone backup', 14.6], ['Exports', 8.1], ['Clips', 3.5]] },
        { path: 'C:\\', items: [['Videos', 96.4], ['Applications', 71.2], ['Games', 54.0], ['Downloads', 38.0], ['Windows', 31.5], ['Pictures', 22.7], ['Documents', 18.2], ['Caches', 11.4, 1], ['Other', 9.8]] },
        { path: 'C:\\Users\\you\\Downloads', items: [['Installers', 16.2], ['Archives', 9.4], ['ISO images', 7.8], ['PDFs', 2.9], ['Other', 1.7]] },
      ];
      const shades = ['#3a4252', '#343b4a', '#2f3542', '#2b303c', '#282c37', '#252933', '#22262f', '#20232b', '#1e2128'];
      const label = $('#treemapPath');
      const tiles = new Map();

      /* Squarified layout (Bruls, Huizing, van Wijk), as the app draws it. */
      const squarify = (items, x, y, w, h) => {
        const total = items.reduce((s, i) => s + i.v, 0);
        const scale = (w * h) / total;
        const rest = items.map((i) => ({ ...i, a: i.v * scale }));
        const out = [];
        while (rest.length) {
          const short = Math.min(w, h);
          let row = [rest.shift()];
          const worst = (r) => {
            const s = r.reduce((t, i) => t + i.a, 0);
            return Math.max(...r.map((i) => Math.max((short * short * i.a) / (s * s), (s * s) / (short * short * i.a))));
          };
          while (rest.length && worst([...row, rest[0]]) <= worst(row)) row.push(rest.shift());
          const s = row.reduce((t, i) => t + i.a, 0);
          const thick = s / short;
          let off = 0;
          for (const i of row) {
            const len = i.a / thick;
            out.push(w >= h ? { ...i, x, y: y + off, w: thick, h: len } : { ...i, x: x + off, y, w: len, h: thick });
            off += len;
          }
          if (w >= h) { x += thick; w -= thick; } else { y += thick; h -= thick; }
        }
        return out;
      };

      const show = (level) => {
        const W = box.clientWidth, H = box.clientHeight, gap = 4;
        const items = level.items.map(([n, v, safe], k) => ({ n, v, safe, k }));
        const total = items.reduce((s, i) => s + i.v, 0);
        const laid = squarify(items, 0, 0, W, H);
        const seen = new Set();
        for (const t of laid) {
          let el = tiles.get(t.n);
          if (!el) {
            el = document.createElement('div');
            el.className = 'tile';
            el.innerHTML = '<b></b><i></i>';
            el.style.cssText = `left:${W / 2}px;top:${H / 2}px;width:0;height:0;opacity:0`;
            box.appendChild(el); tiles.set(t.n, el);
            el.getBoundingClientRect();
          }
          seen.add(t.n);
          el.querySelector('b').textContent = t.n;
          el.querySelector('i').textContent = `${t.v.toFixed(1)} GB · ${Math.round((t.v / total) * 100)}%`;
          el.classList.toggle('safe', !!t.safe);
          el.classList.toggle('small', t.w < 96 || t.h < 50);
          el.classList.toggle('tiny', t.w < 52 || t.h < 26);
          el.style.setProperty('--tile', t.safe ? '#3a3326' : shades[Math.min(t.k, shades.length - 1)]);
          Object.assign(el.style, { left: `${t.x + gap / 2}px`, top: `${t.y + gap / 2}px`, width: `${Math.max(0, t.w - gap)}px`, height: `${Math.max(0, t.h - gap)}px`, opacity: '1' });
        }
        for (const [n, el] of tiles) if (!seen.has(n)) Object.assign(el.style, { opacity: '0', width: '0px', height: '0px' });
        if (label) label.textContent = level.path;
      };

      let i = 0, timer = 0;
      show(levels[0]);
      addEventListener('resize', () => show(levels[i]), { passive: true });
      if (!reduced) whileVisible(box, (v) => {
        clearInterval(timer);
        if (v) timer = setInterval(() => { i = (i + 1) % levels.length; show(levels[i]); }, 3600);
      });
    }
  }

  /* ── Health score ring ────────────────────────────────────────── */
  {
    const ring = $('#healthRing');
    if (ring) once(ring, () => {
      const score = Number(ring.dataset.score);
      const value = $('.value', ring);
      const num = $('.ring-num', ring);
      const wrap = ring.closest('.frame');
      if (wrap) wrap.classList.add('is-live');
      value.style.strokeDashoffset = String(327 * (1 - score / 100));
      tween(0, score, 2200, (v) => { num.textContent = Math.round(v); });
    });
  }

  /* ── Live performance chart ───────────────────────────────────── */
  {
    const canvas = $('#liveChart');
    if (canvas) {
      const ctx = canvas.getContext('2d');
      const cpuOut = $('#cpuNow'), memOut = $('#memNow');
      const N = 90;
      let cpu = Array.from({ length: N }, (_, k) => 22 + 10 * Math.sin(k / 7));
      let mem = Array.from({ length: N }, () => 61);
      let running = false, timer = 0;
      const walk = (v, lo, hi, step) => clamp(v + (Math.random() - 0.5) * step + (Math.random() < 0.04 ? step * 3 : 0), lo, hi);
      const draw = () => {
        const r = canvas.getBoundingClientRect(), dpr = Math.min(devicePixelRatio || 1, 2);
        if (canvas.width !== Math.round(r.width * dpr)) { canvas.width = Math.round(r.width * dpr); canvas.height = Math.round(r.height * dpr); }
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        const W = r.width, H = r.height;
        ctx.clearRect(0, 0, W, H);
        ctx.strokeStyle = 'rgba(244,242,238,0.07)'; ctx.lineWidth = 1;
        for (let g = 1; g < 4; g++) { const y = (H / 4) * g; ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(W, y); ctx.stroke(); }
        const line = (arr, color, fill) => {
          ctx.beginPath();
          arr.forEach((v, k) => { const x = (k / (N - 1)) * W, y = H - (v / 100) * H; k ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
          ctx.strokeStyle = color; ctx.lineWidth = 1.6; ctx.stroke();
          if (fill) { ctx.lineTo(W, H); ctx.lineTo(0, H); ctx.closePath(); const g = ctx.createLinearGradient(0, 0, 0, H); g.addColorStop(0, fill); g.addColorStop(1, 'rgba(0,0,0,0)'); ctx.fillStyle = g; ctx.fill(); }
        };
        line(mem, 'rgba(245,165,36,0.9)');
        line(cpu, 'rgba(244,242,238,0.95)', 'rgba(244,242,238,0.10)');
        if (cpuOut) cpuOut.textContent = `${Math.round(cpu[N - 1])}%`;
        if (memOut) memOut.textContent = `${Math.round(mem[N - 1])}%`;
      };
      const tick = () => {
        cpu = [...cpu.slice(1), walk(cpu[N - 1], 4, 96, 14)];
        mem = [...mem.slice(1), walk(mem[N - 1], 52, 74, 1.6)];
        draw();
      };
      draw();
      addEventListener('resize', draw, { passive: true });
      if (!reduced) whileVisible(canvas, (v) => {
        if (v && !running) { running = true; timer = setInterval(tick, 500); }
        if (!v && running) { running = false; clearInterval(timer); }
      });
    }
  }

  /* ── Duplicate pipeline ───────────────────────────────────────── */
  {
    const pipe = $('#pipeline');
    if (pipe) once(pipe, () => {
      const max = Number($$('[data-count]', pipe)[0]?.dataset.count || 1);
      $$('.bars i', pipe).forEach((bar, k) => {
        const v = Number(bar.dataset.v);
        setTimeout(() => { bar.style.width = `${Math.max(1.5, (v / max) * 100)}%`; }, reduced ? 0 : k * 300);
      });
    });
  }

  /* ── Assistant demo: a question, then an answer typed out ─────── */
  {
    const chat = $('#chat');
    if (chat) {
      const q = chat.dataset.question;
      const a = chat.dataset.answer;
      const you = $('.msg.you', chat), ai = $('.msg.ai', chat), text = $('.ai-text', ai), chips = $('.chips', chat), badge = $('.badge', ai);
      const finish = () => { you.hidden = false; you.textContent = q; text.textContent = a; ai.hidden = false; badge.hidden = false; chips.classList.add('in'); $('.caret', ai)?.remove(); };
      if (reduced) finish();
      else once(chat, () => {
        let k = 0;
        you.hidden = false;
        const typeQ = setInterval(() => {
          you.textContent = q.slice(0, ++k);
          if (k >= q.length) {
            clearInterval(typeQ);
            setTimeout(() => {
              ai.hidden = false;
              const words = a.split(' ');
              let n = 0;
              const typeA = setInterval(() => {
                text.textContent = words.slice(0, ++n).join(' ');
                if (n >= words.length) { clearInterval(typeA); $('.caret', ai)?.remove(); badge.hidden = false; setTimeout(() => chips.classList.add('in'), 300); }
              }, 70);
            }, 700);
          }
        }, 38);
      }, '0px 0px -25% 0px');
    }
  }

  /* ── Download details from downloads.json ─────────────────────── */
  if ($('#dlInstaller')) {
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

    fetch('/downloads.json', { cache: 'no-cache' })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((d) => {
        set('#dlVersion', d.version);
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

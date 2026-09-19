/*
  AllInsight — landing page behaviour.

  No dependencies, no build step, no external requests. Every continuous
  animation is gated on an IntersectionObserver so it stops running the moment
  its section leaves the viewport: the audience for this product is people
  whose machine is already struggling.
*/

(() => {
  'use strict';

  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const $  = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
  const lerp  = (a, b, t) => a + (b - a) * t;

  /* ── Reveal on scroll ───────────────────────────────────────────── */

  {
    const items = $$('[data-reveal]');
    items.forEach(el => el.style.setProperty('--d', el.dataset.delay || 0));

    if (reduced || !('IntersectionObserver' in window)) {
      items.forEach(el => el.classList.add('in'));
    } else {
      const io = new IntersectionObserver((entries) => {
        entries.forEach(e => {
          if (!e.isIntersecting) return;
          e.target.classList.add('in');
          io.unobserve(e.target);
        });
      }, { rootMargin: '0px 0px -12% 0px', threshold: 0.08 });
      items.forEach(el => io.observe(el));
    }
  }

  /* ── Scroll progress + sticky nav ───────────────────────────────── */

  {
    const bar = $('#progress');
    const nav = $('#nav');
    let queued = false;

    const onScroll = () => {
      const max = document.documentElement.scrollHeight - innerHeight;
      const p = max > 0 ? clamp(scrollY / max, 0, 1) : 0;
      bar.style.transform = `scaleX(${p})`;
      nav.classList.toggle('stuck', scrollY > 12);
      queued = false;
    };

    addEventListener('scroll', () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(onScroll);
    }, { passive: true });
    onScroll();
  }

  /* ── Pointer spotlight ──────────────────────────────────────────── */

  if (!reduced && matchMedia('(pointer: fine)').matches) {
    const sp = $('#spotlight');
    let tx = innerWidth / 2, ty = innerHeight * 0.4;
    let cx = tx, cy = ty, running = false;

    addEventListener('pointermove', (e) => {
      tx = e.clientX; ty = e.clientY;
      sp.classList.add('on');
      if (!running) { running = true; requestAnimationFrame(tick); }
    }, { passive: true });

    function tick() {
      cx = lerp(cx, tx, 0.12);
      cy = lerp(cy, ty, 0.12);
      sp.style.setProperty('--mx', cx + 'px');
      sp.style.setProperty('--my', cy + 'px');
      if (Math.abs(cx - tx) > 0.5 || Math.abs(cy - ty) > 0.5) requestAnimationFrame(tick);
      else running = false;
    }
  }

  /* ── Magnetic buttons ───────────────────────────────────────────── */

  if (!reduced && matchMedia('(pointer: fine)').matches) {
    $$('[data-magnetic]').forEach(el => {
      const strength = el.classList.contains('dl-card') ? 0.06 : 0.28;

      el.addEventListener('pointermove', (e) => {
        const r = el.getBoundingClientRect();
        const dx = e.clientX - (r.left + r.width / 2);
        const dy = e.clientY - (r.top + r.height / 2);
        el.style.transform = `translate(${dx * strength}px, ${dy * strength}px)`;
      });

      el.addEventListener('pointerleave', () => { el.style.transform = ''; });
    });
  }

  /* ── Card pointer glow + tilt ───────────────────────────────────── */

  if (!reduced && matchMedia('(pointer: fine)').matches) {
    $$('.card').forEach(card => {
      card.addEventListener('pointermove', (e) => {
        const r = card.getBoundingClientRect();
        card.style.setProperty('--cx', ((e.clientX - r.left) / r.width * 100) + '%');
        card.style.setProperty('--cy', ((e.clientY - r.top) / r.height * 100) + '%');
      });
    });

    $$('[data-tilt]').forEach(el => {
      const soft = el.hasAttribute('data-tilt-soft');
      const max = soft ? 2.2 : 5;

      el.addEventListener('pointermove', (e) => {
        const r = el.getBoundingClientRect();
        const px = (e.clientX - r.left) / r.width - 0.5;
        const py = (e.clientY - r.top) / r.height - 0.5;
        el.style.transform =
          `perspective(900px) rotateX(${-py * max}deg) rotateY(${px * max}deg) translateZ(0)`;
      });

      el.addEventListener('pointerleave', () => { el.style.transform = ''; });
    });
  }

  /* ── Headline scramble ──────────────────────────────────────────── */

  {
    const el = $('[data-scramble]');
    if (el && !reduced) {
      const chars = '#$%&*+/<>[]{}~^01';
      // Walk the text nodes only, so <br> and <em> survive intact.
      const nodes = [];
      (function walk(n) {
        n.childNodes.forEach(c => {
          if (c.nodeType === 3 && c.nodeValue.trim()) nodes.push({ node: c, text: c.nodeValue });
          else if (c.nodeType === 1) walk(c);
        });
      })(el);

      const total = nodes.reduce((a, n) => a + n.text.length, 0);
      let frame = 0;
      const speed = 1.6;

      const run = () => {
        let done = 0, seen = 0;
        nodes.forEach(({ node, text }) => {
          let out = '';
          for (let i = 0; i < text.length; i++) {
            const reveal = frame * speed - seen * 0.55;
            if (reveal > 4) { out += text[i]; done++; }
            else if (reveal > 0 && text[i] !== ' ')
              out += chars[(Math.random() * chars.length) | 0];
            else out += text[i] === ' ' ? ' ' : '';
            seen++;
          }
          node.nodeValue = out;
        });
        frame++;
        if (done < total) requestAnimationFrame(run);
        else nodes.forEach(({ node, text }) => { node.nodeValue = text; });
      };
      requestAnimationFrame(run);
    }
  }

  /* ── Count-up ───────────────────────────────────────────────────── */

  {
    const nums = $$('[data-count]');
    const play = (el) => {
      const target = parseFloat(el.dataset.count);
      if (reduced || target === 0) { el.textContent = target; return; }
      const dur = 1100;
      const t0 = performance.now();
      const step = (t) => {
        const p = clamp((t - t0) / dur, 0, 1);
        const eased = 1 - Math.pow(1 - p, 3);
        el.textContent = Math.round(target * eased);
        if (p < 1) requestAnimationFrame(step);
      };
      requestAnimationFrame(step);
    };

    if ('IntersectionObserver' in window) {
      const io = new IntersectionObserver((es) => {
        es.forEach(e => { if (e.isIntersecting) { play(e.target); io.unobserve(e.target); } });
      }, { threshold: 0.5 });
      nums.forEach(n => io.observe(n));
    } else nums.forEach(play);
  }

  /* ── Hero treemap ───────────────────────────────────────────────── */

  {
    const cv = $('#treemap');
    const ctx = cv && cv.getContext('2d');

    // The category split the application actually reports, so the shape of the
    // map on this page matches the shape of the map in the product.
    const DATA = [
      { name: 'Windows',         gb: 40.7, c: '#31b0c6' },
      { name: 'Applications',    gb: 28.3, c: '#5b8dd9' },
      { name: 'User files',      gb: 25.9, c: '#9b7fd4' },
      { name: 'Development',     gb: 21.4, c: '#d97fb0' },
      { name: 'Other',           gb: 15.7, c: '#e0855a' },
      { name: 'Cache',           gb: 15.2, c: '#d8a02a' },
      { name: 'Documents',       gb: 6.99, c: '#7fbf6a' },
      { name: 'Downloads',       gb: 5.90, c: '#4fb8a5' },
      { name: 'Temporary files', gb: 2.49, c: '#6c7a84' }
    ];

    const PATHS = [
      'C:\\Windows\\System32', 'C:\\Program Files', 'C:\\ProgramData\\Package Cache',
      'C:\\Windows\\Installer', 'C:\\Program Files (x86)', 'C:\\Windows\\WinSxS',
      'C:\\Windows\\SoftwareDistribution', 'C:\\Windows\\Temp', 'C:\\Windows\\Logs'
    ];

    /* Squarified treemap (Bruls, Huizing, van Wijk). */
    function squarify(items, x, y, w, h) {
      const out = [];
      const total = items.reduce((a, i) => a + i.gb, 0);
      let rest = items.map(i => ({ ...i, area: i.gb / total * w * h }));

      const worst = (row, len) => {
        const s = row.reduce((a, r) => a + r.area, 0);
        const mx = Math.max(...row.map(r => r.area));
        const mn = Math.min(...row.map(r => r.area));
        return Math.max((len * len * mx) / (s * s), (s * s) / (len * len * mn));
      };

      while (rest.length) {
        const vertical = w >= h;
        const len = vertical ? h : w;
        const row = [rest[0]];
        let i = 1;
        while (i < rest.length && worst(row.concat(rest[i]), len) <= worst(row, len)) {
          row.push(rest[i]); i++;
        }

        const sum = row.reduce((a, r) => a + r.area, 0);
        const thick = sum / len;
        let off = 0;

        row.forEach(r => {
          const side = r.area / thick;
          out.push(vertical
            ? { ...r, x, y: y + off, w: thick, h: side }
            : { ...r, x: x + off, y, w: side, h: thick });
          off += side;
        });

        if (vertical) { x += thick; w -= thick; } else { y += thick; h -= thick; }
        rest = rest.slice(i);
      }
      return out;
    }

    let tiles = [], dpr = 1, W = 0, H = 0;

    function layout() {
      dpr = Math.min(devicePixelRatio || 1, 2);
      W = cv.clientWidth; H = cv.clientHeight;
      cv.width = Math.round(W * dpr);
      cv.height = Math.round(H * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      // Bias the map to the right half; the headline occupies the left.
      const x0 = W * 0.34;
      tiles = squarify(DATA, x0, 0, W - x0, H);
    }

    let raf = 0, start = 0, visible = false;
    const bar   = $('#scanBar');
    const pathEl = $('#scanPath');
    const countEl = $('#scanCount');
    const timeEl = $('#scanTime');
    const legend = $('#scanLegend');

    legend.innerHTML = DATA.slice(0, 5).map(d => `
      <div class="leg-row">
        <i class="leg-dot" style="background:${d.c}"></i>
        <span class="leg-name">${d.name}</span>
        <span class="leg-val" data-gb="${d.gb}">0 GB</span>
      </div>`).join('');
    const legVals = $$('.leg-val', legend);

    const CYCLE = 5200;   // one full scan, ms
    const HOLD  = 2600;   // then hold the finished map before restarting

    function draw(now) {
      const t = (now - start) % (CYCLE + HOLD);
      const p = clamp(t / CYCLE, 0, 1);
      const eased = 1 - Math.pow(1 - p, 2.2);

      ctx.clearRect(0, 0, W, H);

      tiles.forEach((tile, i) => {
        // Tiles resolve one after another, left to right, as the scan advances.
        const at = i / tiles.length * 0.55;
        const local = clamp((eased - at) / 0.45, 0, 1);
        if (local <= 0) return;

        const g = 2;
        const tw = (tile.w - g) * local;
        const th = (tile.h - g) * local;
        if (tw <= 0 || th <= 0) return;

        ctx.globalAlpha = 0.30 + local * 0.42;
        ctx.fillStyle = tile.c;
        ctx.fillRect(tile.x, tile.y, tw, th);

        ctx.globalAlpha = local * 0.5;
        ctx.strokeStyle = tile.c;
        ctx.lineWidth = 1;
        ctx.strokeRect(tile.x + 0.5, tile.y + 0.5, tw, th);
      });

      // The scan line itself.
      if (p < 1) {
        const sx = W * 0.34 + (W - W * 0.34) * eased;
        ctx.globalAlpha = 1;
        const grad = ctx.createLinearGradient(sx - 70, 0, sx, 0);
        grad.addColorStop(0, 'rgba(69,200,222,0)');
        grad.addColorStop(1, 'rgba(69,200,222,.75)');
        ctx.fillStyle = grad;
        ctx.fillRect(sx - 70, 0, 70, H);
        ctx.fillStyle = 'rgba(180,240,250,.9)';
        ctx.fillRect(sx - 1, 0, 1.5, H);
      }
      ctx.globalAlpha = 1;

      // Readout panel, driven by the same clock.
      bar.style.width = (p * 100).toFixed(1) + '%';
      countEl.textContent = Math.round(1631216 * eased).toLocaleString('en-US');
      timeEl.textContent = (39.5 * p).toFixed(1);
      pathEl.textContent = p < 1
        ? PATHS[Math.floor(eased * PATHS.length) % PATHS.length]
        : 'C:\\  ·  complete';
      legVals.forEach(v => {
        v.textContent = (parseFloat(v.dataset.gb) * eased).toFixed(1) + ' GB';
      });

      raf = requestAnimationFrame(draw);
    }

    function play() {
      if (raf || !visible) return;
      start = performance.now();
      raf = requestAnimationFrame(draw);
    }
    function stop() { cancelAnimationFrame(raf); raf = 0; }

    function still() {
      // Reduced motion, or off-screen: paint the finished map once.
      ctx.clearRect(0, 0, W, H);
      tiles.forEach(tile => {
        ctx.globalAlpha = 0.6;
        ctx.fillStyle = tile.c;
        ctx.fillRect(tile.x, tile.y, tile.w - 2, tile.h - 2);
      });
      ctx.globalAlpha = 1;
      bar.style.width = '100%';
      countEl.textContent = (1631216).toLocaleString('en-US');
      timeEl.textContent = '39.5';
      pathEl.textContent = 'C:\\  ·  complete';
      legVals.forEach(v => { v.textContent = parseFloat(v.dataset.gb).toFixed(1) + ' GB'; });
    }

    if (ctx) {
      layout();
      if (reduced) still();
      else {
        const io = new IntersectionObserver(([e]) => {
          visible = e.isIntersecting;
          visible ? play() : stop();
        }, { threshold: 0.02 });
        io.observe(cv);

        document.addEventListener('visibilitychange', () => {
          document.hidden ? stop() : play();
        });
      }

      let rz;
      addEventListener('resize', () => {
        clearTimeout(rz);
        rz = setTimeout(() => { layout(); if (reduced) still(); }, 180);
      }, { passive: true });
    }
  }

  /* ── Outbound packets: the ones that do not exist ───────────────── */

  {
    const cv = $('#packets');
    const ctx = cv && cv.getContext('2d');

    if (ctx) {
      let W = 0, H = 0, dpr = 1, raf = 0, visible = false;
      let particles = [];

      const fit = () => {
        dpr = Math.min(devicePixelRatio || 1, 2);
        W = cv.clientWidth; H = cv.clientHeight;
        cv.width = Math.round(W * dpr);
        cv.height = Math.round(H * dpr);
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      };

      const spawn = () => ({
        x: Math.random() * (W * 0.5) + 12,
        y: Math.random() * (H - 24) + 12,
        vx: 0.25 + Math.random() * 0.85,
        vy: (Math.random() - 0.5) * 0.35,
        r: 1.3 + Math.random() * 1.8,
        life: 1,
        hit: 0
      });

      const seed = () => { particles = Array.from({ length: 34 }, spawn); };

      function frame(now) {
        const wall = W * 0.62;
        ctx.clearRect(0, 0, W, H);

        // The membrane. Nothing on this page crosses it.
        const pulse = 0.35 + Math.sin(now / 700) * 0.12;
        ctx.strokeStyle = `rgba(224,90,82,${pulse})`;
        ctx.lineWidth = 1.4;
        ctx.setLineDash([5, 7]);
        ctx.beginPath(); ctx.moveTo(wall, 6); ctx.lineTo(wall, H - 6); ctx.stroke();
        ctx.setLineDash([]);

        // Dead zone beyond it, drawn as absence.
        ctx.fillStyle = 'rgba(224,90,82,.03)';
        ctx.fillRect(wall, 0, W - wall, H);

        particles.forEach((p, i) => {
          if (p.hit > 0) {
            // Absorbed at the membrane: a ring that fades, then respawns.
            p.hit += 0.045;
            const a = Math.max(0, 1 - p.hit);
            ctx.strokeStyle = `rgba(224,90,82,${a * 0.75})`;
            ctx.lineWidth = 1.2;
            ctx.beginPath();
            ctx.arc(p.x, p.y, 3 + p.hit * 13, 0, Math.PI * 2);
            ctx.stroke();
            if (p.hit >= 1) particles[i] = spawn();
            return;
          }

          p.x += p.vx; p.y += p.vy;
          if (p.y < 8 || p.y > H - 8) p.vy *= -1;

          if (p.x >= wall - p.r) { p.x = wall - p.r; p.hit = 0.001; return; }

          const near = clamp((p.x - W * 0.2) / (wall - W * 0.2), 0, 1);
          ctx.fillStyle = `rgba(49,176,198,${0.28 + near * 0.5})`;
          ctx.beginPath(); ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2); ctx.fill();

          // Trail.
          ctx.strokeStyle = `rgba(49,176,198,${0.12 + near * 0.18})`;
          ctx.lineWidth = 1;
          ctx.beginPath(); ctx.moveTo(p.x - p.vx * 9, p.y - p.vy * 9); ctx.lineTo(p.x, p.y); ctx.stroke();
        });

        raf = requestAnimationFrame(frame);
      }

      const play = () => { if (!raf && visible) raf = requestAnimationFrame(frame); };
      const stop = () => { cancelAnimationFrame(raf); raf = 0; };

      fit(); seed();

      if (reduced) {
        // One static frame: the membrane and a handful of dots behind it.
        const wall = W * 0.62;
        ctx.strokeStyle = 'rgba(224,90,82,.4)'; ctx.setLineDash([5, 7]); ctx.lineWidth = 1.4;
        ctx.beginPath(); ctx.moveTo(wall, 6); ctx.lineTo(wall, H - 6); ctx.stroke(); ctx.setLineDash([]);
        particles.slice(0, 18).forEach(p => {
          ctx.fillStyle = 'rgba(49,176,198,.5)';
          ctx.beginPath(); ctx.arc(Math.min(p.x, wall - 14), p.y, p.r, 0, Math.PI * 2); ctx.fill();
        });
      } else {
        const io = new IntersectionObserver(([e]) => {
          visible = e.isIntersecting;
          visible ? play() : stop();
        }, { threshold: 0.1 });
        io.observe(cv);
        document.addEventListener('visibilitychange', () => { document.hidden ? stop() : play(); });

        let rz;
        addEventListener('resize', () => {
          clearTimeout(rz);
          rz = setTimeout(() => { fit(); seed(); }, 180);
        }, { passive: true });
      }
    }
  }

  /* ── Safety pipeline, driven by scroll position ─────────────────── */

  {
    const pipe = $('#pipeline');
    const fill = $('#pipeFill');
    const gates = $$('[data-gate]');
    const verdict = $('#pipeVerdict');

    if (pipe) {
      if (reduced) {
        gates.forEach(g => g.classList.add('on'));
        fill.style.width = '100%';
        verdict.textContent = 'ValidatedPath issued — deletion permitted';
        verdict.classList.add('pass');
      } else {
        let queued = false;

        const update = () => {
          const r = pipe.getBoundingClientRect();
          // 0 when the block reaches the lower third, 1 when it passes the middle.
          const p = clamp((innerHeight * 0.78 - r.top) / (innerHeight * 0.42), 0, 1);
          fill.style.width = (p * 100).toFixed(1) + '%';

          const openCount = Math.round(p * gates.length);
          gates.forEach((g, i) => g.classList.toggle('on', i < openCount));

          if (p >= 1) {
            verdict.textContent = 'ValidatedPath issued — deletion permitted';
            verdict.classList.add('pass');
          } else {
            verdict.textContent = openCount === 0
              ? 'Waiting'
              : `Gate ${openCount} of ${gates.length} cleared`;
            verdict.classList.remove('pass');
          }
          queued = false;
        };

        addEventListener('scroll', () => {
          if (queued) return;
          queued = true;
          requestAnimationFrame(update);
        }, { passive: true });
        update();
      }
    }
  }

  /* ── Screenshot rail ────────────────────────────────────────────── */

  {
    const img = $('#shotImg');
    const cap = $('#shotCap');
    const meta = {
      overview: {
        src: 'screens/overview.png',
        alt: 'The AllInsight Overview screen: a device health score of 60 marked Fair, four vital-sign tiles, and ranked recommended actions.',
        cap: 'A health score is never a number on its own. It arrives with the reasons that produced it.'
      },
      performance: {
        src: 'screens/performance.png',
        alt: 'The AllInsight Performance screen: live processor, memory, graphics, disk and network readings with history.',
        cap: 'Processor, memory, graphics, disk and network — live, with ten minutes of history behind them.'
      },
      processes: {
        src: 'screens/processes.png',
        alt: 'The AllInsight Processes screen: a live table of running processes with publisher, processor share, memory, disk, process id and uptime.',
        cap: 'Every process with its publisher and its cost. The End button refuses on the ones Windows needs.'
      }
    };

    $$('.rail-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const key = btn.dataset.shot;
        const m = meta[key];
        if (!m || img.src.endsWith(m.src)) return;

        $$('.rail-btn').forEach(b => {
          const on = b === btn;
          b.classList.toggle('is-on', on);
          b.setAttribute('aria-selected', String(on));
        });

        const swap = () => { img.src = m.src; img.alt = m.alt; cap.textContent = m.cap; };

        if (reduced) { swap(); return; }
        img.classList.add('swapping');
        setTimeout(() => {
          swap();
          img.decode ? img.decode().catch(() => {}).finally(() => img.classList.remove('swapping'))
                     : img.classList.remove('swapping');
        }, 180);
      });
    });

    // Warm the other two so the first swap is instant.
    Object.values(meta).forEach(m => { const i = new Image(); i.src = m.src; });
  }

  /* ── Download metadata ──────────────────────────────────────────── */

  /* Kept out of the markup so a release only has to touch downloads.json.
     Fails silently: the hardcoded fallbacks in the HTML stay correct. */
  {
    const bytes = (n) => {
      if (!Number.isFinite(n)) return null;
      const u = ['B', 'KB', 'MB', 'GB'];
      let i = 0, v = n;
      while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
      return `${v.toFixed(i >= 2 ? 1 : 0)} ${u[i]}`;
    };

    const set = (sel, val) => { const el = $(sel); if (el && val) el.textContent = val; };

    fetch('downloads.json', { cache: 'no-cache' })
      .then(r => r.ok ? r.json() : Promise.reject(r.status))
      .then(d => {
        set('#dlVersion', d.version);
        set('#dlLicence', d.licence);

        if (d.installer) {
          const a = $('#dlInstaller');
          if (d.installer.file) { a.href = `/downloads/${d.installer.file}`; set('#dlInstallerName', d.installer.file); }
          set('#dlInstallerSize', bytes(d.installer.bytes));
          set('#hashInstaller', d.installer.sha256);
        }
        if (d.portable) {
          const a = $('#dlPortable');
          if (d.portable.file) { a.href = `/downloads/${d.portable.file}`; set('#dlPortableName', d.portable.file); }
          set('#dlPortableSize', bytes(d.portable.bytes));
          set('#hashPortable', d.portable.sha256);
        }

        const cmd = $('.hash-how code');
        if (cmd && d.installer && d.installer.file)
          cmd.textContent = `Get-FileHash .\\${d.installer.file} -Algorithm SHA256`;
      })
      .catch(() => { /* markup fallbacks stand */ });
  }

})();

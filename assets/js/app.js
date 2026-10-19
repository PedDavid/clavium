// Small behaviours for the server-rendered UI. No inline handlers, so the
// page works under a strict Content-Security-Policy (script-src 'self').
(() => {
  const PALETTES = ['neutral', 'blue', 'green', 'orange', 'violet'];
  const root = document.documentElement;

  const setCookie = (name, value) => {
    document.cookie = `${name}=${encodeURIComponent(value)}; path=/; max-age=31536000; samesite=lax`;
  };

  // --- Command palette (⌘K) ---
  // An ARIA combobox: focus stays in the input, and aria-activedescendant
  // tells assistive technology which option the arrow keys selected.
  const palette = () => document.getElementById('command-palette');
  const input = () => palette()?.querySelector('input[role="combobox"]');
  const results = () => document.getElementById('command-results');
  const items = () => Array.from(results()?.querySelectorAll('[role="option"]') ?? []);

  const setActive = (next) => {
    items().forEach((el) => {
      el.classList.toggle('active', el === next);
      el.setAttribute('aria-selected', el === next ? 'true' : 'false');
    });
    const box = input();
    if (next) {
      box?.setAttribute('aria-activedescendant', next.id);
      next.scrollIntoView({ block: 'nearest' });
    } else {
      box?.removeAttribute('aria-activedescendant');
    }
  };

  // Keeps the combobox state in step with whatever results are shown.
  const syncResults = () => {
    const list = items();
    input()?.setAttribute('aria-expanded', list.length ? 'true' : 'false');
    setActive(list.find((el) => el.classList.contains('active')) ?? null);
  };

  const openPalette = () => {
    const dialog = palette();
    if (!dialog || dialog.open) return;
    // Drop the previous results right away: until the new ones arrive,
    // Enter must not follow a result for a query that is no longer shown.
    results()?.replaceChildren();
    syncResults();
    dialog.showModal();
    const box = input();
    box.value = '';
    box.focus();
    htmx.trigger(box, 'palette-open');
  };

  document.addEventListener('htmx:afterSwap', (event) => {
    if (event.target === results()) syncResults();
  });

  const move = (delta) => {
    const list = items();
    if (!list.length) return;
    const current = list.findIndex((el) => el.classList.contains('active'));
    setActive(list[(current + delta + list.length) % list.length]);
  };

  document.addEventListener('keydown', (event) => {
    const typing = event.target.closest('input, textarea, select, [contenteditable]');
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      openPalette();
    } else if (event.key === '/' && !typing) {
      event.preventDefault();
      openPalette();
    } else if (palette()?.open) {
      if (event.key === 'ArrowDown') { event.preventDefault(); move(1); }
      if (event.key === 'ArrowUp') { event.preventDefault(); move(-1); }
      if (event.key === 'Enter') {
        const active = items().find((el) => el.classList.contains('active'));
        if (active) { event.preventDefault(); active.click(); }
      }
    }
  });

  document.addEventListener('mousemove', (event) => {
    const item = event.target.closest('#command-palette [role="option"]');
    if (item && !item.classList.contains('active')) setActive(item);
  });

  document.addEventListener('click', (event) => {
    if (event.target.closest('[data-palette-open]')) {
      openPalette();
      return;
    }
    const opener = event.target.closest('[data-dialog-open]');
    if (opener) {
      document.getElementById(opener.dataset.dialogOpen)?.showModal();
      return;
    }
    const closer = event.target.closest('[data-dialog-close]');
    if (closer) {
      closer.closest('dialog')?.close();
      return;
    }
    // A click on the backdrop lands on the <dialog> element itself.
    if (event.target instanceof HTMLDialogElement) {
      event.target.close();
      return;
    }
    if (event.target.closest('[data-theme-toggle]')) {
      const dark = root.classList.toggle('dark');
      setCookie('clavium_theme', dark ? 'dark' : 'light');
      return;
    }
    if (event.target.closest('[data-palette-cycle]')) {
      const current = PALETTES.indexOf(root.dataset.palette || 'neutral');
      const next = PALETTES[(current + 1) % PALETTES.length];
      root.dataset.palette = next;
      setCookie('clavium_palette', next);
    }
  });

  // --- Expiry selector ---
  // Resolves "match rotation", "in 90d", a custom duration or a picked date
  // into the plain date input the server reads (`expires_at`).
  const SECONDS = { s: 1, m: 60, h: 3600, d: 86400, w: 604800 };
  const pad = (n) => String(n).padStart(2, '0');
  const isoDate = (d) => `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;
  const parseIso = (value) => (/^\d{4}-\d{2}-\d{2}$/.test(value) ? new Date(`${value}T00:00:00Z`) : null);

  // Months and years clamp to the end of a shorter month (Jan 31 + 1 month = Feb 28).
  const addToDate = (base, n, unit) => {
    const d = new Date(base);
    if (unit === 'd') d.setUTCDate(d.getUTCDate() + n);
    else if (unit === 'w') d.setUTCDate(d.getUTCDate() + 7 * n);
    else {
      const day = d.getUTCDate();
      d.setUTCDate(1);
      if (unit === 'mo') d.setUTCMonth(d.getUTCMonth() + n);
      else d.setUTCFullYear(d.getUTCFullYear() + n);
      const last = new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 0)).getUTCDate();
      d.setUTCDate(Math.min(day, last));
    }
    return d;
  };

  // The same <n><unit> grammar as maxAge, plus mo and y; null when invalid.
  const parseCustom = (text) => {
    const m = /^(\d{1,5})\s*(d|w|mo|y)$/.exec(text.trim().toLowerCase());
    return m ? { n: Number(m[1]), unit: m[2] } : null;
  };

  const setupExpiry = (root) => {
    const modes = root.querySelector('[data-expiry-modes]');
    const presets = root.querySelector('[data-expiry-presets]');
    const customBox = root.querySelector('[data-expiry-custom]');
    const custom = customBox.querySelector('input');
    const dateInput = root.querySelector('input[type="date"]');
    const result = root.querySelector('[data-expiry-result]');
    const baseInput = root.dataset.base ? document.getElementById(root.dataset.base) : null;
    const maxAge = /^(\d{1,5})([smhdw])$/.exec(root.dataset.maxAge || '');
    const customChip = presets.querySelector('[data-custom]');
    const presetChips = Array.from(presets.querySelectorAll('[data-n]'));

    let mode = root.dataset.default === 'match' && maxAge ? 'match' : 'never';
    let chip = presetChips.find((c) => c.dataset.n === '90') ?? presetChips[0];

    const base = () => parseIso(baseInput?.value ?? '') ?? parseIso(root.dataset.today) ?? new Date();
    const describe = (text, ok) => {
      result.replaceChildren();
      if (ok) {
        const check = document.createElement('span');
        check.className = 'text-(--state-ok)';
        check.setAttribute('aria-hidden', 'true');
        check.textContent = '✓';
        const label = document.createElement('span');
        label.append('Expires on ');
        const date = document.createElement('b');
        date.className = 'font-mono font-medium';
        date.textContent = ok;
        label.append(date);
        result.append(check, label);
        if (text) {
          const extra = document.createElement('span');
          extra.className = 'text-muted-foreground';
          extra.textContent = text;
          result.append(extra);
        }
      } else {
        const note = document.createElement('span');
        note.className = 'text-muted-foreground';
        note.textContent = text;
        result.append(note);
      }
    };

    const render = () => {
      modes.querySelectorAll('[data-mode]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.mode === mode)));
      presets.hidden = mode !== 'in';
      presetChips.forEach((c) => c.setAttribute('aria-pressed', String(c === chip)));
      customChip.setAttribute('aria-pressed', String(chip === customChip));
      customBox.hidden = !(mode === 'in' && chip === customChip);
      dateInput.hidden = mode !== 'on';
      custom.setCustomValidity('');

      let resolved = null;
      let extra = '';
      let message = '';
      if (mode === 'match') {
        resolved = addToDate(base(), Math.ceil((Number(maxAge[1]) * SECONDS[maxAge[2]]) / 86400), 'd');
        extra = `· rotation policy max age ${maxAge[1]}${maxAge[2]}`;
      } else if (mode === 'in' && chip !== customChip) {
        resolved = addToDate(base(), Number(chip.dataset.n), chip.dataset.unit);
      } else if (mode === 'in') {
        const parsed = parseCustom(custom.value);
        if (parsed) resolved = addToDate(base(), parsed.n, parsed.unit);
        else {
          message = custom.value.trim() ? 'Not understood. Try 45d, 10w, 8mo or 2y.' : 'Type a duration above.';
          custom.setCustomValidity(message);
        }
      } else if (mode === 'on') {
        resolved = parseIso(dateInput.value);
        if (!resolved) message = 'Pick a date.';
      } else {
        message = 'No expiry. The rotation policy still applies.';
      }

      if (mode !== 'on') dateInput.value = resolved ? isoDate(resolved) : '';
      result.hidden = false;
      describe(message || extra, resolved ? isoDate(resolved) : '');
    };

    modes.addEventListener('click', (event) => {
      const button = event.target.closest('[data-mode]');
      if (!button) return;
      // Carry the current result into the date picker so it can be adjusted.
      mode = button.dataset.mode;
      render();
    });
    presets.addEventListener('click', (event) => {
      const button = event.target.closest('button');
      if (!button) return;
      chip = button.hasAttribute('data-custom') ? customChip : button;
      render();
      if (chip === customChip) custom.focus();
    });
    custom.addEventListener('input', render);
    dateInput.addEventListener('input', render);
    baseInput?.addEventListener('input', render);

    if (!maxAge) modes.querySelector('[data-mode="match"]').remove();
    modes.hidden = false;
    render();
  };

  document.querySelectorAll('[data-expiry]').forEach(setupExpiry);

  // After a successful rotation, clear the key from the form.
  document.addEventListener('rotation-complete', (event) => {
    event.target.closest('form')?.querySelectorAll('input[name="key"]').forEach((i) => { i.value = ''; });
  });

  // Closing a dialog clears any pasted key and previous results.
  document.addEventListener('close', (event) => {
    if (!(event.target instanceof HTMLDialogElement)) return;
    event.target.querySelectorAll('input[name="key"]').forEach((i) => { i.value = ''; });
    const result = event.target.querySelector('#rotate-result');
    if (result) result.replaceChildren();
  }, true);

  // Follow the OS preference until the user picks a theme.
  if (!document.cookie.includes('clavium_theme=') && window.matchMedia('(prefers-color-scheme: dark)').matches) {
    root.classList.add('dark');
  }
})();

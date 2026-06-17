// Amdion Read Mode — the in-page reader.
//
// Lifts the article out of a cluttered page into a calm, full-screen reading
// surface (Readability extraction → a shadow-DOM overlay), so reading on a
// laptop feels closer to a Kindle: one warm column, big serif type, nothing else.
//
// Design choices that matter:
//  • The reader opens entirely in the extension — no round-trip to the Amdion
//    app on the hot path. It still *tells* the app (read_started / read_ended)
//    so the app can do the "wrap" (lock other tabs, log reading time). If the
//    app/bridge is down, you still get the reader; you just lose the wrap.
//  • Rendered in a shadow root so the host page's CSS can't touch it, exactly
//    like the Soft-mode nudge (see content/nudge.js).
//  • Scroll-first, with a soft page-turn (Space / ↓ advances ~90% of a screen).
//    True paginated reflow is a later, opt-in mode; clean scroll never clips an
//    image or shreds a code block, so it's the honest v1.
//
// Heavy `Readability` is only *run* on trigger (the .parse() below); merely
// defining it on every page is cheap. A later optimization can lazy-inject it.

(() => {
  // Real extension context vs. the static preview harness (which loads this file
  // in a page's main world to screenshot the reader without Chrome). In the
  // harness `chrome.runtime.id` is absent, so we never touch chrome.* there.
  const EXT =
    typeof chrome !== "undefined" && chrome.runtime && chrome.runtime.id
      ? chrome
      : null;

  // ── Preferences (the app writes these in Phase 2; we seed sane defaults) ────
  const PREFS_KEY = "reading";
  const DEFAULT_PREFS = {
    theme: "sepia", // sepia | light | dark
    typeface: "serif", // serif | sans
    size: 3, // 1..5
    wpm: 240, // for the "N min left" estimate
    pillEnabled: true, // the quiet in-page "Read" affordance
  };

  // Warm-paper / light / dark palettes. Sepia is the default — it's the whole
  // reason "reading on a tablet feels easier than a laptop".
  const THEMES = {
    sepia: { bg: "#f4ecd9", fg: "#433b2b", muted: "#8a7d63", accent: "#9a6233", line: "rgba(67,59,43,.14)" },
    light: { bg: "#fbfaf8", fg: "#23211e", muted: "#8a857c", accent: "#2480ba", line: "rgba(0,0,0,.10)" },
    dark:  { bg: "#0c0c0d", fg: "#e7e3da", muted: "#8b8b86", accent: "#2480ba", line: "rgba(255,255,255,.12)" },
  };
  // Body font-size per step. Tuned for laptop reading distance.
  const SIZE_PX = { 1: 17, 2: 18.5, 3: 20, 4: 22, 5: 25 };
  // macOS ships gorgeous reading serifs (Iowan Old Style, Charter); we prefer a
  // bundled Literata when present so every site reads identically, then fall back.
  const SERIF = `'Literata','Iowan Old Style','Charter','Palatino',Georgia,'Times New Roman',serif`;
  const SANS = `-apple-system,BlinkMacSystemFont,'Inter','Segoe UI',system-ui,sans-serif`;

  const HOST_URL = location.href;
  const POS_KEY = "amdion_read_pos"; // map of url → scroll fraction (resume)

  // ── Module state ────────────────────────────────────────────────────────
  let prefs = { ...DEFAULT_PREFS };
  let host = null; // the overlay host element (carries the shadow root)
  let shadow = null;
  let scroller = null; // the scrolling article column
  let pill = null; // the quiet "Read" affordance
  let startedAt = 0;
  let maxPct = 0; // furthest-read fraction this session
  let controlsTimer = 0;

  const isOpen = () => !!host;

  // ── Prefs I/O ─────────────────────────────────────────────────────────────
  function loadPrefs() {
    return new Promise((resolve) => {
      if (!EXT) return resolve({ ...DEFAULT_PREFS });
      EXT.storage.local.get([PREFS_KEY], (r) => {
        resolve({ ...DEFAULT_PREFS, ...(r && r[PREFS_KEY]) });
      });
    });
  }
  function savePrefs() {
    if (EXT) EXT.storage.local.set({ [PREFS_KEY]: prefs });
  }

  function loadPos(cb) {
    if (!EXT) return cb(0);
    EXT.storage.local.get([POS_KEY], (r) => cb(((r && r[POS_KEY]) || {})[HOST_URL] || 0));
  }
  function savePos(frac) {
    if (!EXT) return;
    EXT.storage.local.get([POS_KEY], (r) => {
      const map = (r && r[POS_KEY]) || {};
      if (frac > 0.02 && frac < 0.985) map[HOST_URL] = frac;
      else delete map[HOST_URL]; // don't resume at the very top or once finished
      EXT.storage.local.set({ [POS_KEY]: map });
    });
  }

  // ── Telemetry to the app (relayed over the WS bridge by background.js) ──────
  function tell(event, payload) {
    if (!EXT) return;
    try {
      EXT.runtime.sendMessage({ type: "amdion-read-event", event, payload }, () => void chrome.runtime.lastError);
    } catch (_) {}
  }

  // ── Extraction ──────────────────────────────────────────────────────────
  // Readability mutates the document, so always parse a clone. Returns null when
  // there's nothing worth reading (so we can show a calm toast instead of an
  // empty reader).
  function extract() {
    if (typeof Readability === "undefined") return null;
    try {
      const docClone = document.cloneNode(true);
      const art = new Readability(docClone).parse();
      if (!art || !art.content || (art.length || 0) < 250) return null;
      return art;
    } catch (_) {
      return null;
    }
  }

  function wordCount(text) {
    return (text || "").trim().split(/\s+/).filter(Boolean).length;
  }

  // ── The reader surface ────────────────────────────────────────────────────
  function buildReader(art) {
    const words = wordCount(art.textContent);
    const mins = Math.max(1, Math.round(words / (prefs.wpm || 240)));

    host = document.createElement("div");
    host.id = "amdion-reader-host";
    // Cover everything, above any page chrome. all:initial guards against the
    // host page's resets leaking in.
    host.style.cssText =
      "all: initial; position: fixed; inset: 0; z-index: 2147483646; display: block;";
    shadow = host.attachShadow({ mode: "open" });
    shadow.innerHTML = `
      <style>
        :host { --bg:#f4ecd9; --fg:#433b2b; --muted:#8a7d63; --accent:#9a6233;
                --line:rgba(67,59,43,.14); --fs:20px; --ff:${SERIF}; }
        * { box-sizing: border-box; }
        .wrap { position: fixed; inset: 0; background: var(--bg); color: var(--fg);
                font-family: var(--ff); display: flex; flex-direction: column;
                animation: fade .25s ease both; }
        @keyframes fade { from { opacity: 0 } to { opacity: 1 } }

        /* Thin progress bar pinned to the very top. */
        .prog { position: absolute; top: 0; left: 0; height: 3px; width: 0%;
                background: var(--accent); transition: width .12s linear; z-index: 3; }

        /* Auto-hiding control strip. */
        .bar { position: absolute; top: 0; left: 0; right: 0; z-index: 2;
               display: flex; align-items: center; gap: 6px; padding: 12px 18px;
               background: linear-gradient(var(--bg), rgba(0,0,0,0));
               opacity: 0; transition: opacity .2s ease; pointer-events: none; }
        .bar.show { opacity: 1; pointer-events: auto; }
        .bar .mark { font: 600 10px/1 ${SANS}; letter-spacing: .22em; color: var(--accent); }
        .bar .spacer { flex: 1; }
        .bar button { font: 13px ${SANS}; color: var(--fg); background: transparent;
               border: 1px solid var(--line); border-radius: 8px; padding: 5px 10px;
               cursor: pointer; line-height: 1; }
        .bar button:hover { border-color: var(--muted); }
        .bar button.active { background: var(--accent); border-color: var(--accent); color: #fff; }
        .bar .grp { display: flex; gap: 4px; }

        .scroll { flex: 1; overflow-y: auto; scroll-behavior: smooth;
                  scrollbar-width: thin; scrollbar-color: var(--line) transparent; }
        .scroll::-webkit-scrollbar { width: 9px; }
        .scroll::-webkit-scrollbar-thumb { background: var(--line); border-radius: 9px; }

        .col { max-width: 36em; margin: 0 auto; padding: 11vh 24px 28vh;
               font-size: var(--fs); line-height: 1.62; }
        .meta { font: 600 11px/1 ${SANS}; letter-spacing: .14em; text-transform: uppercase;
                color: var(--muted); margin-bottom: 18px; }
        h1.title { font-family: var(--ff); font-weight: 700; font-size: 1.9em;
                   line-height: 1.18; margin: 0 0 .5em; letter-spacing: -0.01em; }
        .byline { font: 14px ${SANS}; color: var(--muted); margin: 0 0 2.4em; }
        /* Justified, hyphenated body — flush both edges for a typeset feel;
           hyphens keep the word-spacing even (needs the content's lang, which
           the shadow tree inherits from the host page). */
        .body { font-size: 1em; text-align: justify; hyphens: auto; -webkit-hyphens: auto; }
        .body p { margin: 0 0 1.15em; }
        /* Headings read better ragged-left, never justified. */
        .body h2, .body h3 { font-family: var(--ff); line-height: 1.3; margin: 1.6em 0 .5em;
               text-align: left; hyphens: none; }
        .body a { color: var(--accent); text-underline-offset: 2px; }
        .body img, .body figure img { max-width: 100%; height: auto; display: block;
                   margin: 1.6em auto; border-radius: 6px; }
        .body figure { margin: 1.6em 0; }
        .body figcaption { font: 13px ${SANS}; color: var(--muted); text-align: center; margin-top: .5em; }
        .body blockquote { margin: 1.4em 0; padding-left: 1em; border-left: 3px solid var(--line);
                   color: var(--muted); font-style: italic; }
        .body pre { background: rgba(127,127,127,.10); padding: 14px 16px; border-radius: 8px;
                   overflow-x: auto; font: 13.5px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace; }
        .body code { font: .9em ui-monospace,SFMono-Regular,Menlo,monospace; }
        .body hr { border: none; border-top: 1px solid var(--line); margin: 2em 0; }
        .done { text-align: center; color: var(--muted); font: 13px ${SANS}; padding: 2em 0 0; }
        .hint { position: absolute; bottom: 16px; left: 0; right: 0; text-align: center;
                font: 12px ${SANS}; color: var(--muted); opacity: 0; transition: opacity .2s; }
        .hint.show { opacity: .7; }
      </style>
      <div class="wrap">
        <div class="prog"></div>
        <div class="bar">
          <span class="mark">AMDION</span>
          <span class="spacer"></span>
          <span class="grp" data-role="theme">
            <button data-theme="sepia" title="Sepia">Sepia</button>
            <button data-theme="light" title="Light">Light</button>
            <button data-theme="dark" title="Dark">Dark</button>
          </span>
          <span class="grp" data-role="face">
            <button data-face="serif" title="Serif">Serif</button>
            <button data-face="sans" title="Sans">Sans</button>
          </span>
          <span class="grp">
            <button data-size="-1" title="Smaller">A−</button>
            <button data-size="1" title="Larger">A+</button>
          </span>
          <button data-act="close" title="Close (Esc)">Done</button>
        </div>
        <div class="scroll">
          <div class="col">
            <div class="meta"></div>
            <h1 class="title"></h1>
            <div class="byline"></div>
            <div class="body"></div>
            <div class="done">· · ·</div>
          </div>
        </div>
        <div class="hint">Space to turn the page · Esc to leave</div>
      </div>`;

    // Fill content. innerHTML in a shadow root never executes <script>, and
    // Readability has already stripped scripts and absolutized URLs.
    const metaTxt = [art.siteName, mins + " min read"].filter(Boolean).join(" · ");
    shadow.querySelector(".meta").textContent = metaTxt;
    shadow.querySelector(".title").textContent = art.title || document.title || "Untitled";
    const by = shadow.querySelector(".byline");
    if (art.byline) by.textContent = art.byline;
    else by.remove();
    shadow.querySelector(".body").innerHTML = art.content;

    scroller = shadow.querySelector(".scroll");
    wireControls();
    applyPrefs();
    return host;
  }

  // ── Live theme / typography ────────────────────────────────────────────────
  function applyPrefs() {
    if (!shadow) return;
    const t = THEMES[prefs.theme] || THEMES.sepia;
    const root = shadow.host;
    root.style.setProperty("--bg", t.bg);
    root.style.setProperty("--fg", t.fg);
    root.style.setProperty("--muted", t.muted);
    root.style.setProperty("--accent", t.accent);
    root.style.setProperty("--line", t.line);
    root.style.setProperty("--fs", (SIZE_PX[prefs.size] || 20) + "px");
    root.style.setProperty("--ff", prefs.typeface === "sans" ? SANS : SERIF);
    // Reflect active state in the toolbar.
    shadow.querySelectorAll("[data-theme]").forEach((b) => b.classList.toggle("active", b.dataset.theme === prefs.theme));
    shadow.querySelectorAll("[data-face]").forEach((b) => b.classList.toggle("active", b.dataset.face === prefs.typeface));
  }

  function wireControls() {
    const bar = shadow.querySelector(".bar");
    const hint = shadow.querySelector(".hint");
    const showBar = () => {
      bar.classList.add("show");
      clearTimeout(controlsTimer);
      controlsTimer = setTimeout(() => bar.classList.remove("show"), 2600);
    };
    shadow.querySelector(".wrap").addEventListener("mousemove", showBar);
    showBar();
    // First-time hint, then fade.
    hint.classList.add("show");
    setTimeout(() => hint.classList.remove("show"), 4000);

    bar.addEventListener("click", (e) => {
      const b = e.target.closest("button");
      if (!b) return;
      if (b.dataset.act === "close") return close();
      if (b.dataset.theme) prefs.theme = b.dataset.theme;
      else if (b.dataset.face) prefs.typeface = b.dataset.face;
      else if (b.dataset.size) prefs.size = clampSize(prefs.size + Number(b.dataset.size));
      applyPrefs();
      savePrefs();
      showBar();
    });

    scroller.addEventListener("scroll", onScroll, { passive: true });
  }

  function clampSize(s) {
    return Math.max(1, Math.min(5, s));
  }

  function onScroll() {
    if (!scroller) return;
    const max = scroller.scrollHeight - scroller.clientHeight;
    const frac = max > 0 ? scroller.scrollTop / max : 0;
    maxPct = Math.max(maxPct, frac);
    shadow.querySelector(".prog").style.width = (frac * 100).toFixed(1) + "%";
  }

  function turn(dir) {
    if (!scroller) return;
    scroller.scrollBy({ top: dir * scroller.clientHeight * 0.9, behavior: "smooth" });
  }

  // ── Open / close ────────────────────────────────────────────────────────
  function open(art) {
    if (isOpen()) return;
    if (pill) pill.remove(), (pill = null);
    buildReader(art);
    document.documentElement.style.overflow = "hidden"; // freeze the page behind
    (document.body || document.documentElement).appendChild(host);
    // Restore the furthest-read position (after layout settles).
    loadPos((frac) => {
      requestAnimationFrame(() => {
        const max = scroller.scrollHeight - scroller.clientHeight;
        if (frac > 0 && max > 0) scroller.scrollTop = frac * max;
        onScroll();
      });
    });
    // Real fullscreen hides the Dock & menu bar for free — but it needs a user
    // gesture, so it can be refused (e.g. hotkey-triggered). The overlay already
    // covers the viewport either way, so a refusal is harmless.
    try {
      const p = host.requestFullscreen && host.requestFullscreen();
      if (p && p.catch) p.catch(() => {});
    } catch (_) {}
    document.addEventListener("keydown", onKey, true);
    // The browser consumes the first Esc to exit fullscreen (the page never sees
    // that keydown), which would otherwise strand the reader windowed-in-tab.
    // Treat leaving fullscreen as leaving Read Mode, so one Esc exits cleanly.
    document.addEventListener("fullscreenchange", onFsChange);
    startedAt = Date.now();
    maxPct = 0;
    tell("started", {
      url: HOST_URL,
      title: art.title || document.title,
      siteName: art.siteName || null,
      wordCount: wordCount(art.textContent),
      estMin: Math.max(1, Math.round(wordCount(art.textContent) / (prefs.wpm || 240))),
    });
  }

  function close() {
    if (!isOpen()) return;
    savePos(maxPct);
    tell("ended", {
      url: HOST_URL,
      secondsRead: Math.round((Date.now() - startedAt) / 1000),
      pctRead: Math.round(maxPct * 100),
    });
    document.removeEventListener("keydown", onKey, true);
    document.removeEventListener("fullscreenchange", onFsChange);
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    document.documentElement.style.overflow = "";
    host.remove();
    host = shadow = scroller = null;
    clearTimeout(controlsTimer);
    maybeShowPill(); // article's still readerable — offer to re-enter
  }

  // Left fullscreen while reading (the Esc the page never received, or a manual
  // exit) → close the reader too, so Read Mode is never left in a windowed limbo.
  function onFsChange() {
    if (!document.fullscreenElement && isOpen()) close();
  }

  function onKey(e) {
    if (!isOpen()) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === " " || e.key === "ArrowDown" || e.key === "PageDown") {
      e.preventDefault();
      turn(e.shiftKey ? -1 : 1);
    } else if (e.key === "ArrowUp" || e.key === "PageUp") {
      e.preventDefault();
      turn(-1);
    } else if (e.key === "+" || e.key === "=") {
      prefs.size = clampSize(prefs.size + 1);
      applyPrefs();
      savePrefs();
    } else if (e.key === "-") {
      prefs.size = clampSize(prefs.size - 1);
      applyPrefs();
      savePrefs();
    }
  }

  // Entry from a trigger (pill, hotkey, or the app). Extract first; if there's
  // nothing to read, say so calmly rather than opening an empty reader.
  function enter() {
    if (isOpen()) return;
    const art = extract();
    if (!art) return toast("Nothing to read on this page.");
    open(art);
  }

  // ── A calm, self-dismissing toast (for the "can't read this" case) ─────────
  function toast(msg) {
    const t = document.createElement("div");
    t.style.cssText =
      "all: initial; position: fixed; bottom: 22px; left: 50%; transform: translateX(-50%);" +
      "z-index: 2147483647; font: 13.5px/1.4 " + SANS + "; color: #f5f5f5;" +
      "background: rgba(18,18,18,.92); border: 1px solid rgba(255,255,255,.14);" +
      "border-radius: 12px; padding: 11px 16px; box-shadow: 0 12px 40px rgba(0,0,0,.5);" +
      "backdrop-filter: blur(12px);";
    t.textContent = msg;
    document.body.appendChild(t);
    setTimeout(() => t.remove(), 2600);
  }

  // ── The quiet "Read" pill ──────────────────────────────────────────────────
  function maybeShowPill() {
    if (!EXT || pill || isOpen() || !prefs.pillEnabled) return;
    if (typeof isProbablyReaderable === "undefined" || !isProbablyReaderable(document)) return;
    pill = document.createElement("div");
    pill.style.cssText = "all: initial; position: fixed; top: 20px; right: 20px; z-index: 2147483645;";
    const sh = pill.attachShadow({ mode: "open" });
    sh.innerHTML = `
      <style>
        .p { display: flex; align-items: center; gap: 8px; cursor: pointer;
             font: 600 13px ${SANS}; color: #f5f5f5; background: rgba(18,18,18,.9);
             border: 1px solid rgba(255,255,255,.14); border-radius: 999px;
             padding: 9px 14px; box-shadow: 0 8px 28px rgba(0,0,0,.4);
             backdrop-filter: blur(10px); opacity: .55; transition: opacity .18s, transform .18s; }
        .p:hover { opacity: 1; transform: translateY(-1px); }
        .dot { font: 600 9px ${SANS}; letter-spacing: .2em; color: #2480ba; }
        /* The entry hotkey, taught right where reading begins. */
        .kbd { font: 600 10px ${SANS}; letter-spacing: .03em; color: #c2c2c2;
               border: 1px solid rgba(255,255,255,.2); border-radius: 6px; padding: 2px 6px; }
        .x { margin-left: 2px; opacity: .6; padding: 0 2px; }
        .x:hover { opacity: 1; }
      </style>
      <div class="p" title="Read this calmly — ⌥⇧R (Amdion)">
        <span class="dot">READ</span>
        <span class="kbd">⌥⇧R</span>
        <span class="x" data-x title="Hide">×</span>
      </div>`;
    sh.querySelector(".p").addEventListener("click", (e) => {
      if (e.target.hasAttribute("data-x")) {
        // Dismiss for this page load only; the toggle in settings turns it off
        // for good.
        pill.remove();
        pill = null;
        return;
      }
      enter();
    });
    document.body.appendChild(pill);
  }

  // ── Wiring ────────────────────────────────────────────────────────────────
  function boot() {
    loadPrefs().then((p) => {
      prefs = p;
      maybeShowPill();
    });
    // Triggers from the hotkey (chrome.commands) and the app ("Read this tab")
    // both arrive as a runtime message routed by background.js to this tab.
    EXT.runtime.onMessage.addListener((msg) => {
      if (!msg) return;
      if (msg.type === "amdion-read-enter") enter();
      else if (msg.type === "amdion-read-exit") close();
    });
    // Live-apply preference changes made from the Amdion panel.
    EXT.storage.onChanged.addListener((changes, area) => {
      if (area !== "local" || !changes[PREFS_KEY]) return;
      prefs = { ...DEFAULT_PREFS, ...changes[PREFS_KEY].newValue };
      if (isOpen()) applyPrefs();
      else maybeShowPill();
    });
    // Don't leave a dangling reading session if the tab navigates/closes mid-read.
    window.addEventListener("beforeunload", () => {
      if (isOpen()) {
        savePos(maxPct);
        tell("ended", {
          url: HOST_URL,
          secondsRead: Math.round((Date.now() - startedAt) / 1000),
          pctRead: Math.round(maxPct * 100),
        });
      }
    });
  }

  // Boot: real extension, or the static preview harness (no chrome.*).
  if (EXT) {
    boot();
  } else if (typeof window !== "undefined" && window.__AMDION_READER_PREVIEW__) {
    const pv = window.__AMDION_READER_PREVIEW__;
    prefs = { ...DEFAULT_PREFS, ...(pv.prefs || {}) };
    open(pv.article);
  }
})();

// Amdion capture — the low-friction "save a highlight" affordance on normal web
// pages. Select some text and a calm chip appears near the selection; one click
// files the quote as an Amdion Note (kind: highlight) with the page url + title.
//
// This is the in-page half of the Attention layer's capture capability. The
// other half is permission-free viewport screenshots driven from background.js
// (the ⌃⇧C command / the panel button), which also cover Chrome's PDF viewer —
// where no content script, including this one, can run. Rendered in a shadow
// root so the host page's CSS can't touch it (same pattern as content/nudge.js).

(() => {
  const EXT =
    typeof chrome !== "undefined" && chrome.runtime && chrome.runtime.id ? chrome : null;
  if (!EXT) return;
  if (window.top !== window) return; // top frame only — skip iframes

  const MIN_CHARS = 8;
  let chip = null;

  function removeChip() {
    if (chip) {
      chip.remove();
      chip = null;
    }
  }

  function save(text) {
    try {
      EXT.runtime.sendMessage(
        {
          type: "amdion-capture",
          payload: {
            kind: "highlight",
            source: "web",
            text,
            url: location.href,
            title: document.title,
          },
        },
        () => void chrome.runtime.lastError
      );
    } catch (_) {}
  }

  function showChip(x, y, text) {
    removeChip();
    chip = document.createElement("div");
    chip.id = "amdion-capture-host";
    // Anchored above the selection; the inner card translates onto the point.
    chip.style.cssText = `all: initial; position: fixed; left: ${x}px; top: ${y}px; z-index: 2147483646;`;
    const sh = chip.attachShadow({ mode: "open" });
    sh.innerHTML = `
      <style>
        .c { position: relative; transform: translate(-50%, calc(-100% - 8px));
          display: flex; align-items: center; gap: 8px; cursor: pointer; white-space: nowrap;
          font: 600 12px -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif; color: #f5f5f5;
          background: rgba(18,18,18,.94); border: 1px solid rgba(255,255,255,.16);
          border-radius: 10px; padding: 7px 12px; box-shadow: 0 8px 28px rgba(0,0,0,.45);
          backdrop-filter: blur(10px); animation: pop .14s ease both; }
        @keyframes pop { from { opacity: 0 } to { opacity: 1 } }
        .dot { font: 600 9px sans-serif; letter-spacing: .2em; color: #2480ba; }
        .c:hover { border-color: rgba(255,255,255,.32); }
      </style>
      <div class="c" role="button" aria-label="Save highlight to Amdion">
        <span class="dot">AMDION</span><span class="lbl">Save highlight</span>
      </div>`;
    // mousedown (not click) so we act before the selection is cleared, and stop
    // it propagating so the document mousedown handler doesn't dismiss us first.
    sh.querySelector(".c").addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      save(text);
      sh.querySelector(".lbl").textContent = "Saved ✓";
      setTimeout(removeChip, 750);
    });
    document.documentElement.appendChild(chip);
  }

  // After a selection gesture, offer the chip if there's a real text range.
  document.addEventListener("mouseup", () => {
    setTimeout(() => {
      const sel = window.getSelection();
      const text = sel ? String(sel).trim() : "";
      if (text.length < MIN_CHARS) return; // ignore clicks / tiny selections
      const range = sel.rangeCount ? sel.getRangeAt(0) : null;
      const rect = range ? range.getBoundingClientRect() : null;
      if (!rect || (!rect.width && !rect.height)) return;
      showChip(rect.left + rect.width / 2, rect.top, text);
    }, 10);
  });

  // Dismiss on scroll or any click that isn't the chip itself (events from
  // inside the shadow root retarget to the host element, so this comparison
  // holds — and the chip's own handler stops propagation anyway).
  document.addEventListener("scroll", removeChip, { passive: true });
  document.addEventListener("mousedown", (e) => {
    if (chip && e.target !== chip) removeChip();
  });
})();

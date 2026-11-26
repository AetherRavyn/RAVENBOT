/*
 * RAVENBOT demonstration recorder.
 *
 * Injected into the browser dock's remote page by the Rust shell. It has no
 * Tauri IPC access and writes nothing to page storage: it keeps a bounded event
 * buffer in the page for Rust to poll. Rust keeps the authoritative log and
 * re-injects this recorder after navigation.
 *
 * Secrets are excluded before an event exists:
 *   - password and file inputs;
 *   - autocomplete current-password / new-password / one-time-code / cc-*;
 *   - names, ids and labels containing password/passcode/otp/token/secret/
 *     card-number markers;
 *   - anything inside a password-manager card.
 * A sensitive fill keeps its selector and label but never its value.
 */
(function () {
  "use strict";
  var KEY = "__ravenbotDemo";
  var VERSION = 1;
  var MAX_ACTIONS = 200;
  var MAX_MS = 10 * 60 * 1000;
  var FILL_BURST_MS = 3000;
  var MAX_TEXT = 2000;
  var INTERACTIVE =
    "a,button,input,select,textarea,[role=button],[role=link],[contenteditable]";
  var SENSITIVE_WORDS =
    /(password|passwd|passcode|one[-_ ]?time|otp|token|secret|credit[-_ ]?card|card[-_ ]?number|cvv|cvc|security[-_ ]?code)/i;
  var PM_WORDS = /(1password|lastpass|bitwarden|dashlane|keeper|password[-_ ]?manager|passwords)/i;
  var TEXT_INPUT_TYPES = {
    text: true,
    search: true,
    email: true,
    url: true,
    tel: true,
    number: true,
    date: true,
    "datetime-local": true,
    month: true,
    week: true,
    time: true,
    range: true,
    color: true,
  };

  if (window[KEY] && window[KEY].version === VERSION) {
    return;
  }

  var state = {
    active: false,
    startedAt: 0,
    startUrl: "",
    pageId: "",
    events: [],
    truncated: false,
    reason: null,
    sensitiveSkipped: 0,
    lastUrl: "",
    lastFill: null,
    lastClick: null,
    timer: null,
    attached: false,
  };
  var seq = 0;

  function now() {
    return Date.now();
  }

  function clean(value, max) {
    return String(value == null ? "" : value)
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, max || 160);
  }

  function newPageId() {
    return (
      now().toString(36) + Math.random().toString(36).slice(2, 8)
    );
  }

  function isFileInput(el) {
    return el && el.tagName === "INPUT" && (el.type || "").toLowerCase() === "file";
  }

  function inPasswordManager(el) {
    var node = el;
    while (node && node.nodeType === 1) {
      if (node.hasAttribute &&
          (node.hasAttribute("data-1p-ignore") ||
           node.hasAttribute("data-lastpass-icon-root") ||
           node.hasAttribute("data-dashlane"))) {
        return true;
      }
      var marker =
        String(node.className || "") +
        " " +
        String(node.id || "") +
        " " +
        String(node.getAttribute && node.getAttribute("aria-label") || "");
      if (PM_WORDS.test(marker)) return true;
      node = node.parentElement;
    }
    return false;
  }

  function sensitiveReason(el) {
    if (!el || el.nodeType !== 1) return null;
    if (el.tagName === "INPUT") {
      var type = (el.getAttribute("type") || "text").toLowerCase();
      if (type === "password") return "password";
      if (type === "file") return "file";
    }
    var autocomplete = (el.getAttribute("autocomplete") || "").toLowerCase();
    if (/(current-password|new-password|one-time-code|^cc-| cc-)/.test(autocomplete)) {
      return "autocomplete";
    }
    var haystack = [
      el.getAttribute("name"),
      el.getAttribute("id"),
      el.getAttribute("aria-label"),
      el.getAttribute("placeholder"),
      el.getAttribute("data-testid"),
    ]
      .filter(Boolean)
      .join(" ");
    if (SENSITIVE_WORDS.test(haystack)) return "field-name";
    if (inPasswordManager(el)) return "password-manager";
    return null;
  }

  function cssEscape(value) {
    if (window.CSS && typeof window.CSS.escape === "function") {
      return window.CSS.escape(String(value));
    }
    return String(value).replace(/([^a-zA-Z0-9_-])/g, "\\$1");
  }

  function attrEscape(value) {
    return String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  }

  function attrSelector(name, value) {
    return "[" + name + '="' + attrEscape(value) + '"]';
  }

  function stableId(el) {
    if (!el.getAttribute) return null;
    var id = el.getAttribute("id");
    if (!id || id.length > 60) return null;
    if (!/^[A-Za-z][A-Za-z0-9_:.-]*$/.test(id)) return null;
    if (/:r[0-9a-z]+:/i.test(id)) return null;
    if (/[0-9a-f]{8,}/i.test(id)) return null;
    return id;
  }

  function testId(el) {
    if (!el.getAttribute) return null;
    return (
      el.getAttribute("data-testid") || el.getAttribute("data-test") || null
    );
  }

  function cssPath(el) {
    var parts = [];
    var node = el;
    var depth = 0;
    while (node && node.nodeType === 1 && node !== document.documentElement && depth < 6) {
      var id = stableId(node);
      if (id) {
        parts.unshift("#" + cssEscape(id));
        break;
      }
      var test = testId(node);
      if (test) {
        parts.unshift(attrSelector("data-testid", test));
        break;
      }
      var part = node.tagName.toLowerCase();
      var parent = node.parentElement;
      if (parent) {
        var same = [];
        for (var i = 0; i < parent.children.length; i += 1) {
          if (parent.children[i].tagName === node.tagName) same.push(parent.children[i]);
        }
        if (same.length > 1) {
          part += ":nth-of-type(" + (same.indexOf(node) + 1) + ")";
        }
      }
      parts.unshift(part);
      node = parent;
      depth += 1;
    }
    return parts.join(" > ");
  }

  function selectorCandidates(el) {
    var candidates = [];
    var test = testId(el);
    if (test) candidates.push(attrSelector("data-testid", test));
    var id = stableId(el);
    if (id) candidates.push("#" + cssEscape(id));
    var name = el.getAttribute && el.getAttribute("name");
    if (name) candidates.push(attrSelector("name", name));
    var aria = el.getAttribute && el.getAttribute("aria-label");
    if (aria) candidates.push(attrSelector("aria-label", aria));
    var placeholder = el.getAttribute && el.getAttribute("placeholder");
    if (placeholder) candidates.push(attrSelector("placeholder", placeholder));
    var path = cssPath(el);
    if (path) candidates.push(path);

    var unique = [];
    for (var i = 0; i < candidates.length; i += 1) {
      if (candidates[i] && unique.indexOf(candidates[i]) === -1) {
        unique.push(candidates[i]);
      }
    }
    return unique.slice(0, 4);
  }

  function labelFor(el, sensitive) {
    if (!el || el.nodeType !== 1) return "";
    var aria = el.getAttribute && el.getAttribute("aria-label");
    if (aria) return clean(aria, 160);
    var tag = el.tagName;
    if (!sensitive && (tag === "BUTTON" || tag === "A")) {
      var text = clean(el.innerText || el.textContent || "", 160);
      if (text) return text;
    }
    var name = el.getAttribute && el.getAttribute("name");
    if (name) return clean(name, 160);
    var placeholder = el.getAttribute && el.getAttribute("placeholder");
    if (placeholder) return clean(placeholder, 160);
    if (el.getAttribute) {
      var id = el.getAttribute("id");
      if (id) {
        try {
          var label = document.querySelector('label[for="' + attrEscape(id) + '"]');
          if (label && !sensitive) return clean(label.innerText || label.textContent, 160);
        } catch (error) {
          /* an id with a quote is not worth a broken selector */
        }
      }
    }
    if (el.closest) {
      var wrapper = el.closest("label");
      if (wrapper && !sensitive) {
        return clean(wrapper.innerText || wrapper.textContent, 160);
      }
    }
    if (sensitive) return "";
    if (el.tagName === "INPUT" && (el.type === "submit" || el.type === "button")) {
      return clean(el.value, 160);
    }
    return clean(el.innerText || el.textContent || "", 160);
  }

  function nearestInteractive(target) {
    if (!target || target.nodeType !== 1) return null;
    if (target.closest && target.closest("#__ravenbot_dock_toolbar")) return null;
    var control = null;
    if (target.tagName === "LABEL") {
      control =
        target.control ||
        (target.htmlFor && document.getElementById(target.htmlFor)) ||
        (target.querySelector &&
          target.querySelector("input,select,textarea,button,a,[contenteditable]"));
      if (control) return control;
    }
    var el = target.closest ? target.closest(INTERACTIVE) : null;
    if (!el) return null;
    if (el.hasAttribute("contenteditable") && el.getAttribute("contenteditable") === "false") {
      return null;
    }
    return el;
  }

  function editableOf(target) {
    if (!target || target.nodeType !== 1) return null;
    if (target.isContentEditable) return target;
    if (target.closest) {
      var el = target.closest('[contenteditable="true"],[contenteditable=""]');
      if (el) return el;
    }
    return null;
  }

  function readText(el) {
    if (!el) return "";
    var tag = el.tagName;
    if (tag === "SELECT") return String(el.value == null ? "" : el.value).slice(0, MAX_TEXT);
    if (tag === "INPUT" || tag === "TEXTAREA") {
      return String(el.value == null ? "" : el.value).slice(0, MAX_TEXT);
    }
    return String(el.innerText || el.textContent || "").slice(0, MAX_TEXT);
  }

  function warnings() {
    var out = [];
    if (state.truncated || state.reason === "actions") {
      out.push(
        "Recording stopped after " +
          MAX_ACTIONS +
          " actions; later actions were not captured."
      );
    }
    if (state.reason === "time") {
      out.push(
        "Recording reached the 10-minute limit; later actions were not captured."
      );
    }
    if (state.sensitiveSkipped > 0) {
      out.push(
        state.sensitiveSkipped +
          " sensitive field value(s) were not recorded; replay will ask for them."
      );
    }
    return out;
  }

  function status(after) {
    var start =
      typeof after === "number" && after >= 0
        ? Math.min(Math.floor(after), state.events.length)
        : 0;
    return {
      ok: true,
      recording: state.active,
      stopped: !!state.reason,
      reason: state.reason,
      count: state.events.length,
      elapsed_ms:
        state.startedAt > 0 ? Math.max(0, now() - state.startedAt) : 0,
      truncated: state.truncated,
      sensitive_omitted: state.sensitiveSkipped,
      warnings: warnings(),
      url: location.href,
      events: state.events.slice(start),
    };
  }

  function clearTimer() {
    if (state.timer) {
      window.clearTimeout(state.timer);
      state.timer = null;
    }
  }

  function armTimer() {
    clearTimer();
    var remaining = state.startedAt + MAX_MS - now();
    state.timer = window.setTimeout(function () {
      if (state.active) stopInternal("time");
    }, Math.max(0, remaining));
  }

  function pushEvent(event) {
    if (state.events.length >= MAX_ACTIONS) {
      state.truncated = true;
      stopInternal("actions");
      return null;
    }
    state.events.push(event);
    if (state.events.length >= MAX_ACTIONS) {
      state.truncated = true;
      stopInternal("actions");
    }
    return event;
  }

  function maybeNavigate(url) {
    if (!url || url === state.lastUrl) return;
    pushEvent({
      id: state.pageId + "-nav-" + ++seq,
      kind: "navigate",
      at: now(),
      url: url,
    });
    state.lastUrl = url;
  }

  function clickableInput(el) {
    if (el.tagName !== "INPUT") return true;
    var type = (el.type || "text").toLowerCase();
    return (
      type === "button" ||
      type === "submit" ||
      type === "reset" ||
      type === "checkbox" ||
      type === "radio" ||
      type === "image"
    );
  }

  function recordAction(kind, el, extra) {
    if (!state.active) return null;
    if (!el || el.nodeType !== 1) return null;
    if (isFileInput(el)) return null;
    var url = location.href;
    maybeNavigate(url);
    if (!state.active) return null;
    var candidates = selectorCandidates(el);
    var secretField = !!sensitiveReason(el);
    var sensitive = !!(extra && extra.sensitive) || secretField;
    var action = {
      id: state.pageId + "-" + ++seq,
      kind: kind,
      at: now(),
      url: url,
      selector: candidates[0] || null,
      fallbacks: candidates.slice(1, 4),
      label: labelFor(el, sensitive) || null,
      text: extra && typeof extra.text === "string" ? extra.text : null,
      key: extra && extra.key ? extra.key : null,
      sensitive: sensitive,
      checked:
        extra && typeof extra.checked === "boolean" ? extra.checked : null,
    };
    pushEvent(action);
    state.lastFill = null;
    return action;
  }

  function recordFill(el, sensitive) {
    if (!state.active) return null;
    if (!el || el.nodeType !== 1) return null;
    if (isFileInput(el)) return null;
    var url = location.href;
    maybeNavigate(url);
    if (!state.active) return null;
    var candidates = selectorCandidates(el);
    var key =
      (candidates[0] || "") + "|" + url + "|" + (sensitive ? "secret" : "text");
    var at = now();
    var text = sensitive ? null : readText(el);
    if (
      state.lastFill &&
      state.lastFill.key === key &&
      at - state.lastFill.at <= FILL_BURST_MS
    ) {
      var previous = state.events[state.lastFill.index];
      if (previous && previous.kind === "fill") {
        previous.at = at;
        previous.text = text;
        previous.label = labelFor(el, sensitive) || null;
        previous.sensitive = sensitive;
        state.lastFill.at = at;
        return previous;
      }
    }
    if (sensitive) state.sensitiveSkipped += 1;
    var event = {
      id: state.pageId + "-" + ++seq,
      kind: "fill",
      at: at,
      url: url,
      selector: candidates[0] || null,
      fallbacks: candidates.slice(1, 4),
      label: labelFor(el, sensitive) || null,
      text: text,
      key: null,
      sensitive: sensitive,
      checked: null,
    };
    if (state.events.length >= MAX_ACTIONS) {
      state.truncated = true;
      stopInternal("actions");
      return null;
    }
    state.events.push(event);
    state.lastFill = { key: key, index: state.events.length - 1, at: at };
    if (state.events.length >= MAX_ACTIONS) {
      state.truncated = true;
      stopInternal("actions");
    }
    return event;
  }

  function onClick(event) {
    if (!state.active) return;
    // A real browser forwards a label click to its control, which then emits
    // its own click. Recording the label as a second click would toggle a
    // checkbox twice on replay.
    if (event.target && event.target.tagName === "LABEL") return;
    var el = nearestInteractive(event.target);
    if (!el || isFileInput(el)) return;
    if (!clickableInput(el)) return;
    if (el.tagName === "SELECT") return;
    var at = now();
    if (state.lastClick && state.lastClick.el === el && at - state.lastClick.at < 500) {
      return;
    }
    state.lastClick = { el: el, at: at };
    var checked =
      el.tagName === "INPUT" && (el.type === "checkbox" || el.type === "radio")
        ? !!el.checked
        : null;
    recordAction("click", el, { checked: checked });
  }

  function onInput(event) {
    if (!state.active) return;
    var target = event.target;
    if (!target || target.nodeType !== 1) return;
    if (isFileInput(target)) return;
    var tag = target.tagName;
    var type = (target.getAttribute("type") || "text").toLowerCase();
    if (tag === "INPUT" && (type === "checkbox" || type === "radio" || type === "hidden")) {
      return;
    }
    var editable = editableOf(target);
    if (editable) {
      recordFill(editable, !!sensitiveReason(editable));
      return;
    }
    if (tag === "SELECT" || tag === "TEXTAREA") {
      recordFill(target, !!sensitiveReason(target));
      return;
    }
    if (tag === "INPUT") {
      if (!TEXT_INPUT_TYPES[type] && type !== "password") return;
      recordFill(target, type === "password" || !!sensitiveReason(target));
    }
  }

  function onKeyDown(event) {
    if (!state.active) return;
    var key = event.key;
    if (key !== "Enter" && key !== "Escape" && key !== "Tab") return;
    var el = nearestInteractive(event.target);
    if (!el) {
      var active = document.activeElement;
      if (
        active &&
        active !== document.body &&
        active !== document.documentElement &&
        active.nodeType === 1
      ) {
        el = active;
      }
    }
    if (!el || el === document.body || el === document.documentElement) return;
    recordAction("press", el, { key: key });
  }

  function attach() {
    if (state.attached) return;
    state.attached = true;
    document.addEventListener("click", onClick, true);
    document.addEventListener("input", onInput, true);
    document.addEventListener("change", onInput, true);
    document.addEventListener("keydown", onKeyDown, true);
  }

  function stopInternal(reason) {
    state.active = false;
    if (!state.reason) state.reason = reason;
    clearTimer();
  }

  function start(options) {
    options = options || {};
    if (state.active && !options.fresh) return status();
    if (state.active && options.fresh) {
      state.active = false;
      clearTimer();
    }
    // A new page has a new JavaScript context. There is deliberately no
    // sessionStorage mirror: writing form values into the page's storage would
    // expose them to the site's own scripts. Rust keeps the authoritative log
    // and re-injects this recorder after navigation.
    state.events = [];
    state.startedAt = options.started_at || now();
    state.startUrl = options.start_url || location.href;
    state.pageId = newPageId();
    state.truncated = false;
    state.reason = null;
    state.sensitiveSkipped = 0;
    state.lastUrl = "";
    state.lastFill = null;
    state.lastClick = null;
    state.active = true;
    maybeNavigate(state.startUrl);
    attach();
    armTimer();
    return status();
  }

  function stop(options) {
    options = options || {};
    clearTimer();
    if (state.active) {
      state.active = false;
      state.reason = "stopped";
    }
    var snapshot = status();
    snapshot.recording = false;
    if (options.clear) {
      state.events = [];
      state.startedAt = 0;
      state.startUrl = "";
      state.reason = null;
      state.truncated = false;
      state.sensitiveSkipped = 0;
      state.lastFill = null;
      state.lastClick = null;
      snapshot.count = 0;
      snapshot.events = [];
      snapshot.truncated = false;
      snapshot.warnings = [];
      snapshot.sensitive_omitted = 0;
    }
    return snapshot;
  }

  window[KEY] = {
    version: VERSION,
    start: start,
    status: status,
    stop: stop,
  };
})();

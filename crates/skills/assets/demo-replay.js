/*
 * RAVENBOT demonstration replay helper.
 *
 * Runs in the browser dock's remote page. Finds one control for one recorded
 * action and returns a result object; it never persists or rewrites a skill.
 * The selector is tried first, then the recorded fallbacks, then the human
 * label (aria-label, name/id, <label> text, visible text).
 */
(function () {
  "use strict";
  var KEY = "__ravenbotReplay";
  var VERSION = 1;
  var INTERACTIVE =
    'a,button,input,select,textarea,[role="button"],[role="link"],[contenteditable="true"],[contenteditable=""]';

  if (window[KEY] && window[KEY].version === VERSION) {
    return;
  }

  function clean(value, max) {
    return String(value == null ? "" : value)
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, max || 160);
  }

  function textOf(el) {
    if (!el) return "";
    if (el.tagName === "INPUT" && (el.type === "submit" || el.type === "button")) {
      return clean(el.value);
    }
    return clean(el.innerText || el.textContent || "");
  }

  function visible(el) {
    try {
      var rect = el.getBoundingClientRect();
      var style = window.getComputedStyle(el);
      if (style.visibility === "hidden" || style.display === "none") return false;
      return rect.width > 0 || rect.height > 0 || el === document.activeElement;
    } catch (error) {
      return true;
    }
  }

  function interactive() {
    return Array.prototype.slice.call(document.querySelectorAll(INTERACTIVE));
  }

  function findByLabel(label) {
    if (!label) return null;
    var candidates = interactive();
    var i;
    for (i = 0; i < candidates.length; i += 1) {
      if (candidates[i].getAttribute("aria-label") === label) {
        return { el: candidates[i], matched_by: "aria-label" };
      }
    }
    for (i = 0; i < candidates.length; i += 1) {
      if (
        candidates[i].getAttribute("name") === label ||
        candidates[i].getAttribute("id") === label
      ) {
        return { el: candidates[i], matched_by: "name" };
      }
    }
    var labels = Array.prototype.slice.call(document.querySelectorAll("label"));
    for (i = 0; i < labels.length; i += 1) {
      if (textOf(labels[i]) === label) {
        var control =
          labels[i].control ||
          (labels[i].htmlFor && document.getElementById(labels[i].htmlFor)) ||
          labels[i].querySelector("input,select,textarea,button,a,[contenteditable]");
        if (control) return { el: control, matched_by: "label" };
      }
    }
    for (i = 0; i < candidates.length; i += 1) {
      if (textOf(candidates[i]) === label) {
        return { el: candidates[i], matched_by: "text" };
      }
    }
    for (i = 0; i < candidates.length; i += 1) {
      var text = textOf(candidates[i]);
      if (text && (text.indexOf(label) !== -1 || label.indexOf(text) !== -1)) {
        return { el: candidates[i], matched_by: "text" };
      }
    }
    return null;
  }

  function setNativeValue(el, value) {
    if (el.tagName === "SELECT") {
      var match = null;
      var options = Array.prototype.slice.call(el.options || []);
      for (var i = 0; i < options.length; i += 1) {
        if (options[i].value === value || clean(options[i].textContent) === value) {
          match = options[i];
          break;
        }
      }
      if (!match) {
        return { ok: false, error: "The select no longer offers the recorded option." };
      }
      el.value = match.value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return { ok: true };
    }
    if (el.isContentEditable) {
      el.focus();
      el.textContent = value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      return { ok: true };
    }
    var proto =
      el.tagName === "TEXTAREA"
        ? window.HTMLTextAreaElement && window.HTMLTextAreaElement.prototype
        : window.HTMLInputElement && window.HTMLInputElement.prototype;
    var setter = proto && Object.getOwnPropertyDescriptor(proto, "value");
    el.focus();
    if (setter && setter.set) setter.set.call(el, value);
    else el.value = value;
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
    return { ok: true };
  }

  function press(el, key) {
    var target = el || document.activeElement || document.body;
    if (!target) {
      return { ok: false, error: "The page has no focusable target for the key." };
    }
    try {
      target.focus();
    } catch (error) {
      /* a non-focusable body is still a valid event target */
    }
    var init = {
      key: key,
      bubbles: true,
      cancelable: true,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      metaKey: false,
    };
    var allowed = target.dispatchEvent(new KeyboardEvent("keydown", init));
    target.dispatchEvent(new KeyboardEvent("keypress", init));
    target.dispatchEvent(new KeyboardEvent("keyup", init));
    var submitted = false;
    if (key === "Enter" && target.form && allowed) {
      try {
        var valid =
          typeof target.form.checkValidity !== "function" ||
          target.form.checkValidity();
        if (valid) {
          target.form.requestSubmit();
          submitted = true;
        }
      } catch (error) {
        /* older pages may not have requestSubmit */
      }
    }
    return { ok: true, synthetic: true, submitted_form: submitted };
  }

  function run(action) {
    action = action || {};
    var attempted = [];
    var selectors = [];
    if (action.selector) selectors.push(action.selector);
    (action.fallbacks || []).forEach(function (selector) {
      if (selector && selectors.indexOf(selector) === -1) selectors.push(selector);
    });

    var el = null;
    var matchedBy = null;
    var matchedSelector = null;
    for (var i = 0; i < selectors.length; i += 1) {
      attempted.push(selectors[i]);
      try {
        var found = document.querySelector(selectors[i]);
        if (found) {
          el = found;
          matchedBy = i === 0 ? "selector" : "fallback";
          matchedSelector = selectors[i];
          break;
        }
      } catch (error) {
        attempted.push("invalid:" + selectors[i]);
      }
    }

    if (!el && action.label) {
      attempted.push("label:" + action.label);
      var byLabel = findByLabel(action.label);
      if (byLabel) {
        el = byLabel.el;
        matchedBy = byLabel.matched_by;
        matchedSelector = null;
      }
    }

    if (!el && action.kind === "press") {
      el = document.activeElement || document.body;
      matchedBy = "active";
    }

    if (!el) {
      return {
        ok: false,
        matched_by: null,
        selector: null,
        page_url: location.href,
        error:
          "No element matched the recorded selector, its fallbacks, or the recorded label.",
        attempted: attempted,
      };
    }

    var base = {
      ok: true,
      matched_by: matchedBy,
      selector: matchedSelector,
      page_url: location.href,
      attempted: attempted,
      visible: visible(el),
    };

    try {
      if (action.kind === "click") {
        if (typeof el.scrollIntoView === "function") {
          el.scrollIntoView({ block: "center", inline: "center" });
        }
        if (
          el.tagName === "INPUT" &&
          (el.type === "checkbox" || el.type === "radio") &&
          typeof action.checked === "boolean"
        ) {
          if (el.checked !== action.checked) el.click();
        } else {
          el.click();
        }
        base.page_url = location.href;
        return base;
      }

      if (action.kind === "fill") {
        if (action.sensitive) {
          base.ok = false;
          base.needs_input = true;
          base.error =
            "This field needs your input; the demonstration did not record its value.";
          return base;
        }
        if (typeof action.text !== "string") {
          base.ok = false;
          base.error = "The demonstration has no recorded value for this field.";
          return base;
        }
        if (typeof el.scrollIntoView === "function") {
          el.scrollIntoView({ block: "center", inline: "center" });
        }
        var filled = setNativeValue(el, action.text);
        if (!filled.ok) {
          base.ok = false;
          base.error = filled.error;
          return base;
        }
        base.page_url = location.href;
        return base;
      }

      if (action.kind === "press") {
        if (!action.key) {
          base.ok = false;
          base.error = "The demonstration has no recorded key for this press.";
          return base;
        }
        var pressed = press(el, action.key);
        if (!pressed.ok) {
          base.ok = false;
          base.error = pressed.error;
          return base;
        }
        base.synthetic = true;
        base.submitted_form = !!pressed.submitted_form;
        base.page_url = location.href;
        return base;
      }

      base.ok = false;
      base.error = "The demonstration contains an unsupported action.";
      return base;
    } catch (error) {
      base.ok = false;
      base.error = String((error && error.message) || error);
      return base;
    }
  }

  window[KEY] = { version: VERSION, run: run };
})();

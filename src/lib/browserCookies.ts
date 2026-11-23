/**
 * Conservative cookie-banner rejection.
 *
 * The script only clicks an exact reject-style label inside a short consent
 * banner/dialog that mentions cookies, consent, privacy, or tracking. It never
 * clicks accept buttons and never scans the whole page for a bare "Reject".
 * The same string is used by the manual dock button and the opt-in automatic
 * path, so both behave identically.
 */
export const COOKIE_REJECT_SCRIPT = `(() => {
  const rejectLabels = [
    "reject all", "reject all cookies", "reject cookies", "reject",
    "decline", "decline all", "deny", "refuse", "refuse all",
    "only necessary", "only necessary cookies", "necessary only",
    "essential only", "only essential", "only essential cookies",
    "continue without accepting", "do not accept",
    "alle ablehnen", "ablehnen", "nur notwendige", "nur notwendige cookies",
    "nur essenzielle", "nicht akzeptieren", "ohne akzeptieren fortfahren",
    "tout refuser", "refuser", "refuser tout", "uniquement nécessaires",
    "uniquement les nécessaires", "continuer sans accepter",
    "rechazar todo", "rechazar todas", "rechazar", "solo necesarias",
    "solo las necesarias", "continuar sin aceptar",
    "すべて拒否", "拒否", "必須のみ", "必要なもののみ", "同意しない",
    "全部拒绝", "拒绝全部", "拒绝", "仅必要", "仅必需", "不同意"
  ];
  const consentWords = [
    "cookie", "cookies", "consent", "privacy", "gdpr", "tracking",
    "legitimate interest", "data protection", "datenschutz", "privatsphäre",
    "consentimiento", "confidentialité", "クッキー", "同意", "隐私", "隱私"
  ];
  const normalize = (value) => (value || "").replace(/\\s+/g, " ").trim().toLowerCase();
  const visible = (el) => {
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width > 1 && rect.height > 1 &&
      style.visibility !== "hidden" && style.display !== "none" &&
      style.opacity !== "0";
  };
  const textOf = (el) => normalize(
    el.innerText || el.textContent || el.value || el.getAttribute("aria-label") || ""
  );
  const isReject = (el) => {
    const text = textOf(el);
    return rejectLabels.some((label) => text === label);
  };
  const isConsentContext = (el) => {
    const container = el.closest(
      '[role="dialog"], [aria-modal="true"], [class*="cookie" i], [id*="cookie" i], ' +
      '[class*="consent" i], [id*="consent" i], [class*="banner" i], [id*="banner" i]'
    );
    const context = container || el.parentElement?.parentElement || el.parentElement;
    if (!context) return false;
    const text = textOf(context);
    return text.length > 0 && text.length < 1200 &&
      consentWords.some((word) => text.includes(word));
  };
  const candidates = [
    ...document.querySelectorAll(
      'button, [role="button"], input[type="button"], input[type="submit"], a'
    )
  ];
  const match = candidates.find(
    (el) => visible(el) && isReject(el) && isConsentContext(el)
  );
  if (!match) return { ok: true, clicked: false, reason: "no-banner" };
  const label = textOf(match).slice(0, 80);
  match.click();
  return { ok: true, clicked: true, label };
})()`;

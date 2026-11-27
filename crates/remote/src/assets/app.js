/* RAVENBOT remote — vanilla JS, no third-party code, no innerHTML for data. */
(() => {
  "use strict";

  const TOKEN_KEY = "ravenbot.remote.token";
  const NAME_KEY = "ravenbot.remote.name";
  const POLL_MS = 3000;
  const SSO_ERROR =
    "Single sign-on could not be completed. Use your RAVENBOT username and password, or try again.";

  const state = {
    token: localStorage.getItem(TOKEN_KEY) || "",
    me: null,
    account: null,
    ssoEnabled: false,
    mode: "pair",
    bots: [],
    bot: null,
    threads: [],
    thread: null,
    messages: [],
    lastMessageKey: "",
    timer: null,
    busy: false,
  };

  const $ = (id) => document.getElementById(id);
  const el = (tag, className, text) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  };
  const canWrite = () =>
    state.me?.role === "operator" || state.me?.role === "admin";

  function toast(text) {
    const box = $("toast");
    box.textContent = text;
    box.classList.add("show");
    clearTimeout(toast.timer);
    toast.timer = setTimeout(() => box.classList.remove("show"), 1800);
  }

  function showView(mode) {
    state.mode = mode;
    $("pair-view").classList.toggle("hidden", mode !== "pair");
    $("login-view").classList.toggle("hidden", mode !== "login");
    $("chat-view").classList.toggle("hidden", mode !== "chat");
    $("logout").classList.toggle("hidden", mode === "pair");
    renderSso();
    if (mode !== "chat" && state.timer) {
      clearInterval(state.timer);
      state.timer = null;
    }
  }

  /// The SSO button is additive: username/password stays visible and usable.
  function renderSso() {
    const visible = state.mode === "login" && state.ssoEnabled;
    $("sso-button").classList.toggle("hidden", !visible);
    $("sso-divider").classList.toggle("hidden", !visible);
  }

  function showPair(message) {
    resetChat();
    showView("pair");
    $("pair-error").textContent = message || "";
    $("pair-code").focus();
  }

  function showLogin(message) {
    const wasHidden = $("login-view").classList.contains("hidden");
    if (wasHidden) resetChat();
    showView("login");
    $("login-error").textContent = message || "";
    if (wasHidden) $("login-username").focus();
  }

  function showChat() {
    showView("chat");
  }

  /// A device can change accounts without forgetting the token. Clear the
  /// previous account's conversations before another person signs in.
  function resetChat() {
    state.bot = null;
    state.threads = [];
    state.thread = null;
    state.messages = [];
    state.lastMessageKey = "";
    $("bots").replaceChildren();
    $("threads").replaceChildren();
    $("messages").replaceChildren();
    $("attention").replaceChildren();
    $("attention").classList.add("hidden");
    $("thread-view").classList.add("hidden");
    $("threads").classList.add("hidden");
    $("bots").classList.remove("hidden");
    $("back").classList.add("hidden");
    $("view-title").textContent = "Agents";
    $("composer").value = "";
    $("send-form").dataset.replyTo = "";
  }

  function applyStatus(status) {
    state.me = status.device || null;
    state.account = status.account || null;
    state.ssoEnabled = Boolean(status.sso_enabled);
    renderHeader();
    renderSso();
    return Boolean(status.login_required);
  }

  function forget(message) {
    localStorage.removeItem(TOKEN_KEY);
    state.token = "";
    state.me = null;
    state.account = null;
    state.bots = [];
    state.bot = null;
    state.threads = [];
    state.thread = null;
    state.messages = [];
    state.lastMessageKey = "";
    if (state.timer) clearInterval(state.timer);
    state.timer = null;
    showPair(message || "");
  }

  async function api(path, options = {}, { keepSessionOn401 = false } = {}) {
    const headers = { ...(options.headers || {}) };
    if (state.token) headers.Authorization = `Bearer ${state.token}`;
    if (options.body) headers["Content-Type"] = "application/json";
    const response = await fetch(path, { ...options, headers });
    const data = await response.json().catch(() => ({}));
    if (response.status === 401) {
      if (!state.token) {
        // e.g. a wrong pairing code: there is no device session to forget.
        throw new Error(data.error || "That code is wrong or expired.");
      }
      if (data.code === "login_required") {
        showLogin("Sign in with your RAVENBOT account to continue.");
        throw new Error(data.error || "Account sign-in required.");
      }
      if (data.code === "account_revoked") {
        state.account = null;
        showLogin("Your account no longer has access to this workspace. Sign in again.");
        throw new Error(data.error || "Account access ended.");
      }
      if (keepSessionOn401) {
        throw new Error(data.error || "Sign-in failed.");
      }
      // The device token itself is unknown, expired or revoked.
      forget("Pairing expired. Generate a new code on the host.");
      throw new Error("Pairing expired.");
    }
    if (!response.ok) {
      throw new Error(data.error || `Request failed (${response.status}).`);
    }
    return data;
  }

  function timeLabel(value) {
    const date = new Date(value);
    return Number.isNaN(date.getTime())
      ? ""
      : date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }

  function renderHeader() {
    const line = $("device-line");
    if (state.me) {
      const role = state.me.role || "";
      if (state.account?.username) {
        line.textContent = `${state.me.name} · @${state.account.username} · ${role}`;
      } else {
        line.textContent = `${state.me.name} · ${role}`;
      }
      $("role-badge").textContent = role;
    } else {
      line.textContent = "Remote access";
      $("role-badge").textContent = "";
    }
  }

  function renderBots() {
    const list = $("bots");
    list.replaceChildren();
    if (state.bots.length === 0) {
      list.append(el("p", "muted", "No agents yet. Create one on the host."));
      return;
    }
    for (const bot of state.bots) {
      const button = el("button", "item");
      button.type = "button";
      const left = el("span");
      left.append(el("strong", "", bot.name));
      if (bot.description) left.append(el("small", "", bot.description));
      button.append(left, el("span", "muted", "›"));
      button.addEventListener("click", () => openBot(bot));
      list.append(button);
    }
  }

  function renderThreads() {
    const list = $("threads");
    list.replaceChildren();
    if (state.threads.length === 0) {
      list.append(el("p", "muted", "No conversations yet."));
      return;
    }
    for (const thread of state.threads) {
      const button = el("button", "item");
      button.type = "button";
      button.append(
        el("span", "", thread.title || "Untitled conversation"),
        el("span", "muted", "›"),
      );
      button.addEventListener("click", () => openThread(thread));
      list.append(button);
    }
  }

  function renderMessages() {
    const box = $("messages");
    box.replaceChildren();
    let lastUser = null;
    for (const message of state.messages) {
      if (!message.text.trim() && (message.tools || []).length === 0) continue;
      const article = el("article", `msg ${message.role}`);
      const meta = el(
        "span",
        "meta",
        `${message.sender_name || message.role} · ${timeLabel(message.created_at)}`,
      );
      if (message.reply_to_id) meta.textContent += " · reply";
      article.append(meta, document.createTextNode(message.text));
      if (message.tools?.length) {
        const tools = el("div", "tools");
        for (const tool of message.tools) {
          const chip = el(
            "span",
            `tool${tool.ok ? "" : " bad"}`,
            tool.name,
          );
          tools.append(chip);
        }
        article.append(tools);
      }
      box.append(article);
      if (message.role === "user") lastUser = message.id;
    }
    $("send-form").dataset.replyTo = lastUser || "";
    box.scrollTop = box.scrollHeight;
  }

  function renderAttention(data) {
    const box = $("attention");
    box.replaceChildren();
    const approvals = data.approvals || [];
    const questions = data.questions || [];
    if (approvals.length === 0 && questions.length === 0) {
      box.classList.add("hidden");
      return;
    }
    box.classList.remove("hidden");
    box.append(el("h3", "", "Needs your attention"));

    for (const approval of approvals) {
      const card = el("article", "attn");
      card.append(el("span", "risk", `${approval.risk || "action"} · ${approval.tool_label || approval.tool_name}`));
      card.append(el("p", "", `An agent wants to run ${approval.tool_name}.`));
      const args = el("pre", "", JSON.stringify(approval.arguments, null, 2));
      card.append(args);
      if (canWrite()) {
        const actions = el("div", "actions");
        const allow = el("button", "primary", "Allow");
        allow.type = "button";
        allow.addEventListener("click", () => decide(approval.id, true, allow));
        const deny = el("button", "danger", "Deny");
        deny.type = "button";
        deny.addEventListener("click", () => decide(approval.id, false, deny));
        actions.append(allow, deny);
        card.append(actions);
      } else {
        card.append(el("p", "muted", "Viewer role: answer on the host."));
      }
      box.append(card);
    }

    for (const question of questions) {
      const card = el("article", "attn");
      card.append(el("h3", "", question.header || "Question"));
      card.append(el("p", "", question.question));
      if (!canWrite()) {
        card.append(el("p", "muted", "Viewer role: answer on the host."));
        box.append(card);
        continue;
      }
      const payload = question.payload || {};
      const actions = el("div", "actions");
      if (question.kind === "confirm" && Array.isArray(payload.actions)) {
        for (const action of payload.actions) {
          const button = el(
            "button",
            action.danger || action.style === "danger" ? "danger" : "",
            action.label || action.id,
          );
          button.type = "button";
          button.addEventListener("click", () => {
            const send = () =>
              answer(
                question.id,
                JSON.stringify({ action_id: action.id, values: {} }),
                button,
              );
            if (action.danger || action.confirm || action.style === "danger") {
              arm(button, send);
            } else {
              send();
            }
          });
          actions.append(button);
        }
      } else if (question.kind === "choice" && Array.isArray(payload.options)) {
        for (const option of payload.options) {
          const button = el("button", "", option.label || option.id);
          button.type = "button";
          button.addEventListener("click", () =>
            answer(
              question.id,
              JSON.stringify({ action_id: "submit", values: { selected: [option.id] } }),
              button,
            ),
          );
          actions.append(button);
        }
      } else if (question.kind === "form") {
        card.append(el("p", "muted", "Open the full form on the host to answer it."));
      } else {
        for (const option of question.options || []) {
          const button = el("button", "", option);
          button.type = "button";
          button.addEventListener("click", () => answer(question.id, option, button));
          actions.append(button);
        }
        const custom = el("input");
        custom.type = "text";
        custom.placeholder = "Type an answer";
        custom.setAttribute("aria-label", "Answer this question");
        const send = el("button", "primary", "Answer");
        send.type = "button";
        send.addEventListener("click", () => {
          if (custom.value.trim()) answer(question.id, custom.value.trim(), send);
        });
        actions.append(custom, send);
      }
      if (actions.childElementCount) card.append(actions);
      box.append(card);
    }
  }

  /// Dangerous confirmations take two taps, mirroring the hold control on the
  /// desktop. A stray tap must not authorize a destructive action.
  function arm(button, run) {
    if (button.dataset.armed === "1") {
      delete button.dataset.armed;
      run();
      return;
    }
    const label = button.textContent;
    button.dataset.armed = "1";
    button.textContent = "Tap again to confirm";
    setTimeout(() => {
      if (!button.isConnected) return;
      delete button.dataset.armed;
      button.textContent = label;
    }, 4000);
  }

  async function decide(id, allow, button) {
    if (state.busy) return;
    state.busy = true;
    button.disabled = true;
    try {
      await api(`/api/approvals/${encodeURIComponent(id)}`, {
        method: "POST",
        body: JSON.stringify({ allow }),
      });
      toast(allow ? "Allowed" : "Denied");
      await refresh();
    } catch (error) {
      toast(error.message);
    } finally {
      state.busy = false;
      button.disabled = false;
    }
  }

  async function answer(id, text, button) {
    if (state.busy) return;
    state.busy = true;
    button.disabled = true;
    try {
      await api(`/api/questions/${encodeURIComponent(id)}`, {
        method: "POST",
        body: JSON.stringify({ answer: text }),
      });
      toast("Answer sent");
      await refresh();
    } catch (error) {
      toast(error.message);
    } finally {
      state.busy = false;
      button.disabled = false;
    }
  }

  async function loadMessages() {
    if (!state.thread) return;
    try {
      const data = await api(
        `/api/threads/${encodeURIComponent(state.thread.id)}/messages?limit=100`,
      );
      const messages = data.messages || [];
      const key = `${messages.length}:${messages.at(-1)?.id || ""}:${messages.at(-1)?.text?.length || 0}`;
      if (key !== state.lastMessageKey) {
        state.lastMessageKey = key;
        state.messages = messages;
        renderMessages();
      }
    } catch (error) {
      if (state.mode === "chat") toast(error.message);
    }
  }

  async function loadAttention() {
    try {
      const data = await api("/api/attention");
      renderAttention(data);
    } catch (error) {
      if (state.mode === "chat") toast(error.message);
    }
  }

  async function refresh() {
    if (!state.token) return;
    try {
      const status = await api("/api/status");
      if (applyStatus(status)) {
        showLogin("Sign in with your RAVENBOT account to continue.");
        return;
      }
      if (state.thread) {
        await loadMessages();
        await loadAttention();
      } else {
        await loadAttention();
      }
    } catch (error) {
      if (state.mode === "chat") toast(error.message);
    }
  }

  async function loadBots() {
    const data = await api("/api/bots");
    state.bots = data.bots || [];
    renderBots();
  }

  async function openBot(bot) {
    state.bot = bot;
    state.thread = null;
    state.messages = [];
    state.lastMessageKey = "";
    $("thread-view").classList.add("hidden");
    $("bots").classList.add("hidden");
    $("threads").classList.remove("hidden");
    $("back").classList.remove("hidden");
    $("view-title").textContent = bot.name;
    try {
      const data = await api(`/api/threads?bot_id=${encodeURIComponent(bot.id)}`);
      state.threads = data.threads || [];
      renderThreads();
    } catch (error) {
      toast(error.message);
    }
  }

  async function openThread(thread) {
    state.thread = thread;
    state.messages = [];
    state.lastMessageKey = "";
    $("threads").classList.add("hidden");
    $("thread-view").classList.remove("hidden");
    $("view-title").textContent = thread.title || "Conversation";
    await loadMessages();
    $("composer").focus();
  }

  function back() {
    if (state.thread) {
      state.thread = null;
      $("thread-view").classList.add("hidden");
      $("threads").classList.remove("hidden");
      $("view-title").textContent = state.bot?.name || "Conversations";
      loadAttention();
      return;
    }
    state.threads = [];
    state.bot = null;
    $("threads").classList.add("hidden");
    $("bots").classList.remove("hidden");
    $("back").classList.add("hidden");
    $("view-title").textContent = "Agents";
  }

  async function pair(event) {
    event.preventDefault();
    const code = $("pair-code").value.trim();
    const name = $("pair-name").value.trim();
    $("pair-error").textContent = "";
    const button = event.submitter || $("pair-form").querySelector("button");
    button.disabled = true;
    try {
      const data = await api("/api/pair", {
        method: "POST",
        body: JSON.stringify({ code, device_name: name || "Mobile browser" }),
      });
      state.token = data.token;
      localStorage.setItem(TOKEN_KEY, data.token);
      localStorage.setItem(NAME_KEY, data.name || name);
      state.me = { name: data.name, role: data.role };
      state.account = null;
      renderHeader();
      // `enterChat` fetches status first, so the login view can discover
      // whether the host also offers SSO. When account login is off it simply
      // enters the chat.
      await enterChat();
    } catch (error) {
      $("pair-error").textContent = error.message;
    } finally {
      button.disabled = false;
    }
  }

  async function signIn(event) {
    event.preventDefault();
    const username = $("login-username").value.trim();
    const password = $("login-password").value;
    $("login-error").textContent = "";
    if (!username || !password) {
      $("login-error").textContent = "Enter your username and password.";
      return;
    }
    const button = event.submitter || $("login-form").querySelector("button");
    button.disabled = true;
    try {
      await api(
        "/api/login",
        {
          method: "POST",
          body: JSON.stringify({ username, password }),
        },
        { keepSessionOn401: true },
      );
      $("login-password").value = "";
      await enterChat();
    } catch (error) {
      $("login-error").textContent = error.message;
      if (state.mode === "login") $("login-password").select();
    } finally {
      button.disabled = false;
    }
  }

  async function ssoSignIn() {
    const button = $("sso-button");
    $("login-error").textContent = "";
    button.disabled = true;
    try {
      const data = await api("/api/sso/start", { method: "POST" });
      if (!data.url) {
        throw new Error("Single sign-on is not available.");
      }
      // A top-level navigation to the configured provider. The device token
      // stays in localStorage; no token is ever put in a URL.
      location.href = data.url;
      return;
    } catch (error) {
      $("login-error").textContent = error.message;
      button.disabled = false;
    }
  }

  async function enterChat() {
    const status = await api("/api/status");
    if (applyStatus(status)) {
      showLogin("Sign in with your RAVENBOT account to continue.");
      return;
    }
    showChat();
    renderHeader();
    startPolling();
    await loadBots();
    await loadAttention();
  }

  async function send(event) {
    event.preventDefault();
    if (!state.thread || state.busy) return;
    const box = $("composer");
    const content = box.value.trim();
    if (!content) return;
    state.busy = true;
    const button = event.submitter || $("send-form").querySelector("button");
    button.disabled = true;
    try {
      const replyTo = $("send-form").dataset.replyTo || null;
      await api(`/api/threads/${encodeURIComponent(state.thread.id)}/messages`, {
        method: "POST",
        body: JSON.stringify({ content, reply_to_id: replyTo }),
      });
      box.value = "";
      toast("Sent");
      setTimeout(() => {
        loadMessages();
        loadAttention();
      }, 700);
    } catch (error) {
      toast(error.message);
    } finally {
      state.busy = false;
      button.disabled = false;
    }
  }

  function startPolling() {
    if (state.timer) clearInterval(state.timer);
    state.timer = setInterval(() => {
      if (document.hidden) return;
      if (state.token) refresh();
    }, POLL_MS);
  }

  $("pair-form").addEventListener("submit", pair);
  $("login-form").addEventListener("submit", signIn);
  $("sso-button").addEventListener("click", ssoSignIn);
  $("send-form").addEventListener("submit", send);
  $("back").addEventListener("click", back);
  $("logout").addEventListener("click", async () => {
    // Revoke on the host first; clearing localStorage alone would leave the
    // token valid elsewhere. If the request fails, still clear this device and
    // tell the user to revoke it on the host.
    try {
      await api("/api/device/revoke", { method: "POST" });
    } catch {
      /* offline or already revoked */
    }
    forget("Device forgotten. Revoke it on the host if this browser was shared.");
  });
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden && state.token && state.mode === "chat") refresh();
  });

  /// The callback redirects to the configured redirect URI's origin with
  /// `?sso=ok` or `?sso=error&reason=...`. Remove it before anything else so a
  /// refresh cannot replay the notice, and never show the reason text.
  function takeSsoOutcome() {
    const url = new URL(location.href);
    const outcome = url.searchParams.get("sso");
    if (outcome !== "ok" && outcome !== "error") return null;
    url.searchParams.delete("sso");
    url.searchParams.delete("reason");
    history.replaceState(null, "", `${url.pathname}${url.search}${url.hash}`);
    return outcome;
  }

  async function start() {
    const ssoOutcome = takeSsoOutcome();
    if (!state.token) {
      if (ssoOutcome === "error") {
        showPair(SSO_ERROR);
      } else if (ssoOutcome === "ok") {
        showPair("Single sign-on is complete, but this browser is not paired. Pair it to continue.");
      } else {
        showPair("");
      }
      return;
    }
    // Keep the device token while the server tells us whether sign-in is
    // required; only an invalid device token is forgotten by `api`.
    showChat();
    try {
      await enterChat();
      if (ssoOutcome === "ok" && state.mode === "chat") {
        toast("Signed in with SSO");
      }
      if (ssoOutcome === "error" && state.mode === "login") {
        $("login-error").textContent = SSO_ERROR;
      }
    } catch (error) {
      if (state.token && state.mode !== "login") {
        showChat();
        startPolling();
        toast(error.message);
      }
    }
  }

  start();
})();

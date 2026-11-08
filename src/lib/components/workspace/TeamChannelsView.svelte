<script lang="ts">
  /**
   * A team channel: one shared timeline, one task board, one owner per task.
   *
   * The timeline is deliberately not a chat thread. Human notes and agent
   * results land in the same record, and a task's result is posted back into
   * that record when its run finishes. That is what makes a channel something
   * a team returns to instead of a conversation that scrolls away.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, onDestroy, tick } from "svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import ChannelsPanel from "$lib/components/ChannelsPanel.svelte";
  import { Button } from "$lib/components/ui/button";
  import { t, type TranslationKey } from "$lib/i18n";
  import {
    AlertCircle,
    Archive,
    CheckCircle2,
    Circle,
    Hash,
    Loader2,
    Pause,
    Play,
    Plus,
    RotateCcw,
    Send,
    Settings2,
    Users,
  } from "@lucide/svelte";

  interface Channel {
    id: string;
    name: string;
    description: string;
    instructions: string;
    working_folder: string | null;
    color: string | null;
    position: number;
    bot_ids: string[];
  }

  interface ChannelMessage {
    id: string;
    channel_id: string;
    author_kind: "human" | "agent" | "system";
    author_id: string | null;
    author_name: string;
    content: string;
    reply_to_id: string | null;
    created_at: string;
  }

  interface ChannelTask {
    id: string;
    channel_id: string;
    title: string;
    description: string;
    owner_bot_id: string;
    state: "pending" | "running" | "paused" | "done" | "failed" | "archived";
    created_by: string;
    source_thread_id: string | null;
    result: string | null;
    created_at: string;
    updated_at: string;
    archived_at: string | null;
  }

  interface Bot {
    id: string;
    name: string;
    specialty?: string | null;
    rank?: string | null;
  }

  const STATE_LABEL: Record<string, TranslationKey> = {
    pending: "channels.state.pending",
    running: "channels.state.running",
    paused: "channels.state.paused",
    done: "channels.state.done",
    failed: "channels.state.failed",
    archived: "channels.state.archived",
  };

  let channels = $state<Channel[]>([]);
  let bots = $state<Bot[]>([]);
  let selectedChannelId = $state<string | null>(null);
  let messages = $state<ChannelMessage[]>([]);
  let tasks = $state<ChannelTask[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let newMessage = $state("");
  let sending = $state(false);
  let showTaskForm = $state(false);
  let showSettings = $state(false);
  let taskTitle = $state("");
  let taskDescription = $state("");
  let taskOwner = $state("");
  let creatingTask = $state(false);
  let busyTaskId = $state<string | null>(null);

  let timelineEnd = $state<HTMLDivElement | null>(null);
  let composer = $state<HTMLTextAreaElement | null>(null);
  let unlisten: UnlistenFn | null = null;

  const selectedChannel = $derived(
    channels.find((channel) => channel.id === selectedChannelId) ?? null,
  );
  const members = $derived(
    selectedChannel
      ? selectedChannel.bot_ids
          .map((id) => bots.find((bot) => bot.id === id))
          .filter((bot): bot is Bot => !!bot)
      : [],
  );
  const activeTasks = $derived(tasks.filter((task) => task.state !== "archived"));
  const archivedTasks = $derived(tasks.filter((task) => task.state === "archived"));

  function botName(id: string): string {
    return bots.find((bot) => bot.id === id)?.name ?? id.slice(0, 8);
  }

  function stateLabel(state: string): string {
    return t(STATE_LABEL[state] ?? "channels.state.pending");
  }

  function formatTime(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? "" : date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  async function loadChannels() {
    loading = true;
    error = null;
    try {
      channels = await invoke<Channel[]>("list_channels");
      if (!selectedChannelId || !channels.some((channel) => channel.id === selectedChannelId)) {
        selectedChannelId = channels[0]?.id ?? null;
      }
      if (selectedChannelId) await loadChannel(selectedChannelId);
      else {
        messages = [];
        tasks = [];
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  async function loadChannel(id: string) {
    try {
      const [nextMessages, nextTasks] = await Promise.all([
        invoke<ChannelMessage[]>("list_channel_messages", { channelId: id, limit: 200 }),
        invoke<ChannelTask[]>("list_channel_tasks", {
          channelId: id,
          includeArchived: true,
        }),
      ]);
      messages = nextMessages ?? [];
      tasks = nextTasks ?? [];
      taskOwner = taskOwner || members[0]?.id || "";
      await tick();
      timelineEnd?.scrollIntoView?.({ block: "end" });
    } catch (cause) {
      error = String(cause);
    }
  }

  async function selectChannel(id: string) {
    selectedChannelId = id;
    taskOwner = "";
    showTaskForm = false;
    await loadChannel(id);
  }

  async function addChannel() {
    try {
      const id = await invoke<string>("create_channel", {
        name: `Channel ${channels.length + 1}`,
        description: "",
        instructions: "",
        workingFolder: null,
        color: null,
        botIds: [],
      });
      await loadChannels();
      selectedChannelId = id;
      await loadChannel(id);
      showSettings = true;
    } catch (cause) {
      error = String(cause);
    }
  }

  async function send() {
    const content = newMessage.trim();
    if (!content || !selectedChannelId || sending) return;
    sending = true;
    error = null;
    try {
      const message = await invoke<ChannelMessage>("post_channel_message", {
        channelId: selectedChannelId,
        content,
        authorName: "You",
      });
      messages = [...messages, message];
      newMessage = "";
      await tick();
      timelineEnd?.scrollIntoView?.({ block: "end" });
      composer?.focus();
    } catch (cause) {
      error = String(cause);
    } finally {
      sending = false;
    }
  }

  function composerKeys(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  }

  function openTaskForm() {
    taskTitle = "";
    taskDescription = "";
    taskOwner = members[0]?.id ?? "";
    showTaskForm = true;
  }

  async function createTask() {
    if (!selectedChannelId || creatingTask) return;
    if (!taskTitle.trim()) {
      error = t("channels.taskTitle");
      return;
    }
    if (!taskOwner) {
      error = t("channels.noMembers");
      return;
    }
    creatingTask = true;
    error = null;
    try {
      await invoke("create_channel_task", {
        channelId: selectedChannelId,
        title: taskTitle.trim(),
        description: taskDescription.trim(),
        ownerBotId: taskOwner,
      });
      showTaskForm = false;
      await loadChannel(selectedChannelId);
    } catch (cause) {
      error = String(cause);
    } finally {
      creatingTask = false;
    }
  }

  async function taskAction(task: ChannelTask, action: string, owner?: string) {
    busyTaskId = task.id;
    error = null;
    try {
      if (action === "start") {
        await invoke("start_channel_task", { taskId: task.id });
        // The run is spawned by the backend; a short refresh shows `running`
        // while the channel-event listener handles the finish.
        window.setTimeout(() => {
          if (selectedChannelId) void loadChannel(selectedChannelId);
        }, 400);
      } else {
        await invoke("channel_task_action", {
          taskId: task.id,
          action,
          ownerBotId: owner ?? null,
          note: null,
        });
        if (selectedChannelId) await loadChannel(selectedChannelId);
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      busyTaskId = null;
    }
  }

  onMount(() => {
    void loadChannels();
    void (async () => {
      try {
        bots = await invoke<Bot[]>("list_bots");
        unlisten = await listen<{ channel_id?: string }>("channel-event", (event) => {
          if (event.payload?.channel_id === selectedChannelId) {
            void loadChannel(selectedChannelId);
          }
        });
      } catch (cause) {
        // Browser previews have no event bridge; the state still loads.
        console.debug("channel events unavailable:", cause);
      }
    })();
  });

  onDestroy(() => {
    unlisten?.();
    unlisten = null;
  });
</script>

<div class="team-channels">
  <aside class="team-channels__rail" aria-label={t("channels.title")}>
    <header class="team-channels__rail-head">
      <div>
        <h2>{t("channels.title")}</h2>
        <p>{t("channels.subtitle")}</p>
      </div>
      <button
        type="button"
        class="team-channels__icon-button"
        title={t("channels.new")}
        aria-label={t("channels.new")}
        onclick={addChannel}
      >
        <Plus class="size-4" />
      </button>
    </header>

    <div class="team-channels__channel-list">
      {#if loading && channels.length === 0}
        <div class="team-channels__muted"><Loader2 class="size-3.5 animate-spin" /> {t("channels.loading")}</div>
      {:else if channels.length === 0}
        <div class="team-channels__muted">{t("channels.empty")}</div>
      {/if}
      {#each channels as channel (channel.id)}
        <button
          type="button"
          class="team-channels__channel"
          class:is-active={channel.id === selectedChannelId}
          onclick={() => selectChannel(channel.id)}
        >
          <Hash class="size-3.5" style={channel.color ? `color:${channel.color}` : ""} />
          <span class="team-channels__channel-name">{channel.name}</span>
          <span class="team-channels__channel-members">
            <Users class="size-3" /> {channel.bot_ids.length}
          </span>
        </button>
      {/each}
    </div>

    {#if selectedChannel}
      <button
        type="button"
        class="team-channels__settings"
        onclick={() => (showSettings = true)}
      >
        <Settings2 class="size-3.5" />
        {t("channels.settings")}
      </button>
    {/if}
  </aside>

  <main class="team-channels__main">
    {#if error}
      <p class="team-channels__error" role="alert">
        <AlertCircle class="size-3.5" /> {error}
      </p>
    {/if}

    {#if !selectedChannel}
      <div class="team-channels__empty">
        <Hash class="size-8" />
        <p>{t("channels.select")}</p>
      </div>
    {:else}
      <header class="team-channels__header">
        <div>
          <h1><Hash class="size-4" /> {selectedChannel.name}</h1>
          <p>{selectedChannel.description || t("channels.subtitle")}</p>
        </div>
        <span class="team-channels__members">
          <Users class="size-3.5" /> {members.length}
        </span>
      </header>

      <div class="team-channels__timeline" aria-live="polite">
        {#if messages.length === 0}
          <div class="team-channels__empty team-channels__empty--timeline">
            <p>{t("channels.noMessages")}</p>
          </div>
        {/if}
        {#each messages as message (message.id)}
          <article class="team-channels__message" data-kind={message.author_kind}>
            <header>
              <strong>{message.author_name}</strong>
              <time>{formatTime(message.created_at)}</time>
            </header>
            {#if message.author_kind === "system"}
              <p class="team-channels__system">{message.content}</p>
            {:else}
              <div class="team-channels__message-body">
                <MarkdownRenderer content={message.content} />
              </div>
            {/if}
          </article>
        {/each}
        <div bind:this={timelineEnd}></div>
      </div>

      {#if members.length === 0}
        <p class="team-channels__hint">{t("channels.noMembers")}</p>
      {/if}

      <div class="team-channels__composer">
        <textarea
          bind:this={composer}
          bind:value={newMessage}
          onkeydown={composerKeys}
          rows={2}
          placeholder={t("channels.composerPlaceholder")}
          aria-label={t("channels.composerPlaceholder")}
        ></textarea>
        <Button
          size="icon"
          class="team-channels__send"
          disabled={!newMessage.trim() || sending}
          onclick={send}
          aria-label={t("channels.send")}
          title={t("channels.send")}
        >
          {#if sending}<Loader2 class="size-4 animate-spin" />{:else}<Send class="size-4" />{/if}
        </Button>
      </div>
    {/if}
  </main>

  {#if selectedChannel}
    <aside class="team-channels__tasks" aria-label={t("channels.tasks")}>
      <header class="team-channels__tasks-head">
        <h2>{t("channels.tasks")}</h2>
        <Button size="sm" variant="outline" onclick={openTaskForm} disabled={members.length === 0}>
          <Plus class="size-3.5" />
          {t("channels.newTask")}
        </Button>
      </header>

      {#if showTaskForm}
        <div class="team-channels__task-form">
          <input bind:value={taskTitle} placeholder={t("channels.taskTitle")} aria-label={t("channels.taskTitle")} />
          <textarea
            bind:value={taskDescription}
            rows={3}
            placeholder={t("channels.taskDescription")}
            aria-label={t("channels.taskDescription")}
          ></textarea>
          <label>
            <span>{t("channels.owner")}</span>
            <select bind:value={taskOwner}>
              {#each members as member (member.id)}
                <option value={member.id}>{member.name}</option>
              {/each}
            </select>
          </label>
          <div class="team-channels__task-form-actions">
            <Button size="sm" onclick={createTask} disabled={creatingTask}>
              {#if creatingTask}<Loader2 class="size-3.5 animate-spin" />{:else}<Plus class="size-3.5" />{/if}
              {t("channels.createTask")}
            </Button>
            <Button size="sm" variant="ghost" onclick={() => (showTaskForm = false)}>×</Button>
          </div>
        </div>
      {/if}

      <div class="team-channels__task-list">
        {#if activeTasks.length === 0 && archivedTasks.length === 0}
          <div class="team-channels__muted">{t("channels.noTasks")}</div>
        {/if}
        {#each [...activeTasks, ...archivedTasks] as task (task.id)}
          <article class="team-channels__task" data-state={task.state}>
            <header>
              <span class="team-channels__task-state" data-state={task.state}>
                {#if task.state === "running"}
                  <Loader2 class="size-3 animate-spin" />
                {:else if task.state === "done"}
                  <CheckCircle2 class="size-3" />
                {:else if task.state === "failed"}
                  <AlertCircle class="size-3" />
                {:else if task.state === "paused"}
                  <Pause class="size-3" />
                {:else if task.state === "archived"}
                  <Archive class="size-3" />
                {:else}
                  <Circle class="size-3" />
                {/if}
                {stateLabel(task.state)}
              </span>
              <span class="team-channels__task-owner">{botName(task.owner_bot_id)}</span>
              {#if task.state !== "running" && task.state !== "archived"}
                <button
                  type="button"
                  class="team-channels__task-archive"
                  title={t("channels.archive")}
                  aria-label={t("channels.archive")}
                  onclick={() => taskAction(task, "archive")}
                >
                  <Archive class="size-3" />
                </button>
              {/if}
            </header>
            <h3>{task.title}</h3>
            {#if task.description}
              <p>{task.description}</p>
            {/if}
            {#if task.result && (task.state === "done" || task.state === "failed")}
              <div class="team-channels__task-result">
                <MarkdownRenderer content={task.result} />
              </div>
            {/if}

            <div class="team-channels__task-actions">
              {#if busyTaskId === task.id}
                <Loader2 class="size-3.5 animate-spin" />
              {:else if task.state === "running"}
                <button type="button" onclick={() => taskAction(task, "stop")} title={t("channels.stopHint")}>
                  <Pause class="size-3" /> {t("channels.stop")}
                </button>
              {:else if task.state === "archived"}
                <button type="button" onclick={() => taskAction(task, "restore")}>
                  <RotateCcw class="size-3" /> {t("channels.restore")}
                </button>
              {:else}
                {#if task.state === "pending" || task.state === "paused"}
                  <button type="button" onclick={() => taskAction(task, "start")}>
                    <Play class="size-3" /> {t("channels.start")}
                  </button>
                {/if}
                {#if task.state === "pending" || task.state === "paused"}
                  <select
                    aria-label={t("channels.reassign")}
                    title={t("channels.reassign")}
                    value={task.owner_bot_id}
                    onchange={(event) =>
                      taskAction(task, "reassign", (event.currentTarget as HTMLSelectElement).value)}
                  >
                    {#each members as member (member.id)}
                      <option value={member.id}>{member.name}</option>
                    {/each}
                  </select>
                {/if}

              {/if}
            </div>
          </article>
        {/each}
      </div>
    </aside>
  {/if}
</div>

{#if showSettings}
  <ChannelsPanel
    onClose={() => {
      showSettings = false;
      void loadChannels();
    }}
    onChanged={() => void loadChannels()}
  />
{/if}

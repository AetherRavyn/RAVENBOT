// Selection-as-router workspace state (OpenBot pattern: "what's selected" IS
// the route). Every surface reads this module; components never own selection.
import { invoke } from "@tauri-apps/api/core";
import { announce } from "$lib/a11y";
import { t } from "$lib/i18n";

export type RailDest = "home" | "agents" | "offices" | "connectors" | "routines";

class Workspace {
  dest = $state<RailDest>("home");
  loading = $state(true);

  bots = $state<any[]>([]);
  chatrooms = $state<any[]>([]);
  selectedBotId = $state<string | null>(null);
  selectedRoomId = $state<string | null>(null);

  sidebarCollapsed = $state(false);
  sidebarWidth = $state(280);

  killSwitchActive = $state(false);
  showPalette = $state(false);
  showSettings = $state(false);
  settingsTab = $state("keys");
  srMessage = $state("");

  selectedBot = $derived(this.bots.find((b: any) => b.id === this.selectedBotId));
  selectedRoom = $derived(this.chatrooms.find((r: any) => r.id === this.selectedRoomId));

  goto(dest: RailDest) {
    this.dest = dest;
  }

  selectBot(id: string) {
    this.selectedBotId = id;
    this.selectedRoomId = null;
    this.dest = "agents";
    announce(this.bots.find((b: any) => b.id === id)?.name || "");
  }

  selectRoom(id: string) {
    this.selectedRoomId = id;
    this.selectedBotId = null;
    this.dest = "offices";
    announce(this.chatrooms.find((r: any) => r.id === id)?.name || "");
  }

  newChat() {
    this.selectedBotId = null;
    this.selectedRoomId = null;
    this.dest = "home";
    announce("New Chat Home");
  }

  showAgents() {
    this.selectedBotId = null;
    this.selectedRoomId = null;
    this.dest = "agents";
    announce("Agents");
  }

  openSettings(tab = "keys") {
    this.settingsTab = tab;
    this.showSettings = true;
  }

  togglePalette() {
    this.showPalette = !this.showPalette;
  }

  async createAndSend(text: string, botId?: string | null): Promise<boolean> {
    let target = this.bots.find((b: any) => b.id === botId) || this.bots[0];
    try {
      if (!target) {
        target = await invoke("create_bot", {
          name: "Raven Prime",
          description: "Primary Sovereign Fleet Assistant",
          avatarUrl: "/ravenicon.png",
          avatarStyle: "bottts",
        });
        this.bots = [...this.bots, target];
      }
      const thread: any = await invoke("create_thread", {
        botId: target.id,
        title: text.slice(0, 35) + (text.length > 35 ? "..." : ""),
      });
      await invoke("send_message", { threadId: thread.id, content: text });
      this.selectBot(target.id);
      return true;
    } catch (e) {
      console.error("Home prompt dispatch error:", e);
      return false;
    }
  }

  async createAgent(name: string) {
    const bot = await invoke("create_bot", {
      name,
      description: "",
      avatarUrl: "/ravenicon.png",
      avatarStyle: "bottts",
    });
    this.handleBotCreated(bot);
    return bot;
  }

  async init() {
    try {
      this.bots = await invoke("list_bots");
      try {
        this.chatrooms = await invoke("list_chatrooms");
      } catch {}
      const status: any = await invoke("get_status");
      this.killSwitchActive = status.kill_switch_active;
    } catch (e) {
      console.error("Failed to load data:", e);
      this.srMessage = t("sr.fleetLoadFailed");
    } finally {
      this.loading = false;
    }
  }

  async refreshBots() {
    try {
      this.bots = await invoke("list_bots");
    } catch (e) {
      console.error("Failed to reload bots:", e);
    }
  }

  handleBotCreated(bot: any) {
    this.bots = [...this.bots, bot];
    this.selectBot(bot.id);
    this.srMessage = t("sr.botCreated", { name: bot.name });
  }

  handleBotUpdated(updated: any) {
    this.bots = this.bots.map((b: any) => (b.id === updated.id ? updated : b));
    this.srMessage = t("sr.botUpdated", { name: updated.name });
  }

  handleBotDeleted(botId: string) {
    const bot = this.bots.find((b: any) => b.id === botId);
    this.bots = this.bots.filter((b: any) => b.id !== botId);
    if (this.selectedBotId === botId) {
      this.selectedBotId = this.bots.length > 0 ? this.bots[0].id : null;
    }
    this.srMessage = t("sr.botDeleted", { name: bot?.name || t("ui.fallbackAgent") });
  }

  handleRoomCreated(room: any) {
    this.chatrooms = [...this.chatrooms, room];
    this.selectRoom(room.id);
  }

  handleRoomUpdated(e: Event) {
    const updated = (e as CustomEvent).detail?.room;
    if (updated) {
      this.chatrooms = this.chatrooms.map((r: any) => (r.id === updated.id ? updated : r));
    }
  }

  handleRoomDeleted(e: Event) {
    const roomId = (e as CustomEvent).detail?.roomId;
    if (!roomId) return;
    this.chatrooms = this.chatrooms.filter((r: any) => r.id !== roomId);
    if (this.selectedRoomId === roomId) {
      if (this.chatrooms.length > 0) {
        this.selectRoom(this.chatrooms[0].id);
      } else {
        this.selectedRoomId = null;
        this.dest = "agents";
        if (this.bots.length > 0) this.selectedBotId = this.bots[0].id;
      }
    }
  }
}

export const workspace = new Workspace();

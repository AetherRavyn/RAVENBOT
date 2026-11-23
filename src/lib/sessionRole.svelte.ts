import { invoke } from "@tauri-apps/api/core";

/// The effective role in the active shared workspace. Loaded once by the
/// Workspace shell; viewers are read-only and operators cannot administer it.
class SessionRole {
  role = $state("owner");
  workspace = $state("");
  loaded = $state(false);

  async load(): Promise<void> {
    if (this.loaded) return;
    try {
      const account: any = await invoke("get_current_account");
      this.role = account?.role || "owner";
      this.workspace = account?.workspace || "";
    } catch {
      this.role = "owner";
    } finally {
      this.loaded = true;
    }
  }
}

export const sessionRole = new SessionRole();

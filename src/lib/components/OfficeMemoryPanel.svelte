<script lang="ts">
  import MemoryManager from "$lib/components/MemoryManager.svelte";
  import { t } from "$lib/i18n";

  interface Props {
    chatroomId: string;
  }

  let { chatroomId }: Props = $props();

  const CATEGORIES = $derived([
    { id: "general", label: t("memory.cGeneral"), icon: "🧠" },
    { id: "preference", label: t("memory.cPreference"), icon: "⭐" },
    { id: "fact", label: t("memory.cFact"), icon: "📌" },
    { id: "process", label: t("memory.cProcess"), icon: "⚡" },
    { id: "rule", label: t("memory.cRule"), icon: "🛡️" },
    { id: "architecture", label: t("memory.cArchitecture"), icon: "🏛️" },
  ]);
</script>

<MemoryManager scope="office" targetId={chatroomId} categories={CATEGORIES} />

<script lang="ts">
  /**
   * A fresh agent thread before its first message.
   *
   * An empty pane is a dead end. This gives the agent a face, one sentence of
   * purpose, and three concrete starting points — each one is a real prompt
   * the person can send, not a decorative card. The composer remains the main
   * control; these are shortcuts, never a gate.
   */
  import { ArrowUpRight, ListChecks, Search, Sparkles } from "@lucide/svelte";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";

  interface Props {
    name: string;
    description?: string;
    specialty?: string;
    imageUrl?: string | null;
    style?: string | null;
    onPick: (prompt: string) => void;
    class?: string;
  }

  let {
    name,
    description = "",
    specialty = "",
    imageUrl = null,
    style = null,
    onPick,
    class: cls = "",
  }: Props = $props();

  const suggestions = [
    {
      icon: Search,
      title: "Map this workspace",
      text: "Find the entry points, the test command, and the riskiest module.",
      prompt: "Map this workspace: entry points, test command, and the riskiest module.",
    },
    {
      icon: ListChecks,
      title: "Plan the next move",
      text: "Turn the current goal into an ordered, testable task list.",
      prompt: "Turn the current goal into an ordered, testable task list with acceptance checks.",
    },
    {
      icon: Sparkles,
      title: "Review recent changes",
      text: "Read the diff and report what needs attention before it ships.",
      prompt: "Review the recent changes and report what needs attention before it ships.",
    },
  ];

  const purpose = $derived(
    description.trim() || specialty.trim() || "Ready to work in this workspace.",
  );
</script>

<section class={"chat-empty " + cls} aria-label={`${name} is ready`}>
  <div class="chat-empty__portrait" aria-hidden="true">
    <RavenAvatar {name} {imageUrl} {style} mood="idle" decorative />
  </div>
  <span class="chat-empty__eyebrow">Ready when you are</span>
  <h2 class="chat-empty__title">{name}</h2>
  <p class="chat-empty__purpose">{purpose}</p>

  <div class="chat-empty__suggestions" role="group" aria-label="Suggested first prompts">
    {#each suggestions as suggestion (suggestion.title)}
      {@const Icon = suggestion.icon}
      <button
        type="button"
        class="chat-empty__suggestion"
        onclick={() => onPick(suggestion.prompt)}
      >
        <span class="chat-empty__suggestion-icon" aria-hidden="true">
          <Icon class="size-3.5" strokeWidth={1.8} />
        </span>
        <span class="chat-empty__suggestion-copy">
          <span class="chat-empty__suggestion-title">{suggestion.title}</span>
          <span class="chat-empty__suggestion-text">{suggestion.text}</span>
        </span>
        <ArrowUpRight class="chat-empty__suggestion-arrow size-3.5" strokeWidth={1.8} />
      </button>
    {/each}
  </div>
</section>

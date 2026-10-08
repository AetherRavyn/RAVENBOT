<script lang="ts">
  import * as Select from "$lib/components/ui/select";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";

  export interface SelectOption {
    value: string;
    label: string;
    icon?: string;
    disabled?: boolean;
  }

  interface Props {
    value: string;
    options: SelectOption[];
    onValueChange: (value: string) => void;
    placeholder?: string;
    /** Trigger classes (defaults to a standard h-9 field). */
    class?: string;
    contentClass?: string;
    id?: string;
    disabled?: boolean;
  }

  let {
    value,
    options,
    onValueChange,
    placeholder = "Select…",
    class: className = "",
    contentClass = "",
    id,
    disabled = false,
  }: Props = $props();

  let selected = $derived(options.find((o) => o.value === value));
</script>

<Select.Root type="single" {value} onValueChange={(v) => onValueChange(v ?? "")} {disabled}>
  <Select.Trigger
    {id}
    class="w-full h-9 rounded-lg bg-[var(--surface-2)] border border-[var(--hairline)] hover:border-[var(--hairline-strong)] px-2.5 flex items-center justify-between gap-2 text-xs text-[var(--text-secondary)] cursor-pointer outline-none transition-colors data-[placeholder]:text-[var(--text-muted)] {className}"
  >
    <span class={selected ? "truncate" : "truncate text-[var(--text-muted)]"}>
      {selected ? `${selected.icon ? selected.icon + " " : ""}${selected.label}` : placeholder}
    </span>
    <ChevronDown class="size-3.5 text-[var(--text-muted)] shrink-0" />
  </Select.Trigger>
  <Select.Content class="max-h-72 {contentClass}">
    {#each options as option (option.value)}
      <Select.Item value={option.value} disabled={option.disabled} class="text-xs">
        {option.icon ? `${option.icon} ` : ""}{option.label}
      </Select.Item>
    {/each}
  </Select.Content>
</Select.Root>

<script lang="ts">
  import type { ThemeDefinition } from "$lib/theme";

  interface Props {
    theme: ThemeDefinition;
  }

  let { theme }: Props = $props();
</script>

<!-- Flat, solid theme background with a faint pixel grid (no gradients). -->
<div class="absolute inset-0 pointer-events-none overflow-hidden select-none">
  <div class="absolute inset-0" style="background-color: {theme.bgHex};"></div>
  <svg
    class="absolute inset-0 w-full h-full opacity-[0.05]"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <defs>
      <pattern id="rb-theme-grid" width="40" height="40" patternUnits="userSpaceOnUse">
        <path
          d="M 40 0 L 0 0 0 40"
          fill="none"
          stroke={theme.primaryColor}
          stroke-width="1"
        />
      </pattern>
    </defs>
    <rect width="100%" height="100%" fill="url(#rb-theme-grid)" />
  </svg>
</div>

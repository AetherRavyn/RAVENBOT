<script lang="ts">
  /**
   * A measured, variable-height transcript window.
   *
   * The views still own paging (which slice of history is loaded); this
   * component owns mounting (which part of that slice has a DOM row). It keeps
   * the scroll container's height stable with top/bottom spacers, measures the
   * rows it does mount, and re-windows as the reader scrolls. Rows that leave
   * the viewport are unmounted and their measured height is reused the next
   * time the reader comes back.
   *
   * The component is intentionally layout-thin: it renders one block per row
   * and lets the parent supply the row's class and snippet. That keeps the
   * 1:1 and office message markup where it already lives instead of cloning
   * the 300-line ChatMessageRow call into a second place.
   */
  import { onDestroy, tick, type Snippet } from "svelte";
  import { prefersReducedMotion } from "$lib/a11y";
  import {
    describeSlice,
    sliceRows,
    tailRows,
    type VirtualSlice,
  } from "$lib/chat/virtualRows";

  interface Props {
    items: any[];
    container: HTMLElement | null;
    itemKey: (item: any, index: number) => string;
    row: Snippet<[any, number]>;
    /** Height estimate for an unmounted row. The component adds the item gap. */
    estimate?: (item: any) => number;
    /** Extra class for the row wrapper (for example a separator-to-row gap). */
    itemClass?: string;
    /** Leading gap between message items. Adds to every row after the first. */
    gap?: number;
    /** Pixels mounted above and below the viewport. */
    overscan?: number;
    /** Rows mounted before the first measurement. */
    initialRows?: number;
    /** Changing this drops every measured height and starts at the tail. */
    resetKey?: string;
    /** Set to a message key to mount and scroll to it; cleared after use. */
    target?: string;
    /** Increment to pin the window to the live tail (jump-to-latest). */
    pin?: number;
    /** Virtual offsets for the scroll rail, in percentages of the container. */
    onLayout?: (layout: { tops: Record<string, number>; total: number }) => void;
  }

  let {
    items,
    container,
    itemKey,
    row,
    estimate,
    itemClass = "",
    gap = 0,
    overscan = 480,
    initialRows = 10,
    resetKey = "",
    target = $bindable(""),
    pin = 0,
    onLayout,
  }: Props = $props();

  let listEl = $state<HTMLElement | null>(null);
  let start = $state(0);
  let end = $state(0);
  let top = $state(0);
  let bottom = $state(0);
  let version = $state(0);

  const heights = new Map<string, number>();
  let rowObserver: ResizeObserver | null = null;
  let lastReset: string | null = null;
  let trackedStartKey: string | null = null;
  let trackedStartIndex = 0;
  let firstSync = true;
  let pinnedToBottom = true;
  let userScrolled = false;
  let revealing = false;
  let running = false;
  let pendingSync = false;
  let frame = 0;
  let revealTimer: ReturnType<typeof setTimeout> | null = null;
  let layoutSignature = "";
  let lastPin = 0;

  const keys = $derived(items.map((item, index) => itemKey(item, index)));

  const sizes = $derived.by(() => {
    // A measured height is a dependency even though the Map itself is not
    // reactive; bumping `version` is what tells this derivation to look again.
    version;
    return items.map((item, index) => {
      const key = keys[index];
      const measured = heights.get(key);
      const fallback =
        (estimate ? estimate(item) : 120) +
        (index === 0 ? 0 : Math.max(0, gap));
      return {
        key,
        height: Math.max(1, measured ?? fallback),
      };
    });
  });

  const shown = $derived.by(() => {
    const count = items.length;
    const from = Math.max(0, Math.min(start, count));
    const to = Math.max(from, Math.min(end, count));
    return items.slice(from, to).map((item, offset) => ({
      item,
      index: from + offset,
      key: keys[from + offset],
    }));
  });

  function rememberHeight(node: HTMLElement): boolean {
    const key = node.dataset.virtualKey;
    if (!key) return false;
    const height = node.offsetHeight;
    if (!Number.isFinite(height) || height <= 0) return false;
    const previous = heights.get(key);
    if (previous !== undefined && Math.abs(previous - height) < 0.5) {
      return false;
    }
    heights.set(key, height);
    // A long session can walk through thousands of rows; keep the cache
    // bounded so a day of scrolling does not retain every measurement.
    if (heights.size > 2_000) {
      const oldest = heights.keys().next();
      if (!oldest.done) heights.delete(oldest.value);
    }
    version += 1;
    return true;
  }

  function measureMounted() {
    if (!listEl) return;
    for (const node of listEl.querySelectorAll<HTMLElement>("[data-virtual-key]")) {
      rememberHeight(node);
    }
  }

  function getObserver(): ResizeObserver | null {
    if (rowObserver) return rowObserver;
    if (typeof ResizeObserver === "undefined") return null;
    rowObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        rememberHeight(entry.target as HTMLElement);
      }
      schedule();
    });
    return rowObserver;
  }

  /** Row action: measure once and keep watching while the row is mounted. */
  function measureRow(node: HTMLElement) {
    rememberHeight(node);
    getObserver()?.observe(node);
    return {
      destroy() {
        rowObserver?.unobserve(node);
      },
    };
  }

  function apply(slice: VirtualSlice) {
    start = slice.start;
    end = slice.end;
    top = slice.top;
    bottom = slice.bottom;
  }

  function applyCurrentSpacers() {
    const slice = describeSlice(sizes, start, end);
    start = slice.start;
    end = slice.end;
    top = slice.top;
    bottom = slice.bottom;
  }

  function schedule() {
    if (typeof requestAnimationFrame === "undefined") {
      void runSync();
      return;
    }
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      void runSync();
    });
  }

  async function runSync() {
    if (running) {
      pendingSync = true;
      return;
    }
    running = true;
    try {
      do {
        pendingSync = false;
        await syncOnce();
      } while (pendingSync);
    } finally {
      running = false;
    }
  }

  function topAnchor(): { key: string; offset: number } | null {
    if (!container || !listEl) return null;
    const containerRect = container.getBoundingClientRect();
    for (const node of listEl.querySelectorAll<HTMLElement>("[data-virtual-key]")) {
      const rect = node.getBoundingClientRect();
      if (rect.bottom <= containerRect.top + 1) continue;
      const key = node.dataset.virtualKey;
      if (!key) return null;
      return { key, offset: rect.top - containerRect.top };
    }
    return null;
  }

  function restoreAnchor(anchor: { key: string; offset: number }) {
    if (!container || !listEl) return;
    const node = Array.from(
      listEl.querySelectorAll<HTMLElement>("[data-virtual-key]"),
    ).find((candidate) => candidate.dataset.virtualKey === anchor.key);
    if (!node) return;
    const containerRect = container.getBoundingClientRect();
    const offset = node.getBoundingClientRect().top - containerRect.top;
    container.scrollTop += offset - anchor.offset;
  }

  async function syncOnce() {
    if (!container || !listEl || items.length === 0) return;
    if (revealing) {
      reportLayout();
      return;
    }

    const containerRect = container.getBoundingClientRect();
    const listRect = listEl.getBoundingClientRect();
    const listOffset = listRect.top - containerRect.top + container.scrollTop;
    const viewportHeight = container.clientHeight;
    // A scroll event means the reader moved. Content growth does not, so a
    // reader who was pinned to the bottom keeps following a tall new message
    // even though the old scrollTop is now more than 80px from the new bottom.
    const moved = userScrolled;
    userScrolled = false;
    const nearBottom =
      firstSync ||
      (!moved && pinnedToBottom) ||
      container.scrollHeight - container.scrollTop - viewportHeight <= 80;
    const anchor = nearBottom ? null : topAnchor();
    const next = nearBottom
      ? tailRows(sizes, viewportHeight, initialRows, overscan)
      : sliceRows(
          sizes,
          container.scrollTop - listOffset,
          viewportHeight,
          overscan,
        );

    const changed = next.start !== start || next.end !== end;
    apply(next);
    firstSync = false;
    await tick();

    if (changed) measureMounted();
    applyCurrentSpacers();
    await tick();

    if (anchor) {
      restoreAnchor(anchor);
    } else if (nearBottom) {
      container.scrollTop = container.scrollHeight;
    }
    pinnedToBottom = nearBottom;
    reportLayout();
  }

  function reportLayout() {
    if (!onLayout || !container || !listEl) return;
    const containerRect = container.getBoundingClientRect();
    const listRect = listEl.getBoundingClientRect();
    const offset = listRect.top - containerRect.top + container.scrollTop;
    let virtualTotal = 0;
    for (const size of sizes) virtualTotal += size.height;
    const total = Math.max(1, container.scrollHeight, offset + virtualTotal);
    const tops: Record<string, number> = {};
    let cursor = offset;
    let signature = `${total.toFixed(1)}`;
    for (const size of sizes) {
      const top = ((cursor + size.height / 2) / total) * 100;
      tops[size.key] = top;
      signature += `|${size.key}:${top.toFixed(2)}`;
      cursor += size.height;
    }
    if (signature === layoutSignature) return;
    layoutSignature = signature;
    onLayout({ tops, total });
  }

  async function reveal(key: string) {
    if (!container || !listEl) return;
    const index = keys.indexOf(key);
    if (index < 0) return;
    firstSync = false;
    revealing = true;
    try {
      apply(describeSlice(sizes, Math.max(0, index - 2), Math.min(items.length, index + 3)));
      await tick();
      measureMounted();
      applyCurrentSpacers();
      await tick();
      const node = Array.from(
        listEl.querySelectorAll<HTMLElement>("[data-virtual-key]"),
      ).find((candidate) => candidate.dataset.virtualKey === key);
      if (node) {
        const containerRect = container.getBoundingClientRect();
        const rowRect = node.getBoundingClientRect();
        const room = Math.max(0, container.clientHeight - Math.min(rowRect.height, container.clientHeight));
        const destination =
          container.scrollTop +
          (rowRect.top - containerRect.top) -
          room / 2;
        // A long jump (search, reply, rail) must be instant. Animating across
        // thousands of pixels lets the virtualizer re-window mid-flight and
        // changes the spacers the browser is scrolling through. Short moves
        // still animate.
        const far =
          Math.abs(destination - container.scrollTop) >
          container.clientHeight * 1.5;
        const smooth = !prefersReducedMotion() && !far;
        // The reveal is a deliberate move, not a content growth: the next
        // sync must judge the new geometry instead of following the old
        // "pinned to bottom" state.
        userScrolled = true;
        container.scrollTo({
          top: Math.max(0, destination),
          behavior: smooth ? "smooth" : "auto",
        });
        if (!smooth) {
          // Let the scroll event from an instant jump land before unlocking
          // the viewport window.
          revealing = false;
          schedule();
        }
      }
    } finally {
      if (revealTimer) clearTimeout(revealTimer);
      revealTimer = setTimeout(
        () => {
          revealing = false;
          userScrolled = true;
          schedule();
        },
        prefersReducedMotion() ? 0 : 450,
      );
    }
  }

  // Paging can prepend older messages or slide a tail window forward. The
  // mounted indexes are relative to `items`, so follow the row at the top of
  // the mounted range by key and shift the range to wherever that row moved.
  // Without this, a "load earlier" click would remount a different set of
  // messages for a frame and lose the reader's anchor.
  $effect.pre(() => {
    const nextKeys = keys;
    if (nextKeys.length === 0) {
      trackedStartKey = null;
      trackedStartIndex = 0;
      return;
    }
    if (trackedStartKey === null) {
      const index = Math.max(0, Math.min(start, nextKeys.length - 1));
      trackedStartKey = nextKeys[index] ?? null;
      trackedStartIndex = index;
      return;
    }
    const found = nextKeys.indexOf(trackedStartKey);
    if (found >= 0) {
      const shift = found - trackedStartIndex;
      if (shift !== 0) {
        const count = nextKeys.length;
        start = Math.max(0, Math.min(count, start + shift));
        end = Math.max(start, Math.min(count, end + shift));
      }
    }
    const current = Math.max(0, Math.min(start, nextKeys.length - 1));
    trackedStartKey = nextKeys[current] ?? null;
    trackedStartIndex = current;
  });

  // A new conversation must not inherit the previous thread's row heights.
  $effect.pre(() => {
    if (lastReset === null) {
      lastReset = resetKey;
      return;
    }
    if (resetKey === lastReset) return;
    lastReset = resetKey;
    heights.clear();
    version += 1;
    firstSync = true;
    pinnedToBottom = true;
    userScrolled = false;
    revealing = false;
    start = 0;
    end = 0;
    top = 0;
    bottom = 0;
  });

  // Before the first layout there is no viewport height to measure against.
  // Mount a small tail immediately; the first sync then grows it to fill the
  // screen if the viewport is taller than the estimate.
  $effect.pre(() => {
    if (items.length === 0) {
      if (start !== 0 || end !== 0) {
        start = 0;
        end = 0;
        top = 0;
        bottom = 0;
      }
      return;
    }
    if (end === 0) {
      apply(tailRows(sizes, container?.clientHeight ?? 0, initialRows, 0));
    }
  });

  $effect(() => {
    items;
    version;
    listEl;
    schedule();
  });

  // Jump-to-latest is an explicit request, not a scroll position. The view
  // bumps this counter after resetting its paging window; the next sync mounts
  // the new tail and lands on the true bottom, even if the old scrollTop was
  // far away.
  $effect(() => {
    if (pin === lastPin) return;
    lastPin = pin;
    pinnedToBottom = true;
    userScrolled = false;
    revealing = false;
    schedule();
  });

  $effect(() => {
    const wanted = target;
    if (!wanted) return;
    void (async () => {
      await reveal(wanted);
      target = "";
    })();
  });

  $effect(() => {
    const element = container;
    if (!element) return;
    const onScroll = () => {
      userScrolled = true;
      schedule();
    };
    element.addEventListener("scroll", onScroll, { passive: true });
    const resize =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(() => schedule())
        : null;
    resize?.observe(element);
    schedule();
    return () => {
      element.removeEventListener("scroll", onScroll);
      resize?.disconnect();
    };
  });

  onDestroy(() => {
    if (frame) cancelAnimationFrame(frame);
    if (revealTimer) clearTimeout(revealTimer);
    rowObserver?.disconnect();
    rowObserver = null;
  });
</script>

<div bind:this={listEl} class="virtual-transcript entries-static">
  {#if top > 0}
    <div
      class="virtual-transcript__spacer"
      style={`height:${top}px`}
      aria-hidden="true"
    ></div>
  {/if}
  {#each shown as entry (entry.key)}
    <div
      class={itemClass}
      data-virtual-key={entry.key}
      style={gap > 0 && entry.index > 0 ? `padding-top:${gap}px` : undefined}
      use:measureRow
    >
      {@render row(entry.item, entry.index)}
    </div>
  {/each}
  {#if bottom > 0}
    <div
      class="virtual-transcript__spacer"
      style={`height:${bottom}px`}
      aria-hidden="true"
    ></div>
  {/if}
</div>

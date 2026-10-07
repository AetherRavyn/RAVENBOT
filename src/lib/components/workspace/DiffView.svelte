<script lang="ts">
  /**
   * A unified diff, rendered as a unified diff.
   *
   * The ledger next to this stores two integers per write: lines added, lines
   * deleted. Those answer "how much" and immediately provoke the question they
   * cannot answer — *which* lines. `+31 −12` in `auth.rs` is a claim with no way
   * to check it, and every summary in the world is only as interesting as the
   * ability to expand it into the thing it summarises.
   *
   * What this does that a `<pre>` would not:
   *
   *  - **Two line-number columns**, the old side and the new. They are what
   *    make a hunk navigable: without them a run of `+` lines does not say
   *    where in the file it landed, and reading a diff becomes guessing.
   *  - **Headers are detected by adjacency, not by prefix.** `--- ` also
   *    describes a deleted line whose content begins with `-- `. Only a
   *    `---`/`+++` pair whose neighbour is the other half is a filename, so a
   *    removal is never mistaken for a path.
   *  - **No `{@html}`.** Every line is text, rendered as text. A diff of a file
   *    that contains `<script>` is a diff of a file that contains `<script>`,
   *    and it is not the diff's job to execute it.
   *
   * The marker column carries `+`, `-` and a space, but the *background* is
   * what does the work — the same green and red the ledger's badges use, so a
   * reader who has seen a `+31` badge and a reader who has used git for twenty
   * years are looking at the same colour twice.
   */
  interface Props {
    diff: string;
  }

  let { diff }: Props = $props();

  type Kind = "add" | "del" | "ctx" | "hunk" | "head" | "meta";

  interface DiffLine {
    kind: Kind;
    /** The line's text with its `+` / `-` / space marker removed. */
    text: string;
    old: number | null;
    new: number | null;
  }

  /**
   * Parse once per diff, not once per frame.
   *
   * `$derived.by` runs on read and caches until `diff` changes, which matters
   * because a patch is hundreds of lines and this component sits inside a list
   * that re-renders whenever anything in the view changes.
   */
  const lines = $derived.by((): DiffLine[] => {
    const raw = diff.split("\n");
    const out: DiffLine[] = [];
    // Counters start one below the hunk's own first line, because the header
    // numbers the line the hunk *begins* at and the first row of the hunk is
    // that line. Starting at the header's number would put every column one
    // high from the first row onwards — the kind of off-by-one that makes a
    // correct diff look wrong to anyone who can read git's output.
    let oldNo = 0;
    let newNo = 0;

    for (let i = 0; i < raw.length; i++) {
      const line = raw[i];
      // The split leaves one empty element after a trailing newline. It is not
      // a line of the file, and rendering it would add a blank row to every
      // diff in the app.
      if (line === "" && i === raw.length - 1) break;

      const prev = i > 0 ? raw[i - 1] : "";
      const next = i + 1 < raw.length ? raw[i + 1] : "";
      const isHeader =
        (line.startsWith("--- ") && (next.startsWith("+++ ") || prev.startsWith("+++ "))) ||
        (line.startsWith("+++ ") && (next.startsWith("--- ") || prev.startsWith("--- ")));

      if (isHeader) {
        out.push({ kind: "head", text: line, old: null, new: null });
        continue;
      }

      if (line.startsWith("@@")) {
        const m = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
        oldNo = (m ? Number(m[1]) : 1) - 1;
        newNo = (m ? Number(m[2]) : 1) - 1;
        out.push({ kind: "hunk", text: line, old: null, new: null });
        continue;
      }

      const marker = line[0];
      if (marker === "+") {
        newNo += 1;
        out.push({ kind: "add", text: line.slice(1), old: null, new: newNo });
      } else if (marker === "-") {
        oldNo += 1;
        out.push({ kind: "del", text: line.slice(1), old: oldNo, new: null });
      } else if (marker === " " || line === "") {
        oldNo += 1;
        newNo += 1;
        out.push({ kind: "ctx", text: line === "" ? "" : line.slice(1), old: oldNo, new: newNo });
      } else {
        // `\ No newline at end of file`, a stray `diff --git`, anything the
        // emitter wrote that is not a change. Kept rather than dropped: it is
        // part of what was recorded, and silently removing lines from a view
        // of a diff is the one thing a view of a diff must not do.
        out.push({ kind: "meta", text: line, old: null, new: null });
      }
    }

    return out;
  });

  /** Added / removed / context counts, for the strip above the hunks. */
  const summary = $derived.by(() => {
    let added = 0;
    let removed = 0;
    for (const l of lines) {
      if (l.kind === "add") added += 1;
      else if (l.kind === "del") removed += 1;
    }
    return { added, removed };
  });
</script>

<div class="dif">
  <div class="dif__strip">
    <span class="dif__count">
      <span class="proj__add">+{summary.added}</span>
      <span class="proj__del">−{summary.removed}</span>
    </span>
    {#if lines.length}
      <span class="dif__lines">{lines.length}</span>
    {/if}
  </div>

  <div class="dif__body" role="group">
    {#each lines as l, i (i)}
      {#if l.kind === "add" || l.kind === "del" || l.kind === "ctx"}
        <div class="dif__row" data-k={l.kind}>
          <span class="dif__no" aria-hidden="true">{l.old ?? ""}</span>
          <span class="dif__no" aria-hidden="true">{l.new ?? ""}</span>
          <span class="dif__mark" aria-hidden="true"
            >{l.kind === "add" ? "+" : l.kind === "del" ? "−" : " "}</span
          >
          <span class="dif__text">{l.text}</span>
        </div>
      {:else}
        <div class="dif__row" data-k={l.kind}>
          <span class="dif__text">{l.text}</span>
        </div>
      {/if}
    {/each}
  </div>
</div>

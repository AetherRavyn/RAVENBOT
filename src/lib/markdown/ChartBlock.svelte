<script lang="ts">
  /**
   * A dependency-free chart renderer for fenced `chart` blocks.
   *
   * The model writes JSON, the reader gets an SVG. Keeping this local (rather
   * than pulling a charting runtime into the bundle) means a chart renders
   * during streaming, works offline, inherits the app theme, and never runs
   * third-party script inside a message.
   *
   * Accepted shape:
   *   { "type": "bar", "title": "...", "labels": ["Q1"], "series": [{"name":"Revenue","values":[12]}] }
   * Also accepted: `data` rows with `x`/`y`/`series` keys, or a numeric array.
   */

  interface Props {
    code: string;
    streaming?: boolean;
  }

  let { code, streaming = false }: Props = $props();
  let svgElement = $state<SVGSVGElement | null>(null);

  function downloadSvg() {
    if (!svgElement) return;
    const clone = svgElement.cloneNode(true) as SVGSVGElement;
    clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
    const blob = new Blob([new XMLSerializer().serializeToString(clone)], {
      type: "image/svg+xml",
    });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    const safeTitle =
      (title || "chart")
        .replace(/[^a-z0-9-_]+/gi, "-")
        .replace(/^-|-$/g, "") || "chart";
    anchor.download = `${safeTitle}.svg`;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  type ChartType = "bar" | "line" | "area" | "pie" | "donut" | "scatter";
  type Series = { name: string; values: number[]; color?: string };

  interface Normalized {
    type: ChartType;
    title: string;
    subtitle: string;
    labels: string[];
    series: Series[];
    stacked: boolean;
    yLabel: string;
    xLabel: string;
  }

  interface Point {
    key: string;
    x: number;
    y: number;
    value: number;
    label: string;
    series: string;
    color: string;
  }

  interface Bar {
    key: string;
    x: number;
    y: number;
    width: number;
    height: number;
    value: number;
    label: string;
    series: string;
    color: string;
  }

  interface Slice {
    key: string;
    path: string;
    value: number;
    label: string;
    color: string;
    percent: number;
  }

  interface Layout {
    width: number;
    height: number;
    left: number;
    top: number;
    plotWidth: number;
    plotHeight: number;
    ticks: number[];
    yFor: (value: number) => number;
    xFor: (index: number) => number;
    baseline: number;
    bars: Bar[];
    points: Point[];
    slices: Slice[];
    linePaths: { key: string; d: string; color: string }[];
    areaPaths: { key: string; d: string; color: string }[];
    xLabels: { index: number; x: number; text: string; full: string }[];
    legend: { label: string; color: string }[];
    summary: string;
    singleSlice: boolean;
    centerX: number;
    centerY: number;
    outerRadius: number;
    innerRadius: number;
  }

  const WIDTH = 760;
  const HEIGHT = 380;
  const PALETTE = [
    "var(--chart-1)",
    "var(--chart-2)",
    "var(--chart-3)",
    "var(--chart-other)",
    "var(--success)",
    "var(--warning)",
    "var(--danger)",
    "var(--brand)",
  ];

  function toNumber(value: unknown): number | null {
    if (typeof value === "number") return Number.isFinite(value) ? value : null;
    if (typeof value === "string" && value.trim() !== "") {
      const parsed = Number(value);
      return Number.isFinite(parsed) ? parsed : null;
    }
    return null;
  }

  function unique(values: string[]): string[] {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const value of values) {
      if (!seen.has(value)) {
        seen.add(value);
        out.push(value);
      }
    }
    return out;
  }

  function firstStringKey(row: Record<string, unknown>): string | null {
    for (const [key, value] of Object.entries(row)) {
      if (typeof value === "string") return key;
    }
    return null;
  }

  function firstNumberKey(row: Record<string, unknown>): string | null {
    for (const [key, value] of Object.entries(row)) {
      if (toNumber(value) !== null) return key;
    }
    return null;
  }

  function normalize(raw: unknown): Normalized {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
      throw new Error("The chart block must be a JSON object.");
    }
    const spec = raw as Record<string, unknown>;
    const typeRaw = String(spec.type ?? "bar").toLowerCase();
    const allowed: ChartType[] = ["bar", "line", "area", "pie", "donut", "scatter"];
    if (!allowed.includes(typeRaw as ChartType)) {
      throw new Error(`Unknown chart type "${typeRaw}". Use bar, line, area, pie, donut, or scatter.`);
    }
    const type = typeRaw as ChartType;

    let labels: string[] = Array.isArray(spec.labels) ? spec.labels.map(String) : [];
    let series: Series[] = [];

    if (Array.isArray(spec.series)) {
      series = spec.series.map((entry, index) => {
        if (!entry || typeof entry !== "object") {
          throw new Error(`series[${index}] must be an object with a values array.`);
        }
        const item = entry as Record<string, unknown>;
        const values = Array.isArray(item.values) ? item.values.map(toNumber) : [];
        if (values.some((value) => value === null)) {
          throw new Error(`series[${index}].values contains a value that is not a number.`);
        }
        return {
          name: String(item.name ?? `Series ${index + 1}`),
          values: values as number[],
          color: typeof item.color === "string" ? item.color : undefined,
        };
      });
    } else if (Array.isArray(spec.data)) {
      const rows = spec.data;
      if (rows.every((row) => toNumber(row) !== null)) {
        labels = rows.map((_, index) => String(index + 1));
        series = [{ name: String(spec.yLabel ?? "Value"), values: rows.map((row) => toNumber(row) as number) }];
      } else if (rows.every((row) => row && typeof row === "object" && !Array.isArray(row))) {
        const records = rows as Record<string, unknown>[];
        const xKey = String(spec.x ?? firstStringKey(records[0]) ?? "label");
        const yKey = String(spec.y ?? firstNumberKey(records[0]) ?? "value");
        const seriesKey = typeof spec.seriesKey === "string" ? spec.seriesKey : null;
        labels = unique(records.map((row) => String(row[xKey] ?? "")));
        if (seriesKey) {
          const names = unique(records.map((row) => String(row[seriesKey] ?? "")));
          series = names.map((name) => ({
            name,
            values: labels.map((label) => {
              const matches = records.filter(
                (row) => String(row[xKey] ?? "") === label && String(row[seriesKey] ?? "") === name,
              );
              return matches.reduce((total, row) => total + (toNumber(row[yKey]) ?? 0), 0);
            }),
          }));
        } else {
          series = [
            {
              name: String(spec.yLabel ?? yKey),
              values: labels.map((label) => {
                const row = records.find((candidate) => String(candidate[xKey] ?? "") === label);
                return row ? (toNumber(row[yKey]) ?? 0) : 0;
              }),
            },
          ];
        }
      } else {
        throw new Error("`data` must be an array of numbers or an array of objects.");
      }
    }

    if (series.length === 0) {
      throw new Error("No chart data. Provide `series` or `data`.");
    }
    if (labels.length === 0) {
      labels = series[0].values.map((_, index) => String(index + 1));
    }
    series = series.map((entry) => ({
      ...entry,
      values: labels.map((_, index) => entry.values[index] ?? 0),
    }));
    if (labels.length === 0) {
      throw new Error("The chart has no points.");
    }

    const title = typeof spec.title === "string" ? spec.title.trim() : "";
    const subtitle = typeof spec.subtitle === "string" ? spec.subtitle.trim() : "";
    return {
      type,
      title,
      subtitle,
      labels,
      series,
      stacked: spec.stacked === true && type === "bar",
      yLabel: typeof spec.yLabel === "string" ? spec.yLabel.trim() : "",
      xLabel: typeof spec.xLabel === "string" ? spec.xLabel.trim() : "",
    };
  }

  function niceScale(minValue: number, maxValue: number) {
    let min = minValue;
    let max = maxValue;
    if (!Number.isFinite(min) || !Number.isFinite(max)) {
      min = 0;
      max = 1;
    }
    if (min === max) {
      if (min === 0) {
        max = 1;
      } else {
        min = Math.min(0, min);
        max = min + Math.abs(min) * 0.2;
      }
    }
    const span = max - min || 1;
    const rough = span / 4;
    const magnitude = Math.pow(10, Math.floor(Math.log10(rough)));
    const normalized = rough / magnitude;
    const step =
      (normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10) * magnitude;
    const low = Math.floor(min / step) * step;
    const high = Math.ceil(max / step) * step;
    const ticks: number[] = [];
    for (let value = low; value <= high + step / 2; value += step) {
      ticks.push(Number(value.toFixed(10)));
      if (ticks.length > 24) break;
    }
    return { min: low, max: high, ticks };
  }

  function formatTick(value: number): string {
    const abs = Math.abs(value);
    if (abs >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(1)}B`;
    if (abs >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (abs >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
    if (abs > 0 && abs < 1) return value.toFixed(2);
    return String(Number(value.toFixed(2)));
  }

  function clampLabel(value: string, max = 12): string {
    return value.length > max ? `${value.slice(0, max - 1)}…` : value;
  }

  function polar(cx: number, cy: number, radius: number, angle: number) {
    return { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) };
  }

  function arcPath(
    cx: number,
    cy: number,
    outer: number,
    inner: number,
    start: number,
    end: number,
  ): string {
    const large = end - start > Math.PI ? 1 : 0;
    const p0 = polar(cx, cy, outer, start);
    const p1 = polar(cx, cy, outer, end);
    const p2 = polar(cx, cy, inner, end);
    const p3 = polar(cx, cy, inner, start);
    return [
      `M ${p0.x.toFixed(2)} ${p0.y.toFixed(2)}`,
      `A ${outer} ${outer} 0 ${large} 1 ${p1.x.toFixed(2)} ${p1.y.toFixed(2)}`,
      `L ${p2.x.toFixed(2)} ${p2.y.toFixed(2)}`,
      `A ${inner} ${inner} 0 ${large} 0 ${p3.x.toFixed(2)} ${p3.y.toFixed(2)}`,
      "Z",
    ].join(" ");
  }

  function buildLayout(chart: Normalized): Layout {
    const hasTitle = chart.title.length > 0;
    const left = chart.type === "pie" || chart.type === "donut" ? 30 : 58;
    const top = hasTitle ? 54 : 24;
    const right = 22;
    const bottom = 58;
    const plotWidth = WIDTH - left - right;
    const plotHeight = HEIGHT - top - bottom;
    const legend = seriesLegend(chart);
    const centerX = 250;
    const centerY = top + plotHeight / 2;
    const outerRadius = Math.min(plotWidth * 0.32, plotHeight * 0.45);
    const innerRadius = chart.type === "donut" ? outerRadius * 0.58 : 0;

    if (chart.type === "pie" || chart.type === "donut") {
      const values = chart.series[0].values.map((value) => Math.max(0, value));
      const total = values.reduce((sum, value) => sum + value, 0);
      if (total <= 0) throw new Error("Pie and donut charts need at least one positive value.");
      const slices: Slice[] = [];
      let angle = -Math.PI / 2;
      const singleSlice = values.filter((value) => value > 0).length === 1;
      chart.labels.forEach((label, index) => {
        const value = values[index];
        if (value <= 0) return;
        const next = angle + (value / total) * Math.PI * 2;
        const color = PALETTE[index % PALETTE.length];
        slices.push({
          key: `${label}-${index}`,
          path: singleSlice ? "" : arcPath(centerX, centerY, outerRadius, innerRadius, angle, next),
          value,
          label,
          color,
          percent: (value / total) * 100,
        });
        angle = next;
      });
      return blankLayout(chart, left, top, plotWidth, plotHeight, legend, {
        slices,
        singleSlice,
        centerX,
        centerY,
        outerRadius,
        innerRadius,
        summary: `${chart.type} chart: ${chart.labels
          .map((label, index) => `${label} ${formatTick(values[index])}`)
          .join(", ")}`,
      });
    }

    const flatValues = chart.series.flatMap((entry) => entry.values);
    const stackedValues = chart.labels.map((_, index) =>
      chart.series.reduce((sum, entry) => sum + (entry.values[index] ?? 0), 0),
    );
    const valuesForScale = chart.stacked ? stackedValues : flatValues;
    const minValue = Math.min(0, ...valuesForScale);
    const maxValue = Math.max(0, ...valuesForScale);
    const scale = niceScale(minValue, maxValue);
    const yFor = (value: number) =>
      top + plotHeight - ((value - scale.min) / (scale.max - scale.min || 1)) * plotHeight;
    const xFor = (index: number) =>
      left + (chart.labels.length <= 1 ? plotWidth / 2 : (plotWidth / chart.labels.length) * (index + 0.5));
    const baseline = yFor(0);

    const bars: Bar[] = [];
    if (chart.type === "bar") {
      const groupWidth = plotWidth / chart.labels.length;
      const barWidth = Math.min(38, (groupWidth * 0.68) / chart.series.length);
      chart.labels.forEach((label, index) => {
        let positiveStack = 0;
        let negativeStack = 0;
        chart.series.forEach((entry, seriesIndex) => {
          const value = entry.values[index] ?? 0;
          const color = seriesColor(seriesIndex, entry.color);
          if (chart.stacked) {
            if (value === 0) return;
            const start = value >= 0 ? positiveStack : negativeStack;
            const end = start + value;
            if (value >= 0) positiveStack = end;
            else negativeStack = end;
            const yStart = yFor(start);
            const yEnd = yFor(end);
            const x = left + groupWidth * index + groupWidth * 0.16;
            bars.push({
              key: `${index}-${seriesIndex}-${label}`,
              x,
              y: Math.min(yStart, yEnd),
              width: groupWidth * 0.68,
              height: Math.max(1, Math.abs(yEnd - yStart)),
              value,
              label,
              series: entry.name,
              color,
            });
          } else {
            const x = left + groupWidth * index + groupWidth * 0.16 + seriesIndex * barWidth;
            const y = yFor(Math.max(0, value));
            const height = Math.abs(yFor(value) - baseline);
            bars.push({
              key: `${index}-${seriesIndex}-${label}`,
              x,
              y: value >= 0 ? y : baseline,
              width: barWidth,
              height: Math.max(value === 0 ? 0 : 1, height),
              value,
              label,
              series: entry.name,
              color,
            });
          }
        });
      });
    }

    const points: Point[] = [];
    const linePaths: { key: string; d: string; color: string }[] = [];
    const areaPaths: { key: string; d: string; color: string }[] = [];
    if (chart.type !== "bar") {
      chart.series.forEach((entry, seriesIndex) => {
        const color = seriesColor(seriesIndex, entry.color);
        const coordinates = entry.values.map((value, index) => ({
          key: `${seriesIndex}-${index}`,
          x: xFor(index),
          y: yFor(value),
          value,
          label: chart.labels[index],
          series: entry.name,
          color,
        }));
        points.push(...coordinates);
        if (coordinates.length > 0) {
          const d = coordinates
            .map((point, index) => `${index === 0 ? "M" : "L"} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`)
            .join(" ");
          if (chart.type === "line") linePaths.push({ key: `${seriesIndex}-${entry.name}`, d, color });
          if (chart.type === "area") {
            const area = `${d} L ${coordinates[coordinates.length - 1].x.toFixed(2)} ${baseline.toFixed(2)} L ${coordinates[0].x.toFixed(2)} ${baseline.toFixed(2)} Z`;
            areaPaths.push({ key: entry.name, d: area, color });
            linePaths.push({ key: `${seriesIndex}-${entry.name}`, d, color });
          }
        }
      });
    }

    const labelStep = chart.labels.length > 14 ? Math.ceil(chart.labels.length / 10) : 1;
    const xLabels = chart.labels
      .map((label, index) => ({
        index,
        x: xFor(index),
        text: clampLabel(label, 11),
        full: label,
      }))
      .filter((_, index) => index % labelStep === 0);

    return {
      width: WIDTH,
      height: HEIGHT,
      left,
      top,
      plotWidth,
      plotHeight,
      ticks: scale.ticks,
      yFor,
      xFor,
      baseline,
      bars,
      points,
      slices: [],
      linePaths,
      areaPaths,
      xLabels,
      legend,
      summary: `${chart.type} chart: ${chart.labels
        .slice(0, 8)
        .map((label, index) => `${label} ${formatTick(stackedValues[index] ?? flatValues[index] ?? 0)}`)
        .join(", ")}`,
      singleSlice: false,
      centerX,
      centerY,
      outerRadius,
      innerRadius,
    };
  }

  function blankLayout(
    chart: Normalized,
    left: number,
    top: number,
    plotWidth: number,
    plotHeight: number,
    legend: { label: string; color: string }[],
    extra: Partial<Layout>,
  ): Layout {
    return {
      width: WIDTH,
      height: HEIGHT,
      left,
      top,
      plotWidth,
      plotHeight,
      ticks: [],
      yFor: () => top,
      xFor: () => left,
      baseline: top + plotHeight,
      bars: [],
      points: [],
      slices: [],
      linePaths: [],
      areaPaths: [],
      xLabels: [],
      legend,
      summary: chart.title || `${chart.type} chart`,
      singleSlice: false,
      centerX: 250,
      centerY: top + plotHeight / 2,
      outerRadius: 0,
      innerRadius: 0,
      ...extra,
    };
  }

  function seriesColor(index: number, explicit?: string): string {
    return explicit && explicit.trim() ? explicit : PALETTE[index % PALETTE.length];
  }

  function seriesLegend(chart: Normalized): { label: string; color: string }[] {
    if (chart.type === "pie" || chart.type === "donut") {
      return chart.labels
        .map((label, index) => ({
          label,
          color: PALETTE[index % PALETTE.length],
          value: chart.series[0].values[index] ?? 0,
        }))
        .filter((entry) => entry.value > 0)
        .map(({ label, color }) => ({ label, color }));
    }
    return chart.series.map((entry, index) => ({
      label: entry.name,
      color: seriesColor(index, entry.color),
    }));
  }

  const parsed = $derived.by(() => {
    try {
      const chart = normalize(JSON.parse(code));
      return { chart, layout: buildLayout(chart), error: "" };
    } catch (error) {
      return {
        chart: null,
        layout: null,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  });

  const showSkeleton = $derived(Boolean(parsed.error) && streaming);
  const title = $derived(parsed.chart?.title ?? "");
  const subtitle = $derived(parsed.chart?.subtitle ?? "");
</script>

{#if showSkeleton}
  <figure class="md-chart md-chart--loading" aria-label="Chart is being written">
    <div class="md-chart-skeleton">
      <span class="md-chart-skeleton-bar" style="height: 42%"></span>
      <span class="md-chart-skeleton-bar" style="height: 68%"></span>
      <span class="md-chart-skeleton-bar" style="height: 54%"></span>
      <span class="md-chart-skeleton-bar" style="height: 81%"></span>
      <span class="md-chart-skeleton-bar" style="height: 63%"></span>
    </div>
    <figcaption>Building chart…</figcaption>
  </figure>
{:else if parsed.error || !parsed.chart || !parsed.layout}
  <figure class="md-chart md-chart--error" aria-label="Chart could not be rendered">
    <figcaption>Chart could not be rendered</figcaption>
    <p class="md-chart-error-text">{parsed.error}</p>
    <pre class="md-chart-source">{code}</pre>
  </figure>
{:else}
  {@const chart = parsed.chart}
  {@const layout = parsed.layout}
  <figure class="md-chart" aria-label={title || "Data chart"}>
    {#if title || subtitle}
      <figcaption class="md-chart-caption">
        <div class="md-chart-caption-text">
          {#if title}<span class="md-chart-title">{title}</span>{/if}
          {#if subtitle}<span class="md-chart-subtitle">{subtitle}</span>{/if}
        </div>
        <button type="button" class="md-chart-download" onclick={downloadSvg} title="Download SVG">
          SVG
        </button>
      </figcaption>
    {/if}

    <svg
      bind:this={svgElement}
      class="md-chart-svg"
      viewBox="0 0 {layout.width} {layout.height}"
      role="img"
      aria-label={layout.summary}
      preserveAspectRatio="xMidYMid meet"
    >
      {#if chart.type === "pie" || chart.type === "donut"}
        {#if layout.singleSlice}
          {#each layout.slices as slice (slice.key)}
            {#if chart.type === "donut"}
              <circle
                cx={layout.centerX}
                cy={layout.centerY}
                r={(layout.outerRadius + layout.innerRadius) / 2}
                fill="none"
                stroke={slice.color}
                stroke-width={layout.outerRadius - layout.innerRadius}
              >
                <title>{slice.label}: {formatTick(slice.value)} ({slice.percent.toFixed(1)}%)</title>
              </circle>
            {:else}
              <circle cx={layout.centerX} cy={layout.centerY} r={layout.outerRadius} fill={slice.color}>
                <title>{slice.label}: {formatTick(slice.value)} ({slice.percent.toFixed(1)}%)</title>
              </circle>
            {/if}
          {/each}
        {:else}
          {#each layout.slices as slice (slice.key)}
            <path d={slice.path} fill={slice.color} stroke="var(--surface-1)" stroke-width="2">
              <title>{slice.label}: {formatTick(slice.value)} ({slice.percent.toFixed(1)}%)</title>
            </path>
          {/each}
        {/if}
        {#if chart.type === "donut"}
          <text
            x={layout.centerX}
            y={layout.centerY - 4}
            text-anchor="middle"
            class="md-chart-donut-total"
          >{formatTick(layout.slices.reduce((sum, slice) => sum + slice.value, 0))}</text>
          <text
            x={layout.centerX}
            y={layout.centerY + 16}
            text-anchor="middle"
            class="md-chart-donut-label"
          >total</text>
        {/if}
        <g class="md-chart-pie-legend">
          {#each layout.legend as item, index (index)}
            {@const slice = layout.slices[index]}
            <rect x="470" y={layout.top + index * 24} width="10" height="10" rx="2" fill={item.color} />
            <text x="488" y={layout.top + index * 24 + 9} class="md-chart-legend-text">
              {item.label} {#if slice}<tspan class="md-chart-legend-value">{slice.percent.toFixed(0)}%</tspan>{/if}
            </text>
          {/each}
        </g>
      {:else}
        {#each layout.ticks as tick (tick)}
          {@const y = layout.yFor(tick)}
          <line x1={layout.left} y1={y} x2={layout.left + layout.plotWidth} y2={y} class="md-chart-grid" />
          <text x={layout.left - 10} y={y + 4} text-anchor="end" class="md-chart-axis">{formatTick(tick)}</text>
        {/each}
        <line
          x1={layout.left}
          y1={layout.baseline}
          x2={layout.left + layout.plotWidth}
          y2={layout.baseline}
          class="md-chart-axis-line"
        />

        {#if chart.type === "bar"}
          {#each layout.bars as bar (bar.key)}
            <rect x={bar.x} y={bar.y} width={bar.width} height={bar.height} rx="3" fill={bar.color}>
              <title>{bar.series} · {bar.label}: {formatTick(bar.value)}</title>
            </rect>
          {/each}
        {:else}
          {#each layout.areaPaths as area (area.key)}
            <path d={area.d} fill={area.color} opacity="0.18" />
          {/each}
          {#each layout.linePaths as line (line.key)}
            <path d={line.d} fill="none" stroke={line.color} stroke-width="2.4" stroke-linejoin="round" stroke-linecap="round" />
          {/each}
          {#each layout.points as point (point.key)}
            <circle cx={point.x} cy={point.y} r="3.2" fill={point.color} stroke="var(--surface-1)" stroke-width="1.2">
              <title>{point.series} · {point.label}: {formatTick(point.value)}</title>
            </circle>
          {/each}
        {/if}

        {#each layout.xLabels as label (label.index)}
          <text x={label.x} y={layout.top + layout.plotHeight + 20} text-anchor="middle" class="md-chart-axis">
            {label.text}
            <title>{label.full}</title>
          </text>
        {/each}

        {#if chart.yLabel}
          <text
            x={layout.left - 10}
            y={layout.top - 14}
            text-anchor="end"
            class="md-chart-axis-title"
          >{chart.yLabel}</text>
        {/if}
        {#if chart.xLabel}
          <text
            x={layout.left + layout.plotWidth}
            y={layout.top + layout.plotHeight + 44}
            text-anchor="end"
            class="md-chart-axis-title"
          >{chart.xLabel}</text>
        {/if}
      {/if}
    </svg>

    {#if layout.legend.length > 0 && chart.type !== "pie" && chart.type !== "donut"}
      <div class="md-chart-legend" aria-label="Legend">
        {#each layout.legend as item, index (index)}
          <span class="md-chart-legend-item">
            <span class="md-chart-swatch" style="background: {item.color}"></span>
            {item.label}
          </span>
        {/each}
      </div>
    {/if}
  </figure>
{/if}

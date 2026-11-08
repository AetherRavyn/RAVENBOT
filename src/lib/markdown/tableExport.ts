/**
 * Table export helpers.
 *
 * A rendered table is data a reader often needs somewhere else. Markdown keeps
 * the table editable; CSV opens in a spreadsheet. Neither is a screenshot, and
 * neither changes the values the model wrote.
 */

export interface ExportableTable {
  headers: readonly string[];
  rows: readonly (readonly string[])[];
}

export function tableToMarkdown(table: ExportableTable): string {
  const row = (cells: readonly string[]) =>
    `| ${cells.map(escapeCell).join(" | ")} |`;
  const separator = table.headers.map(() => "---");
  return [row(table.headers), `| ${separator.join(" | ")} |`, ...table.rows.map(row)].join(
    "\n",
  );
}

export function tableToCsv(table: ExportableTable): string {
  return [table.headers, ...table.rows]
    .map((cells) =>
      cells
        .map((cell) => (/[",\r\n]/u.test(cell) ? `"${cell.replaceAll('"', '""')}"` : cell))
        .join(","),
    )
    .join("\n");
}

function escapeCell(cell: string): string {
  return cell.replace(/\\(?=[\\|]|$)/gu, "\\\\").replace(/\|/gu, "\\|");
}

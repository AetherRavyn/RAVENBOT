import { describe, expect, it } from "vitest";
import {
  missingRequiredFields,
  serializeBlockResponse,
  submitLabel,
  type BlockField,
  type InteractionBlock,
} from "./blocks";

describe("interaction blocks", () => {
  it("serializes the response the runtime parses", () => {
    expect(
      JSON.parse(
        serializeBlockResponse({
          action_id: "deploy",
          values: { env: "production", selected: ["a"] },
        }),
      ),
    ).toEqual({
      action_id: "deploy",
      values: { env: "production", selected: ["a"] },
    });
  });

  it("uses the payload's submit label or the fallback", () => {
    const block: InteractionBlock = { submit_label: "  Send request  " };
    expect(submitLabel(block, "Submit")).toBe("Send request");
    expect(submitLabel({}, "Submit")).toBe("Submit");
  });

  it("finds only required fields with an empty value", () => {
    const fields: BlockField[] = [
      { id: "name", label: "Name", required: true },
      { id: "note", label: "Note" },
      { id: "team", label: "Team", required: true },
    ];
    expect(missingRequiredFields(fields, { name: "Ada", team: " " })).toEqual([
      fields[2],
    ]);
  });
});

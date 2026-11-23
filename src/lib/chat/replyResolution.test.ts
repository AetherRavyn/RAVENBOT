import { describe, expect, it } from "vitest";
import { mergeReplyTargets, replyTargetsToLoad } from "./replyResolution";

describe("reply resolution", () => {
  it("requests only missing, non-pending reply targets", () => {
    const messages = [
      { id: "a", reply_to_id: "old-1" },
      { id: "b", reply_to_id: "old-1" },
      { id: "c", reply_to_id: "present" },
      { id: "d", reply_to_id: null },
    ];
    expect(
      replyTargetsToLoad(messages, new Set(["present"]), new Set()),
    ).toEqual(["old-1"]);
    expect(
      replyTargetsToLoad(messages, new Set(["present"]), new Set(["old-1"])),
    ).toEqual([]);
  });

  it("merges fetched targets without duplicates", () => {
    const merged = mergeReplyTargets(
      [{ id: "a" }, { id: "b" }],
      [{ id: "b" }, { id: "old" }],
    );
    expect(merged.map((message) => message.id)).toEqual(["a", "b", "old"]);
  });
});

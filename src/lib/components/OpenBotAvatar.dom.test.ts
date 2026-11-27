// @vitest-environment jsdom
/**
 * The OpenBot component.
 *
 * The package is mocked, because the point here is the *contract*: which
 * definition/colour/state reaches the renderer, that an update is a diff
 * rather than a remount, and that a host which cannot draw falls back to the
 * same character as a still SVG.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";

const mountAvatar = vi.hoisted(() => vi.fn());

vi.mock("open-bot-avatar/dom", () => ({ mountAvatar }));

import OpenBotAvatar from "./OpenBotAvatar.svelte";
import { OPENBOT_STATES } from "$lib/openBotAvatar";

class FakeResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

let instances: Array<{ update: ReturnType<typeof vi.fn>; destroy: ReturnType<typeof vi.fn> }>;

beforeEach(() => {
  instances = [];
  mountAvatar.mockReset();
  mountAvatar.mockImplementation((host: HTMLElement, options: any) => {
    host.setAttribute("data-mounted", options.definition.id);
    host.setAttribute("data-state", String(options.state));
    const avatar = { update: vi.fn(), destroy: vi.fn() };
    instances.push(avatar);
    return avatar;
  });
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  vi.stubGlobal("CanvasRenderingContext2D", class {});
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({} as never);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("OpenBotAvatar — live host", () => {
  it("mounts the chosen definition, colour, state and mood", async () => {
    render(OpenBotAvatar, {
      props: {
        name: "Ada",
        definitionId: "cat",
        colour: "#f8a01c",
        state: "working",
        moodTone: "happy",
      },
    });

    await waitFor(() => expect(mountAvatar).toHaveBeenCalledTimes(1));
    const [, options] = mountAvatar.mock.calls[0];
    expect(options.definition.id).toBe("cat");
    expect(options.color).toBe("#f8a01c");
    expect(options.state).toBe("working");
    expect(options.mood).toBe("happy");
    expect(options.label).toBe("Ada");
    expect(options.followPointer).toBe(false);
    expect(options.depth).toBe(6);
  });

  it("mounts the render options stored in the URL", async () => {
    render(OpenBotAvatar, {
      props: {
        name: "Ada",
        imageUrl: "openbot://ghost?color=%23f4f4f4&depth=11&follow=1&renderer=scene",
      },
    });
    await waitFor(() => expect(mountAvatar).toHaveBeenCalledTimes(1));
    const [, options] = mountAvatar.mock.calls[0];
    expect(options.definition.id).toBe("ghost");
    expect(options.color).toBe("#f4f4f4");
    expect(options.depth).toBe(11);
    expect(options.followPointer).toBe(true);
    expect(options.renderer).toBe("scene");
  });

  it("lets explicit props win over the URL's render options", async () => {
    render(OpenBotAvatar, {
      props: {
        name: "Ada",
        imageUrl:
          "openbot://ghost?color=%23f4f4f4&depth=11&follow=1&finish=glass&renderer=scene",
        depth: 2,
        followPointer: false,
        material: "flat",
        renderer: "canvas",
      },
    });
    await waitFor(() => expect(mountAvatar).toHaveBeenCalledTimes(1));
    const [, options] = mountAvatar.mock.calls[0];
    expect(options.depth).toBe(2);
    expect(options.followPointer).toBe(false);
    expect(options.renderer).toBe("canvas");
    expect(options.material).toBe("flat");
  });

  it("loads the finish named by the URL after mounting", async () => {
    render(OpenBotAvatar, {
      props: {
        name: "Ada",
        imageUrl: "openbot://ghost?color=%23f4f4f4&finish=glass",
      },
    });
    await waitFor(() => expect(mountAvatar).toHaveBeenCalledTimes(1));
    await waitFor(() => {
      const updates = instances[0]?.update.mock.calls ?? [];
      expect(
        updates.some(([changed]) => changed.material && changed.material !== "flat"),
      ).toBe(true);
    });
  });

  it("takes its state from the live mood unless the studio overrides it", async () => {
    const { rerender } = render(OpenBotAvatar, {
      props: { name: "Ada", definitionId: "ghost", mood: "failed" },
    });
    await waitFor(() => expect(mountAvatar).toHaveBeenCalledTimes(1));
    expect(mountAvatar.mock.calls[0][1].state).toBe("blocked");

    await rerender({ name: "Ada", definitionId: "ghost", mood: "responded" });
    await tick();
    expect(instances[0].update).toHaveBeenCalledWith({ state: "done", mood: "happy" });
    expect(mountAvatar).toHaveBeenCalledTimes(1);
  });

  it("updates only the changed option and keeps one mount", async () => {
    const { rerender } = render(OpenBotAvatar, {
      props: { name: "Ada", definitionId: "cat", colour: "#f8a01c", depth: 6 },
    });
    await waitFor(() => expect(mountAvatar).toHaveBeenCalledTimes(1));

    await rerender({
      name: "Ada",
      definitionId: "cat",
      colour: "#1fbf68",
      depth: 9,
      followPointer: true,
    });
    await tick();
    expect(instances[0].update).toHaveBeenCalledTimes(1);
    expect(instances[0].update).toHaveBeenCalledWith({
      color: "#1fbf68",
      depth: 9,
      followPointer: true,
    });
    expect(mountAvatar).toHaveBeenCalledTimes(1);
  });

  it("destroys the avatar on unmount", async () => {
    const { unmount } = render(OpenBotAvatar, {
      props: { name: "Ada", definitionId: "cat" },
    });
    await waitFor(() => expect(mountAvatar).toHaveBeenCalledTimes(1));
    unmount();
    expect(instances[0].destroy).toHaveBeenCalledTimes(1);
  });

  it("gives a labelled host a role and name, and a decorative one neither", async () => {
    const labelled = render(OpenBotAvatar, {
      props: { name: "Grace", definitionId: "cat" },
    });
    await waitFor(() => expect(mountAvatar).toHaveBeenCalled());
    const host = labelled.container.querySelector(".openbot-avatar__host")!;
    expect(host.getAttribute("role")).toBe("img");
    expect(host.getAttribute("aria-label")).toBe("Grace");
    expect(labelled.container.querySelector(".openbot-avatar")!.getAttribute("aria-hidden")).toBeNull();

    cleanup();
    mountAvatar.mockClear();
    const decorative = render(OpenBotAvatar, {
      props: { name: "Grace", definitionId: "cat", decorative: true },
    });
    await waitFor(() => expect(mountAvatar).toHaveBeenCalled());
    const decorativeHost = decorative.container.querySelector(".openbot-avatar__host")!;
    expect(decorativeHost.getAttribute("role")).toBeNull();
    expect(decorativeHost.getAttribute("aria-hidden")).toBe("true");
    expect(decorative.container.querySelector(".openbot-avatar")!.getAttribute("aria-hidden")).toBe("true");
    expect(mountAvatar.mock.calls.at(-1)![1].label).toBe("");
  });

  it("reports the six states on the root, so a DOM test can read the live state", async () => {
    for (const state of OPENBOT_STATES) {
      const { container, unmount } = render(OpenBotAvatar, {
        props: { name: "Ada", definitionId: "cat", state },
      });
      await tick();
      expect(container.querySelector(".openbot-avatar")!.getAttribute("data-openbot-state")).toBe(state);
      unmount();
    }
  });
});

describe("OpenBotAvatar — fallbacks", () => {
  it("shows a still SVG when the environment has no canvas", async () => {
    vi.unstubAllGlobals();
    vi.stubGlobal("ResizeObserver", FakeResizeObserver);
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);

    const { container } = render(OpenBotAvatar, {
      props: { name: "Ada", definitionId: "cat", colour: "#f8a01c", mood: "waiting" },
    });
    await tick();

    expect(mountAvatar).not.toHaveBeenCalled();
    const img = container.querySelector("img")!;
    expect(img.getAttribute("src")!.startsWith("data:image/svg+xml")).toBe(true);
    expect(img.getAttribute("alt")).toBe("Ada");
    expect(container.querySelector(".openbot-avatar")!.getAttribute("data-openbot-state")).toBe("waiting");
  });

  it("falls back to a still when the live mount throws", async () => {
    const warning = vi.spyOn(console, "warn").mockImplementation(() => {});
    mountAvatar.mockImplementation(() => {
      throw new Error("no canvas");
    });

    const { container } = render(OpenBotAvatar, {
      props: { name: "Ada", definitionId: "ghost", mood: "failed" },
    });
    await waitFor(() => expect(mountAvatar).toHaveBeenCalled());

    expect(container.querySelector(".openbot-avatar__host")).toBeNull();
    const img = container.querySelector("img")!;
    expect(img.getAttribute("src")!.startsWith("data:image/svg+xml")).toBe(true);
    expect(img.getAttribute("alt")).toBe("Ada");
    expect(container.querySelector(".openbot-avatar")!.getAttribute("data-openbot-state")).toBe("blocked");
    expect(warning).toHaveBeenCalled();
  });

  it("keeps a decorative fallback out of the accessibility tree", async () => {
    vi.unstubAllGlobals();
    vi.stubGlobal("ResizeObserver", FakeResizeObserver);
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);

    const { container } = render(OpenBotAvatar, {
      props: { name: "Ada", definitionId: "cat", decorative: true },
    });
    await tick();
    const img = container.querySelector("img")!;
    expect(img.getAttribute("alt")).toBe("");
    expect(img.getAttribute("aria-hidden")).toBe("true");
  });
});

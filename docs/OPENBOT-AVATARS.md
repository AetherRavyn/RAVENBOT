# OpenBot avatars

RAVENBOT has three avatar families, and they coexist:

- **Raven native** — the generated SVG face, with 16 expressions, 15 animation
  states and 12 colours.
- **DiceBear** — the remote style library. The style owns its own animation.
- **OpenBot** — 23 bundled object characters with eyes, drawn locally by the
  `open-bot-avatar` package.

An agent can switch between them without a migration, because all three use the
same two columns (`avatar_url`, `avatar_style`).

## Source and licence

The definitions and the renderer are MIT licensed, from
[DominikScholz/open-bot-avatar](https://github.com/DominikScholz/open-bot-avatar).
They are bundled at build time:

- the 16 community definitions in upstream `gallery/*.json` (excluding
  `colors.json`);
- the package's seven examples: controller, barbell, mug, lightbulb, chat,
  sprout and watering-can.

`src/lib/openBotGalleryData.ts` holds the definitions verbatim.
`src/lib/openBotAvatar.ts` validates each with the package's `validateAvatar`
and drops invalid entries (the test fails if anything was dropped). There is no
network fetch, no user-provided definition path, and no uploaded image flow.
The upstream editor, generator and import tools are **not** integrated; only the
bundled gallery and runtime are.

## URL scheme and storage

```
avatar_style = openbot:cat
avatar_url   = openbot://cat?color=%23f8a01c
```

- `openbot://<definition-id>?color=<url-encoded-hex>` is the image URL.
- `openbot:<definition-id>` is the style. Either one is enough to render an
  OpenBot character, which is why a surface that only stores `avatar_url` still
  works.
- A missing or invalid `color` is treated as absent; the colour then falls back
  from the definition's URL, to a stable colour derived from the agent's name.
  Unknown query parameters are ignored.
- Accepted colours are `#rgb` and `#rrggbb`, with or without the leading `#`.
  Anything else falls back to `#ff6a1a`, the package default.
- Definition ids and colours are normalised by `openBotUrl`,
  `parseOpenBotUrl`, `openBotStyleId` and `parseOpenBotStyle`.

Existing agents keep their records. A DiceBear URL, a custom image, or
`raven-native` is untouched; only values that start with `openbot://` or
`openbot:` take the OpenBot path.

## States

RAVENBOT has seven roster moods; the OpenBot renderer has six work states. The
mapping is exhaustive and tested:

| RAVENBOT `AvatarMood` | OpenBot state | Meaning |
|---|---|---|
| `idle` | `idle` | Nothing needs you |
| `sleeping` | `idle` | No activity; OpenBot has no separate sleep pose |
| `thinking` | `thinking` | A long run; leave it alone |
| `working` | `working` | Work is happening now |
| `waiting` | `waiting` | It needs a decision from you |
| `failed` | `blocked` | It stopped and could not continue |
| `responded` | `done` | It finished and nobody has read it |

The live state comes from `fleetActivity.mood(bot.id)` wherever fleet activity
is available, not from a hardcoded preview. The studio's selected state is a
preview only; Apply stores the definition and colour, never the preview state.

`RavenAvatar` exposes `data-openbot-state` with the mapped live state, so a DOM
test (or a stylesheet) can distinguish working from waiting from blocked from
done. It also reports the definition id in `data-shape`, the hex in
`data-colour`, and `data-source="openbot"`.

## Surfaces

- `RavenAvatar.svelte` detects OpenBot before the DiceBear/custom branches and
  renders `OpenBotAvatar` with the real mood. The outer span keeps the
  accessible name; the inner host is decorative.
- `Sidebar`, `OfficeHeader`, `OfficeBoard`, `ChatMessageRow`, `ChatRoomList`,
  `ThreadHeader`, `ThreadView`, `ChatRoomView`, `ConnectorCenter`,
  `CreateChatRoom`, `OfficeSettings`, `WorkspaceSidebar`, `SkillManager`,
  `UsagePanel`, `PlanDag` and `BotSettings` pass the stored URL/style where
  they have it. `ChatMessageRow` has the URL but not the style; the URL is
  enough by design.
- `AvatarPicker` adds one OpenBot tile with a still preview (not 24 canvases).
  Picking it opens `OpenBotStudio`; the native `AvatarStudio` stays for
  `raven-native`.
- `OpenBotStudio` shows the full gallery, a body-colour picker (curated
  swatches plus a native colour input), neutral/happy, volume 0–14,
  follow-pointer, flat/fluffy/glass, an animation toggle, and the six-state
  strip in the reference order: Idle, Thinking, Working, Waiting, Blocked,
  Done.

## Rendering and materials

`OpenBotAvatar.svelte` mounts the package's `mountAvatar` into a square host and
updates only the options that changed. It supports:

- `renderer: "canvas"` (default) and `"scene"` (CSS 3D);
- `material: "flat" | "fluffy" | "glass"` by dynamically importing
  `open-bot-avatar/materials/fluffy` or `glass`. A material is passed as the
  imported value, not its name; the package deliberately bundles none of them.
- Fluffy and glass need a canvas and WebGL2. Without WebGL2 the package draws
  flat; this integration does not fake a finish with CSS.
- If a material import fails, the component logs one console warning and keeps
  the flat body. There is no user-facing error and no blank avatar.

## Reduced motion and fallbacks

- `reducedMotion: "system"` is passed to the package, so the OS
  prefers-reduced-motion setting wins. The studio's animation toggle affects
  previews only; it is not stored on the agent.
- In SSR and in environments without a usable canvas/layout engine (jsdom,
  older WebKit), the component renders `openBotStillUrl(...)` as an
  `data:image/svg+xml;charset=utf-8,...` image with the same character, colour
  and state. No canvas API is required, and nothing throws into a roster.
- A host that claims support but fails to mount also falls back to the still.
- Stills are percent-encoded, never injected as raw HTML. Gallery tiles and the
  six-state strip therefore need no canvas and no `{@html}`.

## Tests

- `src/lib/openBotAvatar.test.ts` — gallery validation and unique ids, URL and
  style round-trips, colour fallbacks, stable name defaults, the full mood
  mapping, and still-data-URL safety.
- `src/lib/components/OpenBotAvatar.dom.test.ts` — mount/update/destroy,
  decorative vs labelled semantics, the six states, and both fallback paths
  (mocked `open-bot-avatar/dom`).
- `src/lib/components/RavenAvatar.dom.test.ts` — OpenBot URL and style
  rendering, `data-openbot-state`, and the unchanged DiceBear/native/custom
  paths.
- `src/lib/components/OpenBotStudio.dom.test.ts` — gallery, six native state
  buttons, and the applied URL/style.
- `src/lib/components/AvatarPicker.openbot.dom.test.ts` — the picker's OpenBot
  tile and commit shape.

## Limits

- Only the bundled 23 definitions are available. There is no import, upload,
  editor, or generated-definition flow in the app.
- The renderer receives a definition object and a hex colour; no remote assets,
  scripts, or CSS colours from user input reach it.
- Fluffy and glass degrade to flat without WebGL2. The CSS 3D scene renderer is
  available to callers, but the picker/studio do not persist it; they use the
  package default.
- The selected studio state is not stored. A live agent's pose is always
  derived from its fleet mood, so an agent cannot be left frozen in "blocked".

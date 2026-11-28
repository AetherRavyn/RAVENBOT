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
network fetch and no user-provided definition path. The upstream editor,
generator and definition import tools are **not** integrated; only the bundled
gallery and runtime are. A person can upload a **photo** as their own custom
avatar (see below); that is an image identity, never an OpenBot definition.

## Custom photos

The picker accepts a local PNG, JPEG, WebP or GIF up to **512 KB**:

- the file is validated by magic bytes, not only `File.type`, so a renamed SVG
  or text file is refused;
- the bytes become a `data:` URL and are stored in the existing `avatar_url`
  column with `avatar_style = "custom"`;
- the preview updates immediately, and **Use Selected Avatar** commits it;
- **Remove image** clears it and returns to a generated/DiceBear choice;
- a corrupt or undecodable image falls back to the generated face through the
  existing `RavenAvatar` broken-image path;
- no network request is made for the photo.

The 512 KB cap is deliberate: the data URL travels in the bot-list payload.
Moving larger photos to `~/RAVENBOT/cache/avatars/` with an `asset:` URL is a
future change only if a real fleet shows payload pressure.

## URL scheme and storage

```
avatar_style = openbot:cat
avatar_url   = openbot://cat?color=%23f8a01c&depth=9&follow=1&finish=fluffy
```

- `openbot://<definition-id>?color=<url-encoded-hex>` is the image URL. Render
  choices are appended after the colour:
  - `depth=0`…`14` — the package volume (default 6).
  - `follow=1` or `follow=0` — pointer-follow (default off).
  - `finish=flat|fluffy|glass` — body finish (default flat).
  - `renderer=scene` — the CSS 3D renderer. Canvas is the default and is
    represented by omitting the parameter. The studio does not expose this
    choice and never writes it; the URL and component accept it for callers
    that opt in.
- `openbot:<definition-id>` is the style. Either the style or the URL is enough
  to render an OpenBot character, which is why a surface that only stores
  `avatar_url` still works.
- `color` is always first. The other parameters appear only when explicitly
  chosen.
- A missing or invalid `color` is treated as absent; the colour then falls back
  from the definition's URL, to a stable colour derived from the agent's name.
  Unknown or invalid query parameters are ignored: an out-of-range `depth`, a
  non-`1`/`0` `follow`, an unknown `finish`, or a non-`scene` `renderer` is
  never passed to the renderer.
- Accepted colours are `#rgb` and `#rrggbb`, with or without the leading `#`.
  Anything else falls back to `#ff6a1a`, the package default.
- Definition ids and colours are normalised by `openBotUrl`,
  `parseOpenBotUrl`, `openBotStyleId` and `parseOpenBotStyle`. An older
  colour-only URL (`openbot://cat?color=%23f8a01c`) still parses and uses the
  defaults.

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

The live state comes from `fleetActivity.mood(bot.id)` on the surfaces that
share fleet activity — the sidebar, office header, office board, chat rows,
thread header and the pre-first-token row — not from a hardcoded preview. Plan
nodes instead use their own run state (`running`, `done`, `failed`), so a
finished node beside a failed one shows both truths. The studio's selected state
is a preview only; Apply stores the definition and colour, never the preview
state.

### Tool activity

When the mood is `working`, `ThreadView` also passes the current tool activity.
`fleetActivity` derives it from `tool_started.name`, holds it for 1.4 seconds so
rapid calls do not flicker, and clears it on done/error. `openBotActivityForTool`
maps tool families onto seven names: `searching`, `browsing`, `reading`,
`running`, `editing`, `delegating` and `reviewing`.

The renderer has six poses, not seven activities, so the nearest pose is used:

| Activity | OpenBot state |
|---|---|
| `searching`, `browsing`, `reading` | `thinking` |
| `editing`, `running`, `delegating` | `working` |
| `reviewing` | `waiting` |

This is a deliberate limitation, not a claim that the renderer draws seven
distinct faces. The activity is still carried separately from the urgency mood,
so a future renderer or label can distinguish them. Activity is ignored unless
the mood is `working`; a waiting or blocked agent keeps its own face. With
reduced motion the pose is still applied and the character stops animating.

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
  Done. **Surprise me** picks a different character and its curated colour
  without touching those render choices. Apply stores the character, colour,
  volume, pointer-follow and finish in `avatar_url`; reopening the studio reads
  them back. The neutral/happy toggle and the animation checkbox affect the
  preview only — the live tone follows the run state, and the picker owns the
  global animation preference.

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
  style round-trips (including depth/follow/finish/renderer and invalid-value
  rejection), colour fallbacks, stable name defaults, the full mood mapping,
  the seven activity names and nearest-pose table, the deterministic surprise
  shuffle, and still-data-URL safety.
- `src/lib/components/OpenBotAvatar.dom.test.ts` — mount/update/destroy,
  decorative vs labelled semantics, the six states, and both fallback paths
  (mocked `open-bot-avatar/dom`).
- `src/lib/components/RavenAvatar.dom.test.ts` — OpenBot URL and style
  rendering, `data-openbot-state`, activity only while working, and the
  unchanged DiceBear/native/custom paths.
- `src/lib/fleetActivity.test.ts` — tool activity derivation, the 1.4-second
  hold, and clearing on done/error.
- `src/lib/components/OpenBotStudio.dom.test.ts` — gallery, six native state
  buttons, the two-click Surprise me flow, preserved render choices, and the
  applied URL/style.
- `src/lib/components/AvatarPicker.openbot.dom.test.ts` — the picker's OpenBot
  tile and commit shape.
- `src/lib/components/AvatarPicker.upload.dom.test.ts` — valid upload, forged
  bytes, over-512 KB, remove and generated fallback.

## Limits

- Only the bundled 23 definitions are available. There is no OpenBot definition
  import, generator or editor flow. Local photos are supported through the
  separate custom-image path above.
- A custom photo is limited to 512 KB and PNG/JPEG/WebP/GIF; the renderer gets
  the resulting `data:` URL, never markup or a script.
- The renderer receives a definition object and a hex colour; no remote assets,
  scripts, or CSS colours from user input reach it.
- Fluffy and glass degrade to flat without WebGL2. The CSS 3D scene renderer is
  supported by the URL and component (`renderer=scene`), but the studio keeps
  it explicitly unexposed and writes the canvas default (by omitting the
  parameter).
- The selected studio state is not stored. A live agent's pose is always
  derived from its fleet mood, so an agent cannot be left frozen in "blocked".

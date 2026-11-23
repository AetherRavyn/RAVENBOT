# Motion graphics with fframes

Use this workflow for a short video: a title card, an explainer, a promo, a
social clip, or a rendered animation. It is not for editing long footage.

## 1. Pin the brief

Write down the audience, the one sentence the video must communicate, the
duration, the aspect ratio, and where it will be watched. A six-second vertical
clip and a sixty-second landscape explainer are different designs.

## 2. Install or confirm fframes

```bash
cargo fframes --version
```

If it is missing:

```bash
cargo install cargo-fframes
```

## 3. Scaffold a project

Use the current CLI and a template that fits the brief:

```bash
cargo fframes new my-clip --template multi-scene --backend skia
cd my-clip
```

`--template multi-scene` gives you more than one scene; a single title card can
use the simpler template. Pick the backend you have installed; if you are unsure,
start with the default and read the generated README.

## 4. Design the scenes

- One idea per scene. A scene that needs a paragraph is a slide, not motion.
- Keep text on screen long enough to read once. If it is longer than a short
  sentence, cut it.
- Motion should explain a change: a value counting up, a bar growing, a route
  drawing, a before/after wipe. Decorative movement is noise.
- Use the brand palette from the brief; do not invent a second accent.
- Put real data in the video. A number on screen is a claim; make it traceable.
- Respect safe margins for vertical platforms (top and bottom UI overlays).

## 5. Render and inspect

Preview a frame before paying for a full render:

```bash
cargo run --release -- inspect
cargo run --release -- frame --at 1.5s
```

Review the frame or contact sheet, then render:

```bash
cargo run --release
```

The output is a video in the project's `output/` directory. Render at the
brief's resolution and frame rate; do not upscale a low-resolution draft.

## 6. Verify the file

```bash
ffprobe -v error -show_entries format=duration,size:stream=codec_name,width,height,r_frame_rate -of default=noprint_wrappers=1 output.mp4
```

Check duration, codec, dimensions, frame rate, and audio presence. If the brief
specifies loudness, measure it:

```bash
ffmpeg -i output.mp4 -af loudnorm=I=-16:TP=-1.5:LRA=11 -f null -
```

For a quick review pass, `frame` and `onion` are cheaper than re-rendering:
`onion` shows adjacent frames, and `inspect` reports the project structure.

## 7. ffmpeg fallback

If `cargo-fframes` cannot be installed, ship a designed slideshow with ffmpeg
instead of claiming fframes output. A tested fallback is:

1. Render each scene as a 1920×1080 PNG (or the brief's size).
2. Build a concat file with a duration per image.
3. Add captions and a silent or music track.
4. Encode H.264/AAC with `-pix_fmt yuv420p` for compatibility.
5. Apply `loudnorm` if audio is present.

Say clearly that this is an ffmpeg slideshow, not real motion graphics. It can
still be a good deliverable; it is just a different one.

## 8. Report honestly

Name the output path, the duration, the dimensions, the codec, and the commands
used to verify it. If the render failed, report the failing command and its
stderr. Never claim a video exists without `ffprobe` confirming it.

## 9. Avoid

- Text that appears and disappears before it can be read.
- A different font or colour on every scene.
- Fake UI screenshots or fabricated metrics.
- Rendering a 4K master when the brief asks for a social clip.
- Calling an ffmpeg slideshow “fframes output”.
- Uploading or publishing the video without the user asking.

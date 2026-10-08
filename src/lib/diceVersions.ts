/**
 * The DiceBear API major version this app requests.
 *
 * 10.x, because that is where the animated styles live. 9.x has no
 * `animationVariant` option at all, so a 9.x URL for `voxel-bot` returns a still
 * robot and there is no way to ask for the blink — the feature is simply absent
 * from the older major. Every style the app already offered still resolves on
 * 10.x, which is what makes the jump safe.
 *
 * Kept in its own module because the URL builder and the URL upgrader both need
 * it, and a second hard-coded copy is a second thing to forget when it changes.
 */
export const DICEBEAR_VERSION = "10.x";

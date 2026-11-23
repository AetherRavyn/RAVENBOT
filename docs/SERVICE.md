# Background daemon

RAVENBOT can run as a per-user **headless daemon**: no window, no webview, no
display. It owns the database, runtime, scheduler, webhook receiver, office
loops, audit maintenance and the optional remote listener. Settings → Local →
Background service installs it.

## What it runs

- Cron routines (the same executor as the desktop app).
- Office queue and autonomous-office heartbeats.
- Routine **and workflow** webhook triggers; scheduled workflows also run.
- Audit export/retention.
- Provider credential refresh (a keychain that unlocks after login is picked up
  without restarting the daemon).
- Attention push: OS notification when a session exists, plus the configured
  HTTPS webhook and SMTP alert.
- The selected account's data root. Per-account service units are generated with
  their own systemd/launchd/task name, port block and keychain namespace. See
  `docs/ACCOUNTS.md`.
- Native Telegram long polling when the connector is enabled.
- Native Slack Socket Mode when the connector is enabled.
- Native Discord Gateway WebSocket when the connector is enabled.
- A loopback chat-webhook listener on `127.0.0.1:8801` for signed WhatsApp and
  Microsoft Teams deliveries. Expose `/whatsapp` or `/teams` through a trusted
  HTTPS reverse proxy; the listener itself never binds a public interface by
  default.
- Remote access, if enabled.

A routine's high-risk tool still parks under the per-run **Review** policy. In
daemon mode there is no approval card in a window; the attention loop notifies
the configured channels, the remote page shows the card, and launching the
desktop app takes over. Pair a device before logout if you want remote control.

## One fleet owner

The daemon and the desktop app both own the same SQLite database and timers, so
only one may run. They share `~/RAVENBOT/fleet.lock`:

- The first process takes an exclusive OS file lock.
- A second daemon logs the current owner and exits cleanly.
- A desktop launch that finds the daemon holding the lock asks whether to take
  over. **Take over** stops the user service, waits for the lock, and continues.
  **Keep service** exits the desktop launch.
- A crash releases the lock automatically; a stale file is harmless.

The single-instance plugin still handles a second desktop launch.

## Files

| Platform | Unit |
|---|---|
| Linux (systemd) | `~/.config/systemd/user/ravenbot.service` |
| macOS (launchd) | `~/Library/LaunchAgents/dev.ravenbot.background.plist` |
| Windows (Task Scheduler) | `~/RAVENBOT/config/ravenbot-task.xml` |

The unit/task runs `<exe> daemon`:

- Linux: `Restart=on-failure`, `RestartSec=5`, `WantedBy=default.target`.
- macOS: `RunAtLoad`, `KeepAlive` with `SuccessfulExit=false`.
- Windows: per-user logon task, `LeastPrivilege`, `IgnoreNew`, hidden, with
  `RestartOnFailure`. This is a **logon task**, not a Windows service; it needs
  no administrator rights and stops with the user session.

A clean stop/SIGTERM exits `0` and stays stopped; a crash is restarted.

## Linux

The generated unit enables `NoNewPrivileges`, `PrivateTmp`, `ProtectSystem=full`,
protected kernel interfaces, `RestrictRealtime`, `LockPersonality`, and
`LimitCORE=0`. The daemon can still write the RAVENBOT root and workspace
folders under `$HOME`. System-level package installation and other `/etc` or
`/usr` writes are intentionally not available to the service; run those outside
the daemon.

The unit deliberately does not add a systemd `SystemCallFilter` or
`IPAddressDeny` allowlist. The daemon executes arbitrary agent tools, and a
systemd syscall allowlist would either break those tools or duplicate the
per-command bubblewrap/Docker boundary. `systemd-analyze security` therefore
still reports exposure for those controls; the real command boundary is the
sandbox runner, not the unit.: after logout

The daemon has no display dependency. With lingering enabled, the systemd user
manager and the daemon keep running after logout:

```sh
loginctl enable-linger "$USER"
```

Settings → Background service exposes the state and a toggle. Install attempts
to enable lingering. This is the supported path for routines and remote access
while nobody is logged in.

## macOS

The LaunchAgent runs headless at login and stops when the user session ends.
A LaunchDaemon running as the login user after logout is not implemented;
document and use the Linux path for true after-logout operation. A private
tunnel plus a login session still gives remote access while logged in.

## Install and handoff

1. Settings → Local → Background service.
2. **Install service** writes and enables the unit and disables the plain login
   autostart.
3. **Restart in background** exits this process and starts the daemon. The
   single-instance plugin is not involved: the fleet lock is released when the
   desktop process exits, and the daemon acquires it.
4. Launching the desktop app later offers to take over and stop the service.

The service CLI is available for headless installs and diagnosis:

```sh
ravenbot service status
ravenbot service unit > /tmp/ravenbot.service
systemd-analyze --user verify /tmp/ravenbot.service
ravenbot service install
ravenbot service start
ravenbot service handoff
ravenbot service stop
ravenbot service uninstall
```

The Settings path also disables the desktop autostart entry; the CLI does not
touch it.

## Headless administration

When the daemon owns the fleet there is no desktop UI. Pair and inspect devices
from a terminal:

```sh
ravenbot remote status
ravenbot remote pair --role operator
ravenbot remote devices
ravenbot remote revoke <device-id>
```

`remote status` reports whether the listener is actually accepting connections,
the URL, devices and recent mutation audit. `remote pair` requires remote access
to be enabled and prints a one-time code. Pair a phone **before** logging out if
you depend on after-logout remote control.

## Uninstall

Settings → Background service → **Uninstall**, or `ravenbot service uninstall`.

- Linux: disables the unit, removes the file, stops the still-loaded unit, then
  reloads systemd. The stop is after the file removal so a service-owned
  process cannot leave a stale unit behind.
- macOS: removes the plist, then boots the job out.

Install and uninstall are idempotent; the writer only rewrites a changed file.

## Manual acceptance checks

Headless startup without a display:

```sh
env -u DISPLAY -u WAYLAND_DISPLAY ravenbot daemon
```

It must reach `RAVENBOT headless daemon running; no window`, create
`~/RAVENBOT/ravenbot.db`, and exit cleanly on Ctrl-C/SIGTERM.

Linux service:

```sh
systemctl --user status ravenbot.service
systemctl --user is-enabled ravenbot.service
journalctl --user -u ravenbot.service -n 50
curl -s http://127.0.0.1:8790/api/health   # if remote access is enabled
```

macOS:

```sh
launchctl print gui/$(id -u)/dev.ravenbot.background
tail -n 50 ~/RAVENBOT/logs/service.log
tail -n 50 ~/RAVENBOT/logs/service.error.log
```

Then:

1. Install and hand off; confirm the process is running with no window.
2. Close the desktop window and confirm routines still fire.
3. `kill -9` the daemon and confirm systemd/launchd restarts it.
4. On Linux with lingering, log out and confirm the daemon and remote API stay
   up.
5. Launch the desktop app and confirm the takeover prompt stops the service.
6. Quit from the tray and confirm it does **not** restart.
7. The `ravenbot service unit` output must pass
   `systemd-analyze --user verify` from a file.

## Limits

- Windows uses a per-user Task Scheduler logon task, not a Windows service: it
  needs no administrator rights and stops with the user session.
- macOS after-logout operation needs a LaunchDaemon; not implemented.
- Interactive browser-dock tools, screenshots and computer control need a
  display and the desktop shell. HTTP fetch/search, files, shell, scheduling,
  offices, workflows and MCP connectors work headless.
- Desktop notifications require a live user session. After logout the HTTPS
  webhook and SMTP alert remain, and the remote page is the inbox.
- The daemon is a per-user process, not a root system daemon.
- In an AppImage, the unit uses `$APPIMAGE`, not the temporary mount path.
- Cloud provider keys stored in an OS keychain may be unavailable to a cold
  daemon start while the keychain is locked after logout. The daemon refreshes
  provider credentials every two minutes, so an unlock after login is picked up
  without restarting. Pair devices and verify a cloud run before relying on
  after-logout autonomy.
- Multi-user desktop profiles are not implemented; the daemon is per OS user.

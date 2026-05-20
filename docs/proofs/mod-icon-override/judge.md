Verdict: sufficient

Technical debt: clear

Acceptance criteria: met

The change keeps the engine icon untouched, stores the Rundale artwork under
the Rundale mod, validates mod icon paths so they cannot escape the mod asset
directory, and wires the active mod icon through both browser favicon updates
and native Tauri desktop icon application. The macOS path explicitly updates
NSApplication's application icon, which is the Dock surface; the existing window
icon path remains in place for window-level icon support. The runtime also
reapplies the icon on Tauri `RunEvent::Ready`, after Tauri dev mode's own
bundle-icon update, so `just run` and packaged launches use the same mod-owned
Dock icon. The mod icon PNGs now carry alpha with transparent rounded corners,
and the included 64px proof artifact confirms the generated artwork remains
legible at small icon sizes while preserving the transparent corner mask.

Criterion review:

- Mod ownership and engine-icon separation are met: all new app icon assets live
  under `mods/rundale/assets/icons/app/`, with no change to the Parish engine
  icon.
- Branding declaration and validation are met: `ui.toml` declares app-icon and
  favicon paths, and the core tests cover both in-assets resolution and
  escaping-path rejection.
- Web/browser behavior is met: the frontend test exercises active-mod favicon
  and apple-touch link updates.
- Desktop behavior is met: the Tauri code applies the icon in setup and again
  after `RunEvent::Ready`, covering both packaged launch and `just run` dev
  launch behavior.
- Transparent-corner behavior is met: the icon PNGs are RGBA, `sips` reports an
  alpha channel, and the alpha spot-check confirms transparent corners with an
  opaque center.
- Regression coverage is met: focused tests, compile checks, live dev launch,
  proof gate, whitespace check, and the full `just check` gate were recorded.

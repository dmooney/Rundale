# Parish Visual

`parish/apps/visual` is the graphics-first Parish client. It is separate from
the Svelte/Tauri HUD and talks to a running Parish server through the existing
HTTP scene-state contract.

## Commands

```sh
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual run test
npm --prefix parish/apps/visual run build
npm --prefix parish/apps/visual run dev
```

The dev server listens on `VISUAL_CLIENT_PORT` or `4174` by default. Requests
under `/api/*` are proxied to the local Parish backend at
`http://127.0.0.1:3030`.

## Scope

This milestone uses plain browser Canvas 2D and zero runtime dependencies. It
draws placeholder geometry from `/api/scene-state`; it is not the final sprite
or animation engine. The small command form posts to `/api/command` so the
visual client owns its browser session and can move to an authored scene.
Canvas hotspots are interactive: travel hotspots submit movement commands,
while inspect hotspots show their authored inspection text in the command log.
NPC sprites render from scene-state `sprite_url` values; clicking a sprite
prepares a `talk to ...` command without submitting dialogue automatically.
The Recent panel keeps a bounded local transcript of commands, world responses,
inspections, and sprite selections so the app can be played without losing
context after each action. The Hotspots and People side panels expose the same
actions as buttons, so the graphics client can be played even when a precise
Canvas click is awkward. On desktop, the stage stays fixed to the viewport while
the inspector scrolls independently; on narrow screens the page stacks into a
normal scrolling document. A compact status line reports loading, ready, empty,
sending, and connection-error states, and controls are disabled while network
work is in flight.

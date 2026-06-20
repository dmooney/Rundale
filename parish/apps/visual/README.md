# Parish Visual

`parish/apps/visual` is the graphics-first Parish client. It is separate from
the Svelte/Tauri HUD and talks to a running Parish server through the existing
HTTP scene-state contract.

## Commands

```sh
npm --prefix parish/apps/visual run check
npm --prefix parish/apps/visual run test
npm --prefix parish/apps/visual run build
PARISH_BACKEND_URL=http://127.0.0.1:3030 npm --prefix parish/apps/visual run dev
```

The dev server listens on `VISUAL_CLIENT_PORT` or `4174` by default. Requests
under `/api/*` are proxied to `PARISH_BACKEND_URL`, which defaults to
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

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

This milestone uses PixiJS as the game renderer. The first viewport is a
full-screen scene, not an inspector: ordered compositor layers from
`/api/scene-state` are drawn into the world, NPC sprites are placed in authored
slots, and hotspots are clicked directly on the scene. Travel hotspots submit
backend-authored commands from the scene contract, inspect hotspots write a
short caption/log entry, and NPC sprite clicks prepare a `talk to ...` command
without submitting dialogue automatically. The compact bottom HUD keeps recent
commands and world responses visible without dominating the scene. Server
settings and refresh controls live behind the small Settings drawer.

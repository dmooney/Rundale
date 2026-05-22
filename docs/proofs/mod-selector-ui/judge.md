Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

[Mod list endpoint returns active flag]: transcript.txt GET /api/mods shows `"active": true` on testbed, `"active": false` on rundale. Exactly one active entry. ✓

[Switch endpoint updates mod-list.toml]: transcript.txt POST /api/mods/switch → `{"ok":true}` and `mods/mod-list.toml` content `active_setting = "rundale"`. Invalid id → `{"ok":false,"error":"unknown mod id"}`. ✓

[Overlay opens from the UI]: `ModSelectorOverlay` imported in `+page.svelte`, conditionally rendered when `modSelectorVisible` store is true. "Mod" button in `StatusBar` triggers it. Code reviewed in evidence.md. ✓

[Active mod is visually indicated]: `mod-card--active` CSS class applied to active mod; selected state pre-initialized to the active mod's id; "active" badge rendered. Code reviewed in evidence.md. ✓

[Confirm triggers switch and reload]: `confirm()` in `ModSelectorOverlay` calls `switchMod()` → POST to backend → on success shows restart notice with "Reload now" button calling `window.location.reload()`. Backend write verified in transcript. ✓

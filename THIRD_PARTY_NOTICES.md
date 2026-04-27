# Third-Party Notices

Rundale on the Parish engine includes, uses, or redistributes the third-party
software and data components listed below. Each component is used under its
own licence, and the full licence text for each distinct licence is reproduced
at the bottom of this file.

This file is **maintained manually** for direct dependencies. To regenerate
an exhaustive list including all transitive dependencies, run:

```sh
just notices
```

That recipe invokes [`cargo-about`](https://github.com/EmbarkStudios/cargo-about)
for the Rust workspace and
[`license-checker-rseidelsohn`](https://www.npmjs.com/package/license-checker-rseidelsohn)
for the frontend. The generated output is written alongside this file as
`THIRD_PARTY_NOTICES.rust.md` and `THIRD_PARTY_NOTICES.ui.md`.

---

## Map data and tiles

### OpenStreetMap (base tiles + Nominatim geocoding)

- Source: <https://www.openstreetmap.org/>, <https://nominatim.openstreetmap.org/>
- Licence: **Open Database License (ODbL) 1.0** for the database,
  **Database Contents License (DbCL) 1.0** for individual contents
- Attribution: "© OpenStreetMap contributors"
- Terms: <https://www.openstreetmap.org/copyright>

### National Library of Scotland — Historic 6″ OS Ireland tiles

- Source: <https://maps.nls.uk/> (tileset served from
  `mapseries-tilesets.s3.amazonaws.com/os/roscommon1/…`)
- Content: Ordnance Survey of Ireland First Edition 6-inch maps, surveyed
  1829–1842, scanned and hosted by the National Library of Scotland
- Licence: **Creative Commons Attribution-ShareAlike 3.0 (CC-BY-SA 3.0)**
- Attribution: "Historic 6″ OS Ireland (1829–1842) — National Library of Scotland"
- Terms: <https://maps.nls.uk/copyright.html>

## Fonts

### Noto Sans Symbols 2

- Source: <https://github.com/notofonts/symbols2>
- Copyright: 2022 The Noto Project Authors
- Licence: **SIL Open Font License 1.1 (OFL-1.1)**
- Bundled: `assets/fonts/NotoSansSymbols2-Regular.ttf`
- Full licence text: `assets/fonts/NotoSansSymbols2-LICENSE.txt`

---

## Frontend runtime dependencies (`apps/ui/package.json`)

| Package | Version (minimum) | Licence | Copyright |
|---|---|---|---|
| [maplibre-gl](https://github.com/maplibre/maplibre-gl-js) | ^5.22 | **BSD-3-Clause** (plus MPL-2.0 components inherited from the Mapbox fork point) | © 2020 MapLibre contributors; © 2014–2020 Mapbox, Inc. |
| [phosphor-svelte](https://github.com/haruaki07/phosphor-svelte) | ^3.1 | **MIT** | © 2020 Phosphor Icons; © Haruaki Tanaka |
| [@tauri-apps/api](https://github.com/tauri-apps/tauri) | ^2.10 | **MIT OR Apache-2.0** | © Tauri Programme within The Commons Conservancy |

### Frontend build/test dependencies

All of the following are build-only and are not shipped to end users in the
production bundle. They are listed here for completeness; their licences are
permissive (MIT / Apache-2.0 / BSD / ISC / 0BSD).

`@playwright/test`, `@sveltejs/adapter-auto`, `@sveltejs/adapter-static`,
`@sveltejs/kit`, `@sveltejs/vite-plugin-svelte`, `@testing-library/jest-dom`,
`@testing-library/svelte`, `jsdom`, `svelte`, `svelte-check`, `typescript`,
`vite`, `vitest`.

Full transitive list: run `just notices` → `THIRD_PARTY_NOTICES.ui.md`.

---

## Rust workspace runtime dependencies

Direct dependencies declared across `crates/*/Cargo.toml`. All are licensed
under one or more of MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC,
MPL-2.0, Unicode-DFS-2016, or Zlib. Full transitive list with exact
versions, copyright holders, and licence texts: run `just notices` →
`THIRD_PARTY_NOTICES.rust.md`.

### Core runtime

| Crate | Licence | Upstream |
|---|---|---|
| anyhow | MIT OR Apache-2.0 | <https://github.com/dtolnay/anyhow> |
| axum, axum-core | MIT | <https://github.com/tokio-rs/axum> |
| chrono | MIT OR Apache-2.0 | <https://github.com/chronotope/chrono> |
| clap, clap_derive | MIT OR Apache-2.0 | <https://github.com/clap-rs/clap> |
| dashmap | MIT | <https://github.com/xacrimon/dashmap> |
| dotenvy | MIT | <https://github.com/allan2/dotenvy> |
| governor | MIT | <https://github.com/boinkor-net/governor> |
| jsonwebtoken | MIT | <https://github.com/Keats/jsonwebtoken> |
| once_cell | MIT OR Apache-2.0 | <https://github.com/matklad/once_cell> |
| png | MIT OR Apache-2.0 | <https://github.com/image-rs/image-png> |
| rand, rand_chacha | MIT OR Apache-2.0 | <https://github.com/rust-random/rand> |
| regex | MIT OR Apache-2.0 | <https://github.com/rust-lang/regex> |
| reqwest | MIT OR Apache-2.0 | <https://github.com/seanmonstar/reqwest> |
| rusqlite (with bundled SQLite) | MIT; SQLite itself is public domain | <https://github.com/rusqlite/rusqlite> |
| serde, serde_json | MIT OR Apache-2.0 | <https://github.com/serde-rs/serde> |
| serde_yml | MIT | <https://github.com/sebastienrousseau/serde_yml> |
| strsim | MIT | <https://github.com/rapidfuzz/strsim-rs> |
| thiserror | MIT OR Apache-2.0 | <https://github.com/dtolnay/thiserror> |
| tokio, tokio-util | MIT | <https://github.com/tokio-rs/tokio> |
| toml | MIT OR Apache-2.0 | <https://github.com/toml-rs/toml> |
| tower, tower-http | MIT | <https://github.com/tower-rs/tower> |
| tracing, tracing-appender, tracing-subscriber | MIT | <https://github.com/tokio-rs/tracing> |
| uuid | MIT OR Apache-2.0 | <https://github.com/uuid-rs/uuid> |

### Desktop (Tauri) runtime

| Crate | Licence | Upstream |
|---|---|---|
| tauri, tauri-build | MIT OR Apache-2.0 | <https://github.com/tauri-apps/tauri> |

### Platform bindings (Linux desktop)

`gdk`, `glib`, `gtk3`, `webkit2gtk4.1`, `libappindicator3` and their
associated Rust `-sys` bindings are used under **MIT**. The underlying
system libraries (GTK, WebKitGTK, glib) are LGPL and are linked
dynamically at runtime — they are not redistributed as part of the
Parish binaries.

### Dev/test-only

`tempfile`, `wiremock`, `tokio-test`, `serial_test`, `cargo_metadata` —
permissive (MIT / Apache-2.0). Not shipped to end users.

---

## Full licence texts

The following licence texts are reproduced in full because one or more
components above are licensed under them.

### MIT License

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

### Apache License 2.0

Full text: <https://www.apache.org/licenses/LICENSE-2.0.txt>.

Reproduced in the generated `THIRD_PARTY_NOTICES.rust.md` and in the source
trees of every Apache-2.0 dependency bundled with compiled binaries. A
canonical copy must travel with every redistribution; see `deploy/` for the
release-packaging scripts.

### BSD-3-Clause

```
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```

### ISC License

```
Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

### Mozilla Public License 2.0

Full text: <https://www.mozilla.org/en-US/MPL/2.0/>.

MPL-2.0 is a file-level weak copyleft licence. The MapLibre GL JS runtime
(and any transitive MPL-2.0 Rust crates) are included in compiled form;
their source is available at the upstream repository links listed above.
If Rundale modifies any MPL-2.0 file, the modified file remains under
MPL-2.0 and its source must be made available to recipients of the
binary on request.

### Open Database License (ODbL) 1.0

Full text: <https://opendatacommons.org/licenses/odbl/1-0/>.

Applies to OpenStreetMap data consumed via Nominatim and raster tile
servers. The Parish engine is a **produced work** under ODbL §4.5, not a
derivative database, so the share-alike clause (§4.4) does not extend to
the engine source. Attribution (§4.3) is provided visibly in the map UI
and in this file.

### Creative Commons Attribution-ShareAlike 3.0

Full text: <https://creativecommons.org/licenses/by-sa/3.0/legalcode>.

Applies to the National Library of Scotland historic map tiles.

### SIL Open Font License 1.1

Full text: `assets/fonts/NotoSansSymbols2-LICENSE.txt` and
<https://openfontlicense.org/>.

---

## Notes

- The Rust and SQLite bindings pulled in by `rusqlite = { features = ["bundled"] }`
  compile a bundled copy of SQLite. SQLite itself is in the
  [public domain](https://www.sqlite.org/copyright.html); no attribution
  is legally required but it is acknowledged here for transparency.
- Compiled Tauri bundles (`.app`, `.deb`, `.msi`, `.dmg`) include `LICENSE`,
  `NOTICE`, and this file via `tauri.conf.json` → `bundle.resources`.
- The hosted web build (Axum server) serves `/LICENSE`, `/NOTICE`, and
  this file (`/THIRD_PARTY_NOTICES.md`) as public routes alongside the
  static bundle, so the licence travels with the deployed binary as
  GPL-3.0 requires. The corresponding source remains available at
  <https://github.com/dmooney/Parish>.
- This file must be kept in sync with direct dependencies. Any PR that adds
  a new runtime dependency must update the relevant table above and/or
  regenerate the `.rust.md` / `.ui.md` companions.

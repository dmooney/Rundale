# Dot-Chain Suppression Report

Input: `docs/graphics-v2/pipeline-experiments/idea-ah-kilteevan-z17-map-crop.png`
Input size: 768x432
Dark threshold: 154
Candidate area range: 3..45
Candidate max width/height: 10
Candidate min density: 0.42
Minimum chain members/span: 14/220.0
Maximum median chain gap: 16.0
Connected dark components: 594
Compact dot/dash candidates: 427
Candidates marked as long dot/peck chains: 185
Accepted chain segments: 21
Continuous chain strip width: 2.2
Suppressed pixels after dilation: 17985

Method: threshold dark ink, find compact connected components, run a
coarse Hough-style long-chain detector over candidate centers, dilate
only the marked components, and locally fill masked pixels from nearby
unmasked map texture. No location name, road graph, or hand-authored
feature hints are used.

Outputs:
- `idea-aj4-kilteevan-dot-suppressed-mainchains-no-admin-map-crop.png`
- `idea-aj4-kilteevan-dot-suppressed-mainchains-suppression-overlay.png`
- `idea-aj4-kilteevan-dot-suppressed-mainchains-no-admin-oblique-raw-warp.png`

Caveats: this is a prototype. It may miss curved or irregular dotted
administrative boundaries, and it may suppress some legitimate physical
dot chains if they have similar geometry. Always pass the original crop
alongside the cleaned crop, and audit visually before using the cleaned
crop as physical-linework authority.

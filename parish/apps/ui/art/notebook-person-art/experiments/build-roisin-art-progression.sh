#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/../../../../../.." && pwd)"
work_dir="$script_dir/roisin-progression-work"
output="$script_dir/roisin-art-progression.png"

paper="#deccae"
panel="#eadbbd"
ink="#36362e"
muted="#5c5747"
rule="#807661"
font_sans="/System/Library/Fonts/Supplemental/Arial.ttf"
font_serif="/System/Library/Fonts/Supplemental/Georgia.ttf"

mkdir -p "$work_dir"

fit_art() {
    local source="$1"
    local target="$2"
    magick "$source" -resize '640x520>' -gravity center \
        -background "$panel" -extent 640x520 "$target"
}

make_panel() {
    local number="$1"
    local label="$2"
    local caption="$3"
    local art="$4"
    local target="$5"

    magick -size 700x700 "xc:$panel" \
        -stroke "$rule" -strokewidth 2 -fill none \
        -draw 'rectangle 1,1 698,698' \
        "$art" -geometry +30+85 -compose Over -composite \
        \( -background none -fill "$ink" -stroke none -font "$font_sans" \
        -pointsize 25 -gravity northwest -size 640x45 \
        "caption:$number  $label" \) \
        -gravity northwest -geometry +30+24 -compose Over -composite \
        \( -background none -fill "$muted" -stroke none -font "$font_sans" \
        -pointsize 19 -gravity northwest -size 640x55 "caption:$caption" \) \
        -gravity northwest -geometry +30+625 -compose Over -composite "$target"
}

# 01: selected Roisin in the earlier dark Conversation Lens direction.
magick "$repo_root/docs/graphics-v2/concept-7a-conversation-lens.png" \
    -crop 650x650+575+160 +repage -resize '640x520>' -gravity center \
    -background "$panel" -extent 640x520 "$work_dir/art-01.png"

# 02: the concept that established painted world plus pen-and-ink notebook UI.
magick "$repo_root/docs/graphics-v2/illustrated-parish-notebook.png" \
    -crop 900x720+700+70 +repage -resize '640x520>' -gravity center \
    -background "$panel" -extent 640x520 "$work_dir/art-02.png"

fit_art "$repo_root/docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-a/npc-0004-roisin-connolly/a1.png" "$work_dir/art-03.png"
fit_art "$repo_root/docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-b/npc-0004-roisin-connolly/b1.png" "$work_dir/art-04.png"
fit_art "$repo_root/docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-c/npc-0004-roisin-connolly/c1.png" "$work_dir/art-05.png"
fit_art "$repo_root/docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-d/npc-0004-roisin-connolly/d1.png" "$work_dir/art-06.png"
fit_art "$repo_root/docs/graphics-v2/npc-portraits/pipeline-experiments/cycle-e/npc-0004-roisin-connolly/e1.png" "$work_dir/art-07.png"
fit_art "$script_dir/roisin-prompt-v1-portrait.png" "$work_dir/art-08.png"

# 09: remove only the presentation key from the accepted chat calibration.
magick -size 640x520 "xc:$panel" \
    \( "$script_dir/../references/accepted-roisin-chat-portrait-style-keyed.png" \
    -alpha on -fuzz 12% -transparent '#ff00ff' \
    -fill '#36362e' -colorize 100 -resize '520x520>' \) \
    -gravity center -compose Over -composite "$work_dir/art-09.png"

# 10: first live API result, retained as the instructive over-rendered miss.
magick -size 640x520 "xc:$panel" \
    \( "$script_dir/../candidates/objects/50/502172728d4e3241d4aeced93d79b4740da23196c25b9c30342d22e6bb891740/attempts/roisin-live-smoke-v2-20260710141222954-518f5849-a9a6-4043-b54b-6e4faad96079/candidate.png" \
    -resize '520x520>' \) -gravity center -compose Over -composite \
    "$work_dir/art-10.png"

# 11: standalone API calibration that recovered the sparse notebook language.
magick -size 640x520 "xc:$panel" \
    \( "$script_dir/../candidates/objects/29/29e29f2de7050009672abe7b0db28b8075c89e4563c3d3e43bfbd6f026a6a6e9/attempts/reprocess-20260710160634238-35c8dfff-40ac-4ae4-b837-8f3128d11fb2/candidate.png" \
    -resize '520x520>' \) -gravity center -compose Over -composite \
    "$work_dir/art-11.png"

# 12: first shared-call pair. Identity linked, but the portrait was underpainted.
magick \
    \( -size 1024x1024 "xc:$panel" \
    \( "$script_dir/../candidates/objects/cc/cc22d65263d13a38383a4c573e487ece449904869f6b40889a84b9f4460a0299/attempts/roisin-identity-pair-v1-20260710194801379-8f293e27-8e00-4792-902d-1ccf69ed1182/portrait-raw.png" \
    -alpha on -fuzz 12% -transparent '#ff00ff' \) \
    -gravity center -compose Over -composite \) \
    \( -size 1024x1024 pattern:checkerboard \
    \( "$script_dir/../candidates/objects/cc/cc22d65263d13a38383a4c573e487ece449904869f6b40889a84b9f4460a0299/attempts/roisin-identity-pair-v1-20260710194801379-8f293e27-8e00-4792-902d-1ccf69ed1182/marker-raw.png" \
    -alpha on -fuzz 12% -transparent '#ff00ff' \) \
    -gravity center -compose Over -composite \) \
    +append "$work_dir/pair-12.png"
fit_art "$work_dir/pair-12.png" "$work_dir/art-12.png"

make_panel "01" "CONVERSATION LENS" "Painted character marker in the earlier dark interface." "$work_dir/art-01.png" "$work_dir/panel-01.png"
make_panel "02" "NOTEBOOK CONCEPT" "The painted-world and pen-and-ink UI split appears." "$work_dir/art-02.png" "$work_dir/panel-02.png"
make_panel "03" "CYCLE A | REJECTED" "Too polished, too large, and watercolor-heavy." "$work_dir/art-03.png" "$work_dir/panel-03.png"
make_panel "04" "CYCLE B | MEDIUM FIX" "Uncolored, but still a formal portrait drawing." "$work_dir/art-04.png" "$work_dir/panel-04.png"
make_panel "05" "CYCLE C | SCALE FIX" "More paper and smaller scale; linework remains dense." "$work_dir/art-05.png" "$work_dir/panel-05.png"
make_panel "06" "CYCLE D | MARGIN ICON" "Closer to the rough people-list shorthand." "$work_dir/art-06.png" "$work_dir/panel-06.png"
make_panel "07" "CYCLE E | 72 x 82" "Judged at native concept scale instead of master size." "$work_dir/art-07.png" "$work_dir/panel-07.png"
make_panel "08" "CHAT COLOR STUDY" "Fresh Roisin study before the no-color surface rule." "$work_dir/art-08.png" "$work_dir/panel-08.png"
make_panel "09" "CHAT INK TARGET" "The definitive sparse player-notebook portrait target." "$work_dir/art-09.png" "$work_dir/panel-09.png"
make_panel "10" "FIRST API | REJECTED" "The API over-rendered a finished character illustration." "$work_dir/art-10.png" "$work_dir/panel-10.png"
make_panel "11" "API STYLE MATCH" "The sparse, transparent player-sketch contract recovered." "$work_dir/art-11.png" "$work_dir/panel-11.png"
make_panel "12" "FIRST JOINT CALL | REJECTED" "One-call identity worked; the portrait was still filled." "$work_dir/art-12.png" "$work_dir/panel-12.png"

# 13: approved portrait and marker, generated together and shown post-key.
magick -size 2890x900 "xc:$panel" \
    -stroke "$rule" -strokewidth 3 -fill none -draw 'rectangle 1,1 2888,898' \
    \( "$script_dir/roisin-identity-pair-api-v2-review.png" -resize '1440x720>' \) \
    -geometry +725+88 -compose Over -composite \
    \( -background none -fill "$ink" -stroke none -font "$font_sans" \
    -pointsize 34 \
    -gravity northwest -size 2810x55 \
    'caption:13  APPROVED PRODUCTION PAIR' \) \
    -gravity northwest -geometry +40+25 -compose Over -composite \
    \( -background none -fill "$muted" -stroke none -font "$font_sans" \
    -pointsize 23 \
    -gravity northwest -size 2810x45 \
    'caption:One metadata-driven provider call | atomic portrait + marker | approved 2026-07-10' \) \
    -gravity northwest -geometry +40+830 -compose Over -composite \
    "$work_dir/panel-13.png"

magick -size 3010x3350 "xc:$paper" \
    "$work_dir/panel-01.png" -geometry +60+200 -compose Over -composite \
    "$work_dir/panel-02.png" -geometry +790+200 -compose Over -composite \
    "$work_dir/panel-03.png" -geometry +1520+200 -compose Over -composite \
    "$work_dir/panel-04.png" -geometry +2250+200 -compose Over -composite \
    "$work_dir/panel-05.png" -geometry +60+930 -compose Over -composite \
    "$work_dir/panel-06.png" -geometry +790+930 -compose Over -composite \
    "$work_dir/panel-07.png" -geometry +1520+930 -compose Over -composite \
    "$work_dir/panel-08.png" -geometry +2250+930 -compose Over -composite \
    "$work_dir/panel-09.png" -geometry +60+1660 -compose Over -composite \
    "$work_dir/panel-10.png" -geometry +790+1660 -compose Over -composite \
    "$work_dir/panel-11.png" -geometry +1520+1660 -compose Over -composite \
    "$work_dir/panel-12.png" -geometry +2250+1660 -compose Over -composite \
    "$work_dir/panel-13.png" -geometry +60+2390 -compose Over -composite \
    \( -background none -fill "$ink" -stroke none -font "$font_serif" \
    -pointsize 58 \
    -gravity center -size 2890x80 'caption:ROISIN CONNOLLY | ART PROGRESSION' \) \
    -gravity northwest -geometry +60+48 -compose Over -composite \
    \( -background none -fill "$muted" -stroke none -font "$font_sans" \
    -pointsize 24 \
    -gravity center -size 2890x45 \
    'caption:EARLY INTERFACE CONCEPTS TO APPROVED IDENTITY-LOCKED PRODUCTION ART' \) \
    -gravity northwest -geometry +60+126 -compose Over -composite "$output"

printf '%s\n' "$output"

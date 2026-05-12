Evidence type: code diff + test run transcript
Verdict: sufficient
Technical debt: clear

The duplicate struct was removed cleanly. All tests pass, clippy is clean, downstream crates compile. The parish-types AnachronismEntry is now the single source of truth for both mod loading and NPC detection. No remaining debt in this area.

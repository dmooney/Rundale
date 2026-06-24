# Plan: visual-atom-auditor-m12

1. Add a Node script that reads `scenes.json`, parses PNG headers/pixels, and
   audits the Crossroads layer assets.
2. Add `npm run audit:atoms` to the visual client package scripts.
3. Feather any remaining kit atom hard edges that the auditor would correctly
   reject.
4. Add script tests for the PNG parser/audit behavior using tiny generated PNG
   fixtures.
5. Run visual tests, check, build, the M12 audit, and agent-check.

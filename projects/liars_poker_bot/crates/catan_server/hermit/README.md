# beaver on hermit

Not deployed yet (prepared 2026-09-27; it still runs on misc as
catan.fewworddotrick.com). To move it:

1. Deploy: `HERMIT_TOKEN=gsvc1.... scripts/hermit-deploy.sh catan_server beaver`
   (from `projects/liars_poker_bot`). Stateless: nothing to copy.
2. Steven approves public access on hermit's /admin.
3. Point catan.fewworddotrick.com at it (or retire it) in emu-nest's
   Caddyfile, and remove beaver from misc (compose, misc.yml build tasks).

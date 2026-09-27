# heron on hermit

Not deployed yet (prepared 2026-09-27; it still runs on misc as
ohh.fewworddotrick.com). To move it:

1. Deploy: `HERMIT_TOKEN=gsvc1.... scripts/hermit-deploy.sh oh_hell_server heron`
   (from `projects/liars_poker_bot`). It comes up on PIMCTS alone until the
   weights are in place.
2. Weights go to heron's bulk storage (`/bulk`, not backed up). They are on
   barn already, so copy them there (from emu-nest's `ansible/`):

       ansible barn -i hosts -m shell -a 'set -e; D=/mnt/ssd1/hermit-bulk/apps/heron; S=/mnt/ssd1/card_platypus;
         mkdir -p $D; cp -a $S/oh_hell $S/oh_hell.3p_*t_bid $D/; chown -R 1031:1031 $D'

   or upload them through hermit (`PUT /apps/heron/bulk/<path>`, see /help).
   Then `POST /apps/heron/restart`.
3. Steven approves public access on hermit's /admin.
4. Point ohh.fewworddotrick.com at it (or retire it) in emu-nest's Caddyfile,
   and remove heron from misc (compose, misc.yml build tasks).

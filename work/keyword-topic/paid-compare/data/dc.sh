#!/bin/sh
cd /Users/blockall/workspace/Ripple
TUNNEL_TOKEN= PUBLIC_URL= WEB_PORT=3300 BACKUP_HOST_DIR=./.check-backups KIS_APP_KEY= KIS_APP_SECRET= FINNHUB_API_KEY= docker compose -p ripple-check "$@"

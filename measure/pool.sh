#!/bin/sh
# Whether queued writers leave a read with no connection: holds the server's write lock from outside
# its pool, queues WRITERS writes behind it, then times one read. Run on a scratch database only.
#   PGURL    the database the server uses
#   SERVER   the server's address, e.g. http://127.0.0.1:7878
#   KEY      a key the server accepts
#   PROJECT  a project in that database, ITEM an item in it
#   WRITERS  how many writes to queue (default 10); HOLD seconds the lock is held (default 45)
set -eu
WRITERS=${WRITERS:-10}
HOLD=${HOLD:-45}
lock=$(printf '%d' 0x00646f636b657401)
psql "$PGURL" -qAt -c "SELECT pg_advisory_lock($lock), pg_sleep($HOLD)" >/dev/null &
holder=$!
sleep 1
i=0
while [ "$i" -lt "$WRITERS" ]; do
  curl -s -o /dev/null -w "write %{http_code} %{time_total}s\n" -H "Authorization: Bearer $KEY" \
    -H 'Content-Type: application/json' \
    -d "{\"project\":\"$PROJECT\",\"ids\":[\"$ITEM\"],\"tier\":\"normal\"}" "$SERVER/do/priority" &
  i=$((i + 1))
done
sleep 2
waiting=$(psql "$PGURL" -qAt -c "SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND NOT granted")
echo "writers waiting on the lock: $waiting"
curl -s -o /dev/null -w "read %{http_code} %{time_total}s\n" -H "Authorization: Bearer $KEY" \
  "$SERVER/check?project=$PROJECT"
wait $holder
wait

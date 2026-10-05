#!/bin/sh
# The statements one request issues, grouped by query text, read from pg_stat_statements.
# The database must load pg_stat_statements (shared_preload_libraries) and have no other client.
#   PGURL    the database the server reads, as a superuser
#   SERVER   the server's address, e.g. http://127.0.0.1:7878
#   KEY      a key the server accepts
#   ROUTE    the request, e.g. "/summary?project=OWNER/REPO"
set -eu
psql "$PGURL" -qAt -c "CREATE EXTENSION IF NOT EXISTS pg_stat_statements" -c "SELECT pg_stat_statements_reset()" >/dev/null
start=$(date +%s%N)
curl -sf -o /dev/null -H "Authorization: Bearer $KEY" "$SERVER$ROUTE"
end=$(date +%s%N)
echo "wall ms: $(( (end - start) / 1000000 ))"
psql "$PGURL" -P pager=off -c "
SELECT calls, round(total_exec_time::numeric, 1) AS ms, left(regexp_replace(query, '\s+', ' ', 'g'), 140) AS query
FROM pg_stat_statements
WHERE query NOT ILIKE '%pg_stat_statements%'
ORDER BY calls DESC" -c "
SELECT sum(calls) AS statements, round(sum(total_exec_time)::numeric, 1) AS exec_ms
FROM pg_stat_statements WHERE query NOT ILIKE '%pg_stat_statements%'"

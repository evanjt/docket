#!/bin/sh
# Commit latency on the storage a Postgres keeps its WAL on: COMMITS single-row commits in a scratch
# table, timed by the server, with the WAL fsync count and time read from pg_stat_io (Postgres 18).
# The timing settings are set for this session only, so no server setting changes. Needs a superuser.
#   PGURL    a scratch database on the server to measure; the script creates and drops one table
#   COMMITS  how many commits (default 50)
set -eu
COMMITS=${COMMITS:-50}
psql "$PGURL" -qAt -v ON_ERROR_STOP=1 <<SQL
SET track_io_timing = on;
SET track_wal_io_timing = on;
DROP TABLE IF EXISTS commit_probe;
CREATE TABLE commit_probe (n int);
CREATE TEMP TABLE io_before AS
  SELECT coalesce(sum(fsyncs), 0) AS fsyncs, coalesce(sum(fsync_time), 0) AS fsync_ms,
         coalesce(sum(writes), 0) AS writes, coalesce(sum(write_time), 0) AS write_ms
  FROM pg_stat_io WHERE backend_type = 'client backend' AND object = 'wal';
DO \$\$
DECLARE
  started timestamptz;
  took double precision[] := '{}';
BEGIN
  FOR i IN 1..$COMMITS LOOP
    started := clock_timestamp();
    INSERT INTO commit_probe VALUES (i);
    COMMIT;
    took := took || extract(epoch FROM clock_timestamp() - started) * 1000;
  END LOOP;
  RAISE NOTICE 'commits %, median % ms, p90 % ms, max % ms', $COMMITS,
    round((SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY t) FROM unnest(took) t)::numeric, 3),
    round((SELECT percentile_cont(0.9) WITHIN GROUP (ORDER BY t) FROM unnest(took) t)::numeric, 3),
    round((SELECT max(t) FROM unnest(took) t)::numeric, 3);
END
\$\$;
SELECT pg_stat_force_next_flush();
SELECT 'wal fsyncs ' || (a.fsyncs - b.fsyncs) || ', fsync ms ' || round((a.fsync_ms - b.fsync_ms)::numeric, 2)
       || ', per fsync ms ' || round(((a.fsync_ms - b.fsync_ms) / nullif(a.fsyncs - b.fsyncs, 0))::numeric, 3)
       || ', wal writes ' || (a.writes - b.writes) || ', write ms ' || round((a.write_ms - b.write_ms)::numeric, 2)
FROM io_before b,
  (SELECT coalesce(sum(fsyncs), 0) AS fsyncs, coalesce(sum(fsync_time), 0) AS fsync_ms,
          coalesce(sum(writes), 0) AS writes, coalesce(sum(write_time), 0) AS write_ms
   FROM pg_stat_io WHERE backend_type = 'client backend' AND object = 'wal') a;
DROP TABLE commit_probe;
SQL

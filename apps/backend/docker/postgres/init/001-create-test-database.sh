#!/bin/sh
set -eu

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname postgres <<-EOSQL
SELECT 'CREATE DATABASE ${POSTGRES_TEST_DB}'
WHERE NOT EXISTS (
    SELECT 1
    FROM pg_database
    WHERE datname = '${POSTGRES_TEST_DB}'
)\gexec
EOSQL

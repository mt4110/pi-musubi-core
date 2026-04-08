#!/bin/sh
set -eu

psql \
    -v ON_ERROR_STOP=1 \
    -v test_db="$POSTGRES_TEST_DB" \
    --username "$POSTGRES_USER" \
    --dbname postgres <<-'EOSQL'
SELECT format('CREATE DATABASE %I', :'test_db')
WHERE NOT EXISTS (
    SELECT 1
    FROM pg_database
    WHERE datname = :'test_db'
)\gexec
EOSQL

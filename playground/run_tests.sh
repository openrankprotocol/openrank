#!/bin/bash

TRUST_DB_FILE="trust-db.csv"
SEED_DB_FILE="seed-db.csv"

TRUST_DB="https://raw.githubusercontent.com/openrankprotocol/datasets/main/${TRUST_DB_FILE}"
SEED_DB="https://raw.githubusercontent.com/openrankprotocol/datasets/main/${SEED_DB_FILE}"

if ! [ -e $TRUST_DB_FILE ]; then
    curl $TRUST_DB -o $TRUST_DB_FILE
    wait
fi

if ! [ -e $SEED_DB_FILE ]; then
    curl $SEED_DB -o $SEED_DB_FILE
    wait
fi

OUTPUT="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk trust-update -- "./${TRUST_DB_FILE}" "./config.toml")"
echo $OUTPUT
wait

OUTPUT="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk -- seed-update "./${SEED_DB_FILE}" "./config.toml")"
echo $OUTPUT
wait

TX_HASH="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk -- job-run-request "./config.toml")"
echo $TX_HASH
wait

# Sleep 1s until the job is finished
sleep 1s
wait

OUTPUT="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk -- get-results-and-check-integrity $TX_HASH "./config.toml" "./test_vector.csv")"
echo $OUTPUT
wait

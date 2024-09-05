#!/bin/bash

TRUST_DB_FILE="../datasets/trust-db.csv"
SEED_DB_FILE="../datasets/seed-db.csv"

OUTPUT="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk trust-update "./${TRUST_DB_FILE}" "./config.toml")"
echo $OUTPUT
wait

OUTPUT="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk seed-update "./${SEED_DB_FILE}" "./config.toml")"
echo $OUTPUT
wait

TX_HASH="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk job-run-request "./config.toml")"
echo $TX_HASH
wait

# Sleep 1s until the job is finished
sleep 1s
wait

OUTPUT="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk get-results-and-check-integrity $TX_HASH "./config.toml" "./test_vector.csv")"
echo $OUTPUT
wait

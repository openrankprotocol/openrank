#!/bin/bash
curl https://raw.githubusercontent.com/openrankprotocol/datasets/main/small-trust-db.csv -o trust-db.csv
wait
curl https://raw.githubusercontent.com/openrankprotocol/datasets/main/small-seed-db.csv -o seed-db.csv
wait
cargo run -p openrank-sdk trust-update -- "./trust-db.csv" "./openrank-sdk/config.toml"
wait
cargo run -p openrank-sdk -- seed-update "./seed-db.csv" "./openrank-sdk/config.toml"
wait
TX_HASH="$(cargo run -p openrank-sdk -- job-run-request "./openrank-sdk/config.toml")"
wait
sleep 1s
wait
RESULTS="$(cargo run -p openrank-sdk -- get-results $TX_HASH "./openrank-sdk/config.toml")"
wait

EXPECTED_RESULTS="Address(0000000200000000000000000000000000000000): 0.45882937
Address(0000000300000000000000000000000000000000): 0.418254
Address(0000000100000000000000000000000000000000): 0.122916676"

if [ "$RESULTS" != "$EXPECTED_RESULTS" ]
then
    echo "INVALID_RESULT"
    echo -e "EXPECTED:\n$EXPECTED_RESULTS\nGOT:\n$RESULTS"
    exit 1
else
    echo "VALID_RESULT"
fi

#!/bin/bash
curl https://raw.githubusercontent.com/openrankprotocol/datasets/main/trust-db.csv -o trust-db.csv
wait
curl https://raw.githubusercontent.com/openrankprotocol/datasets/main/seed-db.csv -o seed-db.csv
wait
RUSTFLAGS=-Awarnings cargo run -p openrank-sdk trust-update -- "./trust-db.csv" "./openrank-sdk/config.toml"
wait
RUSTFLAGS=-Awarnings cargo run -p openrank-sdk -- seed-update "./seed-db.csv" "./openrank-sdk/config.toml"
wait
TX_HASH="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk -- job-run-request "./openrank-sdk/config.toml")"
wait
sleep 1s
wait
RESULTS="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk -- get-results $TX_HASH "./openrank-sdk/config.toml")"
wait

EXPECTED_RESULTS="Address(00003bfd00000000000000000000000000000000): 0.41635644
Address(0003eba300000000000000000000000000000000): 0.17619567
Address(00043dc000000000000000000000000000000000): 0.1102105
Address(000a27ba00000000000000000000000000000000): 0.043718923
Address(00066a8d00000000000000000000000000000000): 0.017245345
Address(00003e6f00000000000000000000000000000000): 0.013828715
Address(0005fa7100000000000000000000000000000000): 0.0124359485
Address(0000461400000000000000000000000000000000): 0.009337622
Address(0006085200000000000000000000000000000000): 0.009251362
Address(0007c28400000000000000000000000000000000): 0.008563522"

if [ "$RESULTS" != "$EXPECTED_RESULTS" ]
then
    echo "INVALID_RESULT"
    echo -e "EXPECTED:\n$EXPECTED_RESULTS\nGOT:\n$RESULTS"
    exit 1
else
    echo "VALID_RESULT"
fi

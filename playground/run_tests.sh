#!/bin/bash
curl https://raw.githubusercontent.com/openrankprotocol/datasets/main/trust-db.csv -o trust-db.csv
wait
curl https://raw.githubusercontent.com/openrankprotocol/datasets/main/seed-db.csv -o seed-db.csv
wait
RUSTFLAGS=-Awarnings cargo run -p openrank-sdk trust-update -- "./trust-db.csv" "../openrank-sdk/config.toml"
wait
RUSTFLAGS=-Awarnings cargo run -p openrank-sdk -- seed-update "./seed-db.csv" "../openrank-sdk/config.toml"
wait
TX_HASH="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk -- job-run-request "../openrank-sdk/config.toml")"
wait
sleep 1s
wait
RESULTS="$(RUSTFLAGS=-Awarnings cargo run -p openrank-sdk -- get-results $TX_HASH "../openrank-sdk/config.toml")"
wait

EXPECTED_RESULTS="Address(00003bfd00000000000000000000000000000000): 0.4163566
Address(0003eba300000000000000000000000000000000): 0.17619719
Address(00043dc000000000000000000000000000000000): 0.110210575
Address(000a27ba00000000000000000000000000000000): 0.043719307
Address(00066a8d00000000000000000000000000000000): 0.01724547
Address(00003e6f00000000000000000000000000000000): 0.013828792
Address(0005fa7100000000000000000000000000000000): 0.012436048
Address(0000461400000000000000000000000000000000): 0.009337675
Address(0006085200000000000000000000000000000000): 0.009251416
Address(0007c28400000000000000000000000000000000): 0.008563595"

if [ "$RESULTS" != "$EXPECTED_RESULTS" ]
then
    echo "INVALID_RESULT"
    echo -e "EXPECTED:\n$EXPECTED_RESULTS\nGOT:\n$RESULTS"
    exit 1
else
    echo "VALID_RESULT"
fi

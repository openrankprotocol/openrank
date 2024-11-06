## Preparing environment
`datasets` repo can be used to get up to speed, since it contains some example datasets, like Degen Tipping DB or OSO GitHub DB.
It contains the script for installing the SDK, as well as an example `config.toml` needed to run openrank-sdk methods.
To clone the `datasets` repo:
```
git clone https://github.com/openrankprotocol/datasets.git
cd ./datasets
```
Then run `./install-sdk.sh` script:
```bash
./install-sdk.sh
```
The above command will install the cargo (Rust package manager) and openrank-sdk binary.
It will also generate a new keypair that will be used for signing messages using `generate-keypair` command.

The generated secret key should be added to local `.env` file that will be used by OpenRankSDK. So, create `.env` file,
and copy-paste the output from the `generate-keypair` command:
```bash
SECRET_KEY="b0f6d4b7865e1128eebfe4eb37b96522d2e58cbd7892c7e0759907c5f4c6ede4"
# ADDRESS: b79aafc95c8866e65ed51a7856e75587feb481ff
```
If you wish for you address to be whitelisted, send us a request at devs@karma3labs.com.

## Datasets
If you wish to create new datasets to run the compute on, they should be prepared in the following format, e.g.:
- For Seed trust:
```csv
i,v
1,0.1
2,0.5
3,0.4
```
- For local trust
```csv
i,j,v
1,2,30
1,3,40
2,1,10
2,3,50
3,1,5
3,2,25
```

## Usage:
TrustUpdate - Updating a bulk of Trust scores to a specific namespace. It will expect a csv file with `i`,`j`,`v` entries where `i` and `j` are string with arbitrary values, and `v` is integer value:
```sh
openrank-sdk trust-update [TRUST_DB_FILE_PATH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

SeedUpdate - Updating a bulk of Seed scores to a specific namespace. It will expect a csv file with `i`,`v` entries where `i` is a string with arbitrary value and `v` is integer value:
```sh
openrank-sdk seed-update [SEED_DB_FILE_PATH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

ComputeRequest - Request a compute in a domain specified inside `OPENRANK_SDK_CONFIG_PATH` file. The hash of ComputeRequest TX will be returned:
```sh
openrank-sdk compute-request [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

Get Results - Get results of a specific compute request identified by it's TX hash:
```sh
openrank-sdk get-results [ComputeRequest_TX_HASH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

Get Results and Check Integrity - Get the results of a specific compute request, and perform the convergence check of top X amount of scores.
The scores will be compared against a predefined test vector with `TEST_VECTOR_PATH` path:
```sh
openrank-sdk get-results-and-check-integrity [ComputeRequest_TX_HASH] [OPENRANK_SDK_CONFIG_PATH] [TEST_VECTOR_PATH]
```

Get Compute Results - Get ComputeResult object given its identifier:
```sh
openrank-sdk get_compute_result [ComputeRequest_TX_HASH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

Get Compute Results TXs - Get TXs contained in ComputeResult object given:
```sh
openrank-sdk get_compute_result-txs [ComputeRequest_TX_HASH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

Get TX - Get arbitrary TX object given its kind and hash:
```sh
openrank-sdk get_tx [!kind]:[TX_HASH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```
! - kind should be in format: "compute_request", "compute_commitment", etc.
E.g. for how to fetch ComputeRequest TX:
```sh
openrank-sdk get_tx compute_request:3d967111e0e244f62822f64d914ac3c032db85b2284ebc8f5a8bb4fd1273ff74 ./config.toml ./out.json
```

Where:

`TRUST_DB_FILE_PATH` = Path to Trust DB file, a csv file with i,j,v header

`SEED_DB_FILE_PATH` = Path to Seed DB file, a csv file with i,v header

`OUTPUT_PATH` = Path to of the file to write result of the JsonRPC call

`ComputeRequest_TX_HASH` = TX Hash of ComputeRequest TX that was submitted to the protocol

`TX_HASH` = Hash of the TX being requested

`kind` = Kind of the TX: "trust_update", "seed_update", "compute_request", "compute_assignment", "compute_commitment", "compute_verification".

`TEST_VECTOR_PATH` = Path to a file that will contain a vector of scores in csv format (i,v entries),
and will be used for comparing with the scores resulted from the compute

Parameters marked with `*` are optional

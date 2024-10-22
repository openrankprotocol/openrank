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
The output of the command is:
```
SECRET_KEY="b0f6d4b7865e1128eebfe4eb37b96522d2e58cbd7892c7e0759907c5f4c6ede4"
# ADDRESS: b79aafc95c8866e65ed51a7856e75587feb481ff
```
The generated secret key should be added to local `.env` file that will be used by OpenRankSDK. So, create `.env` file,
and copy-paste the output from the `generate-keypair` command:
```bash
SIGNING_KEY="b0f6d4b7865e1128eebfe4eb37b96522d2e58cbd7892c7e0759907c5f4c6ede4"
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
```
openrank-sdk trust-update [TRUST_DB_FILE_PATH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

SeedUpdate - Updating a bulk of Seed scores to a specific namespace. It will expect a csv file with `i`,`v` entries where `i` is a string with arbitrary value and `v` is integer value:
```
openrank-sdk seed-update [SEED_DB_FILE_PATH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

ComputeRequest - Request a compute in a domain specified inside `OPENRANK_SDK_CONFIG_PATH` file. The hash of ComputeRequest TX will be returned:
```
openrank-sdk job-run-request [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

Get Results - Get results of a specific compute request identified by it's TX hash:
```
openrank-sdk get-results [JobRunRequest_TX_HASH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]
```

Get Results and Check Integrity - Get the results of a specific compute request, and perform the convergence check of top X amount of scores.
The scores will be compared against a predefined test vector with `TEST_VECTOR_PATH` path:
```
openrank-sdk get-results-and-check-integrity [JobRunRequest_TX_HASH] [OPENRANK_SDK_CONFIG_PATH] [TEST_VECTOR_PATH]
```

Where:

`TRUST_DB_FILE_PATH` = Path to Trust DB file, a csv file with i,j,v header

`SEED_DB_FILE_PATH` = Path to Seed DB file, a csv file with i,v header

`OUTPUT_PATH` = Path to of the file to write result of the JsonRPC call

`ComputeRequest_TX_HASH` = TX Hash of ComputeRequest TX that was submitted to the protocol

`TEST_VECTOR_PATH` = Path to a file that will contain a vector of scores in csv format (i,v entries),
and will be used for comparing with the scores resulted from the compute

Parameters marked with `*` are optional

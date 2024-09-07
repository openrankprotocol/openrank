TrustUpdate - Updating a bulk of Trust scores to a specific namespace. It will expect a csv file with `i`,`j`,`v` entries where `i` and `j` are string with arbitrary values, and `v` is integer value:

`cargo run -p openrank-sdk trust-update [TRUST_DB_FILE_PATH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]`

SeedUpdate - Updating a bulk of Seed scores to a specific namespace. It will expect a csv file with `i`,`v` entries where `i` is a string with arbitrary value and `v` is integer value:

`cargo run -p openrank-sdk seed-update [SEED_DB_FILE_PATH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]`

JobRunRequest - Request a compute job in a domain specified inside `OPENRANK_SDK_CONFIG_PATH` file. The hash of JobRunRequest TX will be returned:

`cargo run -p openrank-sdk job-run-request [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]`

Get Results - Get results of a specific compute job identified by it TX hash:

`cargo run -p openrank-sdk get-results [JobRunRequest_TX_HASH] [OPENRANK_SDK_CONFIG_PATH] [*OUTPUT_PATH]`

Get Results and Check Integrity - Get the results of a specific job, and perform the convergence check of top X amount of scores.
The scores will be compared against a predefined test vector with `TEST_VECTOR_PATH` path:

`cargo run -p openrank-sdk get-results-and-check-integrity [JobRunRequest_TX_HASH] [OPENRANK_SDK_CONFIG_PATH] [TEST_VECTOR_PATH]`

Where:

`TRUST_DB_FILE_PATH` = Path to Trust DB file, a csv file with i,j,v header

`SEED_DB_FILE_PATH` = Path to Seed DB file, a csv file with i,v header

`OUTPUT_PATH` = Path to of the file to write result of the JsonRPC call

`JobRunRequest_TX_HASH` = TX Hash of JobRunRequest TX that was submitted to the protocol

`TEST_VECTOR_PATH` = Path to a file that will contain a vector of scores in csv format (i,v entries),
and will be used for comparing with the scores resulted from the compute job

Parameters marked with `*` are optional

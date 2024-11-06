# Sequencer node

## JSONRpc endpoints

### `trust_update` - Accepts single signed TrustUpdate TX. Args:
- `tx_str: String` - Hex string of RLP encoded TrustUpdate TX. (hex::encode(rlp::encode(TrustUpdate)))

Returns:
```rust
TxEvent {
  block_number: u64, // number of the block where TX was included (not used)
  proof: InclusionProof, // DA inclusion proof (not used)
  data: Vec<u8>, // Data of the TX, rlp encoded
}
```

### `seed_update` - Accepts single signed SeedUpdate TX. Args:
- `tx_str: String` - Hex string of RLP encoded SeedUpdate TX. (hex::encode(rlp::encode(SeedUpdate)))

Returns:
```rust
TxEvent {
  block_number: u64, // number of the block where TX was included (not used)
  proof: InclusionProof, // DA inclusion proof (not used)
  data: Vec<u8>, // Data of the TX, rlp encoded
}
```

### `compute_request` - Accepts a new signed ComputeRequest TX. Args:
- `tx_str: String` - Hex string of RLP encoded ComputeRequest TX. (hex::encode(rlp::encode(ComputeRequest)))

Returns:
```rust
TxEvent {
  block_number: u64, // number of the block where TX was included (not used)
  proof: InclusionProof, // DA inclusion proof (not used)
  data: Vec<u8>, // Data of the TX, rlp encoded
}
```

### `get_results` - Requests a result of the compute given the `GetResultsQuery` arg. Args:
```rust
GetResultsQuery {
  request_tx_hash: TxHash, // TX hash of the ComputeRequest TX
  start: u32, // Starting index of the score subset
  size: u32 // Size of the score subset
}
```

Returns:
`(Vec<bool>, Vec<ScoreEntry>)` - A touple where .0 is an array of votes from the verifiers and the .1 is a score subset of the whole score set.
i.e. a slice of the original score set with range `start..size`.

### `get_compute_result` - Requests ComputeResult TX. Args:
- `seq_number: u64` - Sequence number of the ComputeResult

Returns:
```rust
struct Result {
    compute_commitment_tx_hash: TxHash, // Hash of the ComputeCommitment TX.
    compute_verification_tx_hashes: Vec<TxHash>, // Hashes of the ComputeVerification TXs.
    compute_request_tx_hash: TxHash, // Hash of the original ComputeRequest TX.
    seq_number: Option<u64>, // Sequence number assigned by the block builder.
}
```

### `get_tx` - Requests a single TX given its `kind` and `hash`. Args:
- `kind: String` - "trust_update", "seed_update", "compute_request", "compute_assignment", "compute_commitment", "compute_verification".
- `tx_hash: TxHash` - TX hash of the TX being requested.

Returns:
```rust
pub struct Tx {
    nonce: u64, // Not used
    from: Address, // Not used
    to: Address, // Not used
    body: Body, // Body of TX
    signature: Signature, // Signature
    sequence_number: Option<u64>, // Sequence number - returned unassigned (None)
}
```

### `get_txs` - Requests a batch of TXs given their `kind` and `hash`. Args:
- `keys: Vec<(String, tx::TxHash)>` - A vector of (kind, hash) pairs for the TXs being requested

Returns:
`Vec<Tx>` - Vector of requested TXs, with it's order preserved

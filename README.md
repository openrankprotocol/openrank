```
Sequencer: RUST_LOG=info cargo run -p openrank-sequencer --release
Block-builder: RUST_LOG=info cargo run -p openrank-block-builder --release "[address-of-sequencer]"
Computer: RUST_LOG=info cargo run -p openrank-computer --release "[address-of-sequencer]"
Verifier: RUST_LOG=info cargo run -p openrank-verifier --release "[address-of-sequencer]"
```

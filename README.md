sequencer - RUST_LOG=info cargo run -p openrank-sequencer --release
block-builder - RUST_LOG=info cargo run -p openrank-block-builder --release "[address-of-sequencer]"
computer - RUST_LOG=info cargo run -p openrank-computer --release "[address-of-sequencer]"
verifier - RUST_LOG=info cargo run -p openrank-verifier --release "[address-of-sequencer]"

# Height Optimized Trie (HOT) - Rust Demo

A simplified Rust implementation of the **Height Optimized Trie (HOT)**, focusing on the core architectural logic of adaptive spans and height-optimized partitioning.

This project is based on the research paper: **"HOT: A Height Optimized Trie Index for Main-Memory Database Systems"** 
Presented at **SIGMOD'18**, June 10-15, 2018, Houston, TX, USA by Günther Specht, Robert Binna, Eva Zangerle, Martin Pichl (University of Innsbruck) and Viktor Leis (Technische Universität München).

### Official Implementation
The original C++ implementation by the authors can be found here:
[https://github.com/speedskater/hot](https://github.com/speedskater/hot)
This version is a simplified Rust implementation used for demonstration purposes only. It does not contain the:
- SIMD instructions (AVX2, `__m256i`).
- BMI2 instructions (PEXT/PDEP).
- Pointer tagging (storing node types in the last bits of pointers).
- Manual memory offsets and copy-on-write with raw buffers that C++ offers.

## Requirements
To run this live-viewer demo, you need the following:

- Rust and Cargo via [rustup.rs](https://rustup.rs/).
- the GUI (powered by `egui`/`eframe`) requires specific development libraries. On Ubuntu/Debian, install them using:
```bash
sudo apt-get update
sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
```

## How to Run

```bash
cargo run
```
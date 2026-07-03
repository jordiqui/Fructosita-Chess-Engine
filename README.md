# Fructosita 🍬♟️

[![CI](https://github.com/josantesbo/Fructosita-Chess-Engine/actions/workflows/ci.yml/badge.svg)](https://github.com/josantesbo/Fructosita-Chess-Engine/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange)
![License](https://img.shields.io/badge/License-MIT-blue)
![UCI](https://img.shields.io/badge/Protocol-UCI-success)
![Status](https://img.shields.io/badge/Status-Active-green)

**Fructosita** is an open-source UCI chess engine written entirely in **Rust**.

The project began on **July 2, 2026** as a personal initiative to learn chess engine development from scratch. The name *Fructosita* was inspired by the author's undergraduate thesis on **fructose**, eventually becoming the engine's permanent identity.

The long-term goal is to evolve Fructosita from a classical handcrafted engine into a modern competitive chess engine featuring advanced search techniques and, eventually, an NNUE evaluation.

---

# Features

### Search

- Alpha-Beta Negamax
- Iterative Deepening
- Quiescence Search
- Aspiration Windows
- Principal Variation Search (planned)

### Move Generation

- Bitboards
- Legal move generation
- Incremental Zobrist hashing
- Static Exchange Evaluation (SEE)
- Magic Bitboards (bishops & rooks)

### Position Evaluation

- Material evaluation
- Piece-Square Tables
- Passed pawn evaluation
- Basic positional heuristics

### Opening

- Polyglot opening book support

### Engine

- UCI compatible
- Transposition Table
- Multi-platform builds
- Continuous Integration via GitHub Actions

---

# What's New in v1.1.0

- ✅ Magic Bitboards for bishops and rooks.
- ✅ Reproducible magic number generator (`find_magics.rs`).
- ✅ Verified against the previous ray-based implementation.
- ✅ More than 2.5 million randomized validation cases.
- ✅ 48 automated tests passing.
- ✅ Approximately 5× faster sliding attack generation in isolation.
- ✅ Approximately 6% faster search performance while preserving identical perft results.

---

# Performance

Current engine status:

| Feature | Status |
|---------|--------|
| UCI Protocol | ✅ |
| Legal Move Generation | ✅ |
| Zobrist Hashing | ✅ |
| Polyglot Book | ✅ |
| SEE | ✅ |
| Magic Bitboards | ✅ |
| Automated Tests | **48 / 48 Passed** |
| Perft Validation | ✅ |

---

# Architecture

```
                 UCI
                  │
                  ▼
             Search Engine
                  │
      ┌───────────┴───────────┐
      ▼                       ▼
 Position Evaluation     Move Generation
      │                       │
      ▼                       ▼
 Piece-Square Tables     Magic Bitboards
      │                       │
      └───────────┬───────────┘
                  ▼
             Bitboard Core
                  │
                  ▼
          Zobrist Hashing
                  │
                  ▼
         Transposition Table
```

---

# Testing

Fructosita includes an extensive automated test suite covering:

- Move generation
- Magic Bitboards
- Zobrist hashing
- Polyglot support
- Static Exchange Evaluation
- Search correctness
- Perft validation

Current status:

```
48 tests passed
0 failed
```

---

# Building

Clone the repository:

```bash
git clone https://github.com/josantesbo/Fructosita-Chess-Engine.git
cd Fructosita-Chess-Engine
```

Compile:

```bash
cargo build --release
```

Run:

```bash
cargo run --release
```

---

# Roadmap

## Search

- [x] Alpha-Beta
- [x] Iterative Deepening
- [x] Quiescence Search
- [x] Transposition Table
- [ ] Late Move Reductions
- [ ] Null Move Pruning
- [ ] History Heuristic
- [ ] Killer Moves
- [ ] MultiPV
- [ ] Lazy SMP

---

## Evaluation

- [x] Material
- [x] PST
- [x] Passed Pawns
- [ ] Mobility
- [ ] King Safety
- [ ] Pawn Structure
- [ ] Space Evaluation
- [ ] Texel Tuning
- [ ] NNUE

---

## Future

- [ ] Syzygy Tablebases
- [ ] Improved Time Management
- [ ] CCRL Testing
- [ ] OpenBench Testing
- [ ] SPRT Regression Testing

---

# Why "Fructosita"?

The engine's name originated during the author's undergraduate research on **fructose**.

Originally intended as a temporary codename, it eventually became the official identity of the project.

---

# Releases

Every tagged release automatically generates binaries for:

- Windows
- Linux
- macOS

using GitHub Actions.

---

# License

This project is released under the MIT License.

---

# Acknowledgements

Fructosita is developed as a personal learning project exploring modern chess engine design.

The project draws inspiration from the broader computer chess community, open research, and many outstanding open-source chess engines that have advanced the field.

---

*"Every strong chess engine started with a legal move generator."* ♟️
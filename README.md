# Solana SPL Token Escrow

An Anchor program for exchanging two SPL tokens atomically between a maker and a taker on Solana.

## Overview

This project demonstrates a complete token escrow lifecycle:

- A maker creates an offer and deposits token A into a program-controlled vault
- The maker can update the requested token B amount while the offer is open
- A taker can accept the offer by paying token B and receiving the deposited token A in the same transaction
- The maker can cancel an open offer and recover the deposited tokens

## Design

Each offer uses an escrow PDA:

```text
["escrow", maker public key, seed]
```

| Account       | Authority  | Purpose                                                   |
| ------------- | ---------- | --------------------------------------------------------- |
| `escrow`      | Program    | Stores maker, mints, requested amount, seed, and bumps    |
| `vault`       | Escrow PDA | Holds token A deposited by the maker                      |
| `maker_ata_a` | Maker      | Supplies token A and receives it again following a refund |
| `maker_ata_b` | Maker      | Receives token B when a taker accepts the offer           |
| `taker_ata_a` | Taker      | Receives token A when the offer is accepted               |
| `taker_ata_b` | Taker      | Supplies the requested token B amount                     |

## Instructions

| Instruction | Purpose                                                     |
| ----------- | ----------------------------------------------------------- |
| `make`      | Create an offer and transfer makers token A into vault      |
| `update`    | Change token B amount requested by the maker                |
| `take`      | Exchange token B for token A atomically and close the offer |
| `refund`    | Return token A to maker and close the cancelled offer       |

## Token and rent flows

### Make

```text
maker_ata_a -- token A --> vault
```

### Take

```text
taker_ata_b -- token B --> maker_ata_b
vault       -- token A --> taker_ata_a
```

### Refund

```text
vault -- token A --> maker_ata_a
```

## Architecture diagrams

### Make

![Make instruction architecture](./arch/make.png)

### Take

![Take instruction architecture](./arch/take.png)

### Refund

![Refund instruction architecture](./arch/refund.png)

## Setup

### 1. Install dependencies

Install Rust, Solana and Anchor

## Testing

Build the program and run tests:

```bash
anchor build
anchor test --skip-local-validator --skip-deploy
```

![All submission tests passing](./tests-passing.png)

## Submission

### Task status

- [x] Implement `make`
- [x] Implement `take`
- [x] Implement `refund`
- [x] Implement `update`
- [x] Add LiteSVM coverage for all base instructions
- [ ] Capture the final passing-test screenshot
- [ ] Implement the optional timed escrow extension

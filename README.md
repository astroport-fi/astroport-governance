# Astroport Governance

[![codecov](https://codecov.io/gh/astroport-fi/astroport-governance/branch/main/graph/badge.svg?token=WDA8WEI7MI)](https://codecov.io/gh/astroport-fi/astroport-governance)

This repo contains Astroport Governance contracts.

## Contracts diagram

![contract diagram](./assets/sc_diagram.png "Contracts Diagram")

## Contracts

| Name                                                                     | Description                                                                                             |
|--------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------|
| [`assembly`](contracts/assembly)                                         | The Astral Assembly governance contract                                                                 |
| [`builder_unlock`](contracts/builder_unlock)                             | ASTRO unlock/vesting contract for Initial Builders                                                      |
| [`emissions_controller`](contracts/emissions_controller)                 | Emissions Controller (Hub) is responsible for receiving vxASTRO votes and managing ASTRO emissions      |
| [`emissions_controller_outpost`](contracts/emissions_controller_outpost) | Emissions Controller (Outpost) is a lightweight satellite for Hub's counterpart which lives on outposts |
| [`voting_escrow`](contracts/voting_escrow)                               | Vote escrowed xASTRO with 14 days lockup                                                                |

## Building Contracts

You will need Rust 1.64.0+ with wasm32-unknown-unknown target installed.

### You can compile each contract:

Go to the repository root and run

```
./scripts/build_release.sh
```

### You can run tests for all contracts

Run the following from the repository root

```
cargo test
```

The optimized contracts are generated in the artifacts/ directory.

## Deployment

Actual deployed contracts and with respective commits [here](https://github.com/astroport-fi/astroport-changelog).

## Docs

Docs can be generated using `cargo doc --no-deps`

## Bug Bounty

The contracts in this repo are included in a [bug bounty program](https://www.immunefi.com/bounty/astroport).

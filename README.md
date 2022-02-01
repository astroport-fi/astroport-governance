# Astroport Governance

This repo contains Astroport Governance related contracts.

## Contracts

| Name                           | Description                      |
| ------------------------------ | -------------------------------- |
| [`builder_unlock`](contracts/builder-unlock) | ASTRO unlock/vesting contract for Initial Builders |
| [`assembly`](contracts/assembly) | The Astral Assembly governance contract |
| [`treasury`](contracts/treasury) | The Astroport DAO Treasury contract |

## Running this contract

You will need Rust 1.44.1+ with wasm32-unknown-unknown target installed.

For a production-ready (compressed) build, run the following command from the repository's root:

```
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.12.3
```

The optimized contracts are generated in the `artifacts/` directory.

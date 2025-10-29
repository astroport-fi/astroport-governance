#!/usr/bin/env bash

set -e
set -o pipefail

projectPath=$(cd "$(dirname "${0}")" && cd ../ && pwd)

docker run -v "$projectPath":/code \
  --mount type=volume,source="$(basename "$projectPath")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.15.1


# terra: https://github.com/CosmWasm/wasmd/blob/a2e48b757b896a2a149b00e9b33e0d1b440d6fac/x/wasm/types/validation.go#L25
# injective: https://github.com/InjectiveLabs/wasmd/blob/e087f275712b5f0a798791495dee0e453d67cad3/x/wasm/types/validation.go#L19
# neutron: https://github.com/neutron-org/neutron/blob/dd3b27211b32ef085056f5e9e3211e124f15674b/app/app.go#L1641
maximum_size=900 # in kB

for artifact in artifacts/*.wasm; do
  artifactsize=$(du -k "$artifact" | cut -f 1)
  if [ "$artifactsize" -gt $maximum_size ]; then
    echo "Artifact file size exceeded: $artifact"
    echo "Artifact size: $artifactsize"
    echo "Max size: $maximum_size"
    exit 1
  fi
done
import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract, executeContract,
} from './helpers.js'
import { join } from 'path'
import {LCDClient} from '@terra-money/terra.js';

const ARTIFACTS_PATH = '../artifacts'

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    let network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    checkParams(network, ["xastroAddress", "tokenAddress"])

    await deployTeamUnlock(terra, wallet)
    await deployAssembly(terra, wallet)
}
async function deployTeamUnlock(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (network.builderUnlockAddress) {
        console.log("builderUnlockAddress already deployed", network.builderUnlockAddress)
        return
    }

    network.builderUnlockAddress = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'builder_unlock.wasm'),
        {
            "owner": wallet.key.accAddress,
            "astro_token": network.tokenAddress,
        }
    )

    console.log("builderUnlockAddress", network.builderUnlockAddress)

    let timestamp = (new Date().getTime() / 1000) | 0;
    let seconds_in_day = 86400;
    let duration = seconds_in_day * 365 * 3; // 3 years

    let msg = Buffer.from(
        JSON.stringify({
            create_allocations: {
                allocations: [
                     [
                        "terra14zees4lwrdds0em258axe7d3lqqj9n4v7saq7e",
                        {
                          amount: "1000000000",
                          unlock_schedule: {
                            start_time: timestamp,
                            cliff: seconds_in_day,
                            duration: duration,
                          },
                          proposed_receiver: undefined,
                        },
                      ],
                    [
                        "terra18wqkwdcz04upyg0eew3vyhepq9rgfl35aq6jw6",
                        {
                          amount: "1000000000",
                          unlock_schedule: {
                            start_time: timestamp,
                            cliff: seconds_in_day,
                            duration: duration,
                          },
                          proposed_receiver: undefined,
                        },
                    ],
                ],
            },
        })
    ).toString("base64");

    let tx = await executeContract(
      terra,
      wallet,
      network.tokenAddress,
      {
        send: {
          contract: network.builderUnlockAddress,
          amount: "2000000000",
          msg
        },
      },
      [],
      `Setting ASTRO Unlocking schedules`
    );

    console.log(tx)

    writeArtifact(network, terra.config.chainID)
}


async function deployAssembly(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (network.assemblyAddress) {
        console.log("assembly already deployed", network.assemblyAddress)
        return
    }

    checkParams(network, ["xastroAddress", "builderUnlockAddress"])

    console.log('Deploying Assembly Contract...')
    network.assemblyAddress = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astro_assembly.wasm'),
        {
            "xastro_token_addr": network.xastroAddress,
            "builder_unlock_addr": network.builderUnlockAddress,
            "proposal_voting_period": 360,
            "proposal_effective_delay": 100,
            "proposal_expiration_period": 100,
            "proposal_required_deposit": '1000000', // 1 xASTRO
            "proposal_required_quorum": 1,
            "proposal_required_threshold": 51
        }
    )

    console.log("assemblyAddress", network.assemblyAddress)

    writeArtifact(network, terra.config.chainID)
}

function checkParams(network:any, required_params: any) {
    for (const k in required_params) {
        if (!network[required_params[k]]) {
            throw "Set required param: " + required_params[k]
        }
    }
}

main().catch(console.log)
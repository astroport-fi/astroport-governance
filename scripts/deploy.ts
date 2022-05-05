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
    await deployVotingEscrow(terra, wallet)
    await deployFeeDistributor(terra, wallet)
    await deployGeneratorController(terra, wallet)
}

async function deployGeneratorController(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    checkParams(network, [
        "votingEscrowAddress",
        "generatorAddress",
        "factoryAddress",
    ])

    if (network.generatorControllerddress) {
        console.log("Generator controller already deployed: ", network.generatorControllerddress)
        return
    }

    console.log('Deploying generator controller...')
    network.generatorControllerddress = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'generator_controller.wasm'),
        {
            "owner": network.multisigAddress,
            "escrow_addr": network.votingEscrowAddress,
            "generator_addr": network.generatorAddress,
            "factory_addr": network.factoryAddress,
            "pools_limit": 12,
        }
    )

    console.log("Generator controller: ", network.generatorControllerddress)

    writeArtifact(network, terra.config.chainID)
}

async function deployFeeDistributor(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    checkParams(network, ["votingEscrowAddress"])

    if (network.feeDistributorAddress) {
        console.log("Fee distributor already deployed: ", network.feeDistributorAddress)
        return
    }

    console.log('Deploying fee distributor...')
    network.feeDistributorAddress = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_escrow_fee_distributor.wasm'),
        {
            "owner": network.multisigAddress,
            "astro_token": network.tokenAddress,
            "voting_escrow_addr": network.votingEscrowAddress,
            "is_claim_disabled": false,
            "claim_many_limit": 12,
        }
    )

    console.log("fee distributor: ", network.feeDistributorAddress)

    writeArtifact(network, terra.config.chainID)
}

async function deployVotingEscrow(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (network.votingEscrowAddress) {
        console.log("votingEscrow already deployed", network.votingEscrowAddress)
        return
    }

    checkParams(network, ["multisigAddress", "xastroAddress"])

    console.log('Deploying votingEscrow...')
    network.votingEscrowAddress = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'voting_escrow.wasm'),
        {
            "owner": network.multisigAddress,
            "guardian_addr": network.assemblyAddress,
            "deposit_token_addr": network.xastroAddress,
            "marketing": {
                "project": "Astroport",
                "description": "Astroport is a neutral marketplace where anyone, from anywhere in the galaxy, can dock to trade their wares.",
                "marketing": "terra1vp629527wwvm9kxqsgn4fx2plgs4j5un0ea5yu",
                "logo": {
                    "url": "https://astroport.fi/vxastro_logo.png"
                }
            },
            "max_exit_penalty": "0.75",
            "slashed_fund_receiver": network.feeDistributorAddress
        }
    )

    console.log("votingEscrow", network.votingEscrowAddress)

    writeArtifact(network, terra.config.chainID)
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
            "max_allocations_amount": String(300_000_000_000000)
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
            "proposal_voting_period": 57600,
            "proposal_effective_delay": 28800,
            "proposal_expiration_period": 201600,
            "proposal_required_deposit": "30000000000", // 30k ASTRO
            "proposal_required_quorum": "0.1", // 10%
            "proposal_required_threshold": '0.50',   // 50%
            "whitelisted_links": ["https://forum.astroport.fi/", "http://forum.astroport.fi/", "https://astroport.fi/", "http://astroport.fi/"]
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

import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract, executeContract, uploadContract, delay,
} from './helpers.js'
import { join } from 'path'
import { LCDClient, LocalTerra, Wallet } from '@terra-money/terra.js';
import { chainConfigs } from "./types.d/chain_configs.js";

const ARTIFACTS_PATH = '../artifacts'
const SECONDS_IN_DAY: number = 60 * 60 * 24 // min, hour, da

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    let property: keyof GeneralInfo;
    for (property in chainConfigs.generalInfo) {
        if (!chainConfigs.generalInfo[property]) {
            throw new Error(`Set required param: ${property}`)
        }
    }

    await deployTeamUnlock(terra, wallet)
    await deployAssembly(terra, wallet)
    await deployVotingEscrow(terra, wallet)

    let network = readArtifact(terra.config.chainID)
    checkParams(network, ["votingEscrowAddress", "assemblyAddress"])

    await deployFeeDistributor(terra, wallet)
    await deployGeneratorController(terra, wallet)
    await deployVotingEscrowDelegation(terra, wallet)
}

async function deployVotingEscrowDelegation(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.nftCodeID) {
        console.log('Register Astroport NFT Contract...')
        network.nftCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_nft.wasm')!)
        writeArtifact(network, terra.config.chainID)
    }

    if (!network.votingEscrowDelegationAddress) {
        chainConfigs.votingEscrowDelegation.admin ||= chainConfigs.generalInfo.multisig
        chainConfigs.votingEscrowDelegation.initMsg.nft_code_id ||= network.nftCodeID
        chainConfigs.votingEscrowDelegation.initMsg.owner ||= network.assemblyAddress
        chainConfigs.votingEscrowDelegation.initMsg.voting_escrow_addr ||= network.votingEscrowAddress

        console.log('Deploying voting escrow delegation...')
        network.votingEscrowDelegationAddress = await deployContract(
            terra,
            wallet,
            chainConfigs.votingEscrowDelegation.admin,
            join(ARTIFACTS_PATH, 'astroport_voting_escrow_delegation.wasm'),
            chainConfigs.votingEscrowDelegation.initMsg,
            chainConfigs.votingEscrowDelegation.label
        )

        console.log("Voting Escrow Delegation: ", network.votingEscrowDelegationAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

async function deployGeneratorController(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.generatorControllerAddress) {
        chainConfigs.generatorController.initMsg.owner ||= network.assemblyAddress
        chainConfigs.generatorController.initMsg.escrow_addr ||= network.votingEscrowAddress
        chainConfigs.generatorController.initMsg.generator_addr ||= chainConfigs.generalInfo.generator_addr
        chainConfigs.generatorController.initMsg.factory_addr ||= chainConfigs.generalInfo.factory_addr
        chainConfigs.generatorController.admin ||= chainConfigs.generalInfo.multisig

        console.log('Deploying generator controller...')
        network.generatorControllerAddress = await deployContract(
            terra,
            wallet,
            chainConfigs.generatorController.admin,
            join(ARTIFACTS_PATH, 'astroport_generator_controller.wasm'),
            chainConfigs.generatorController.initMsg,
            chainConfigs.generatorController.label
        )

        console.log("Generator controller: ", network.generatorControllerAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

async function deployFeeDistributor(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.feeDistributorAddress) {
        chainConfigs.feeDistributor.admin ||= chainConfigs.generalInfo.multisig
        chainConfigs.feeDistributor.initMsg.owner ||= network.assemblyAddress
        chainConfigs.feeDistributor.initMsg.astro_token ||= chainConfigs.generalInfo.astro_token
        chainConfigs.feeDistributor.initMsg.voting_escrow_addr ||= network.votingEscrowAddress

        console.log('Deploying fee distributor...')
        network.feeDistributorAddress = await deployContract(
            terra,
            wallet,
            chainConfigs.feeDistributor.admin,
            join(ARTIFACTS_PATH, 'astroport_escrow_fee_distributor.wasm'),
            chainConfigs.feeDistributor.initMsg,
            chainConfigs.feeDistributor.label,
        )

        console.log("Fee distributor: ", network.feeDistributorAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

async function deployVotingEscrow(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.votingEscrowAddress) {
        checkParams(network, ["assemblyAddress"])
        chainConfigs.votingEscrow.admin ||= chainConfigs.generalInfo.multisig
        chainConfigs.votingEscrow.initMsg.owner ||= network.assemblyAddress
        chainConfigs.votingEscrow.initMsg.deposit_token_addr ||= chainConfigs.generalInfo.xastro_token
        chainConfigs.votingEscrow.initMsg.marketing.marketing ||= chainConfigs.generalInfo.multisig

        console.log('Deploying votingEscrow...')
        network.votingEscrowAddress = await deployContract(
            terra,
            wallet,
            chainConfigs.votingEscrow.admin,
            join(ARTIFACTS_PATH, 'astroport_voting_escrow.wasm'),
            chainConfigs.votingEscrow.initMsg,
            chainConfigs.votingEscrow.label
        )

        console.log("votingEscrow", network.votingEscrowAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

async function deployTeamUnlock(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.builderUnlockAddress) {
        chainConfigs.teamUnlock.admin ||= chainConfigs.generalInfo.multisig
        chainConfigs.teamUnlock.initMsg.owner ||= wallet.key.accAddress
        chainConfigs.teamUnlock.initMsg.astro_token ||= chainConfigs.generalInfo.astro_token

        console.log("Builder Unlock Contract deploying...")
        network.builderUnlockAddress = await deployContract(
            terra,
            wallet,
            chainConfigs.teamUnlock.admin,
            join(ARTIFACTS_PATH, 'astroport_builder_unlock.wasm'),
            chainConfigs.teamUnlock.initMsg,
            chainConfigs.teamUnlock.label
        )
        console.log(`Builder unlock contract address: ${network.builderUnlockAddress}`)

        checkAllocationAmount(chainConfigs.teamUnlock.allocations);
        await create_allocations(terra, wallet, network, chainConfigs.teamUnlock.allocations);

        // Set new owner for builder unlock
        if (chainConfigs.teamUnlock.change_owner) {
            console.log('Propose owner for builder unlock. Ownership has to be claimed within %s days',
                Number(chainConfigs.teamUnlock.propose_new_owner.expires_in) / SECONDS_IN_DAY)
            await executeContract(terra, wallet, network.builderUnlockAddress, {
                "propose_new_owner": chainConfigs.teamUnlock.propose_new_owner
            })
        }
        writeArtifact(network, terra.config.chainID)
    }
}

function checkAllocationAmount(allocations: Allocations[]) {
    let sum = 0;

    for (let builder of allocations) {
        sum += parseInt(builder[1].amount);
    }

    if (sum != parseInt(chainConfigs.teamUnlock.initMsg.max_allocations_amount)) {
        throw new Error(`Sum of allocations is ${sum}, but should be ${chainConfigs.teamUnlock.initMsg.max_allocations_amount}`);
    }
}

async function create_allocations(terra: LocalTerra | LCDClient, wallet: Wallet, network: any, allocations: Allocations[]) {
    if (allocations.length > 0) {
        let from = 0;
        let step = 5;
        let till = allocations.length > step ? step : allocations.length;

        do {
            if (!network[`allocations_created_${from}_${till}`]) {
                let astro_to_transfer = 0;
                let allocations_to_create = [];

                for (let i = from; i < till; i++) {
                    astro_to_transfer += Number(allocations[i][1].amount);
                    allocations_to_create.push(allocations[i]);
                }

                console.log(`from ${from} to ${till}:  ${astro_to_transfer / 1000000} ASTRO to transfer.`);

                // Create allocations : TX
                let tx = await executeContract(terra, wallet, chainConfigs.generalInfo.astro_token,
                    {
                        send: {
                            contract: network.builderUnlockAddress,
                            amount: String(astro_to_transfer),
                            msg: Buffer.from(
                                JSON.stringify({
                                    create_allocations: {
                                        allocations: allocations_to_create,
                                    },
                                })
                            ).toString("base64")
                        },
                    }
                );

                console.log(
                    `Creating ASTRO Unlocking schedules ::: ${from} - ${till}, ASTRO sent : ${astro_to_transfer / 1000000
                    }, \n Tx hash --> ${tx.txhash} \n`
                );

                network[`allocations_created_${from}_${till}`] = true;
                writeArtifact(network, terra.config.chainID);
                await delay(1000);
            }

            from = till;
            step = allocations.length > (till + step) ? step : allocations.length - till
            till += step;
        } while (from < allocations.length);
    } else {
        console.log("Builder Unlock has no allocation points to install")
    }
}

async function deployAssembly(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.assemblyAddress) {
        checkParams(network, ["builderUnlockAddress"])
        chainConfigs.assembly.initMsg.xastro_token_addr ||= chainConfigs.generalInfo.xastro_token
        chainConfigs.assembly.initMsg.builder_unlock_addr ||= network.builderUnlockAddress
        chainConfigs.assembly.admin ||= chainConfigs.generalInfo.multisig

        console.log('Deploying Assembly Contract...')
        network.assemblyAddress = await deployContract(
            terra,
            wallet,
            chainConfigs.assembly.admin,
            join(ARTIFACTS_PATH, 'astroport_assembly.wasm'),
            chainConfigs.assembly.initMsg,
            chainConfigs.assembly.label
        )

        console.log("assemblyAddress", network.assemblyAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

function checkParams(network: any, required_params: any) {
    for (const k in required_params) {
        if (!network[required_params[k]]) {
            throw "Set required param: " + required_params[k]
        }
    }
}

await main()

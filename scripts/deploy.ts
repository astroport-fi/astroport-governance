import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract, executeContract, uploadContract, delay,
} from './helpers.js'
import { join } from 'path'
import {LCDClient, LocalTerra, Wallet} from '@terra-money/terra.js';
import {deployConfigs} from "./types.d/deploy_configs.js";

const ARTIFACTS_PATH = '../artifacts'
const SECONDS_DIVIDER: number = 60 * 60 * 24 // min, hour, da

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    let property: keyof GeneralInfo;
    for (property in deployConfigs.generalInfo){
        if (!deployConfigs.generalInfo[property]) {
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

    console.log("FINISH");
}

async function deployVotingEscrowDelegation(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.nftCodeID) {
        console.log('Register Astroport NFT Contract...')
        network.nftCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_nft.wasm')!)
    }

    if (!network.votingEscrowDelegationAddress) {
        deployConfigs.votingEscrowDelegation.admin ||= wallet.key.accAddress
        deployConfigs.votingEscrowDelegation.initMsg.nft_code_id ||= network.nftCodeID
        deployConfigs.votingEscrowDelegation.initMsg.owner ||= network.assemblyAddress
        deployConfigs.votingEscrowDelegation.initMsg.voting_escrow_addr ||= network.votingEscrowAddress

        console.log('Deploying voting escrow delegation...')
        network.votingEscrowDelegationAddress = await deployContract(
            terra,
            wallet,
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'voting_escrow_delegation.wasm'),
            deployConfigs.votingEscrowDelegation.initMsg,
            deployConfigs.votingEscrowDelegation.label
        )

        console.log("Voting Escrow Delegation: ", network.votingEscrowDelegationAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

async function deployGeneratorController(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.generatorControllerAddress) {
        deployConfigs.generatorController.initMsg.owner ||= network.assemblyAddress
        deployConfigs.generatorController.initMsg.escrow_addr ||= network.votingEscrowAddress
        deployConfigs.generatorController.initMsg.generator_addr ||= deployConfigs.generalInfo.generator_addr
        deployConfigs.generatorController.initMsg.factory_addr ||= deployConfigs.generalInfo.factory_addr

        console.log('Deploying generator controller...')
        network.generatorControllerAddress = await deployContract(
            terra,
            wallet,
            deployConfigs.generatorController.admin,
            join(ARTIFACTS_PATH, 'generator_controller.wasm'),
            deployConfigs.generatorController.initMsg,
            deployConfigs.generatorController.label
        )

        console.log("Generator controller: ", network.generatorControllerAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

async function deployFeeDistributor(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.feeDistributorAddress) {
        deployConfigs.feeDistributor.admin ||= wallet.key.accAddress
        deployConfigs.feeDistributor.initMsg.owner ||= network.assemblyAddress
        deployConfigs.feeDistributor.initMsg.astro_token ||= deployConfigs.generalInfo.astro_token
        deployConfigs.feeDistributor.initMsg.voting_escrow_addr ||= network.votingEscrowAddress

        console.log('Deploying fee distributor...')
        network.feeDistributorAddress = await deployContract(
            terra,
            wallet,
            deployConfigs.feeDistributor.admin,
            join(ARTIFACTS_PATH, 'astroport_escrow_fee_distributor.wasm'),
            deployConfigs.feeDistributor.initMsg,
            deployConfigs.feeDistributor.label,
        )

        console.log("Fee distributor: ", network.feeDistributorAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

async function deployVotingEscrow(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.votingEscrowAddress) {
        checkParams(network, ["assemblyAddress"])
        deployConfigs.votingEscrow.admin ||= wallet.key.accAddress
        deployConfigs.votingEscrow.initMsg.owner ||= network.assemblyAddress
        deployConfigs.votingEscrow.initMsg.deposit_token_addr ||= deployConfigs.generalInfo.xastro_token

        console.log('Deploying votingEscrow...')
        network.votingEscrowAddress = await deployContract(
            terra,
            wallet,
            deployConfigs.votingEscrow.admin,
            join(ARTIFACTS_PATH, 'voting_escrow.wasm'),
            deployConfigs.votingEscrow.initMsg,
            deployConfigs.votingEscrow.label
        )

        console.log("votingEscrow", network.votingEscrowAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

async function deployTeamUnlock(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.builderUnlockAddress) {
        deployConfigs.teamUnlock.admin ||= wallet.key.accAddress
        deployConfigs.teamUnlock.initMsg.owner ||= wallet.key.accAddress
        deployConfigs.teamUnlock.initMsg.astro_token ||= deployConfigs.generalInfo.astro_token

        console.log("Builder Unlock Contract deploying...")
        network.builderUnlockAddress = await deployContract(
            terra,
            wallet,
            deployConfigs.teamUnlock.admin,
            join(ARTIFACTS_PATH, 'builder_unlock.wasm'),
            deployConfigs.teamUnlock.initMsg,
            deployConfigs.teamUnlock.label
        )
        console.log(`Builder unlock contract address: ${network.builderUnlockAddress}`)

        checkAllocationAmount(deployConfigs.teamUnlock.setup_allocations.allocations.create_allocations.allocations);
        await create_allocations(terra, wallet, network, deployConfigs.teamUnlock.setup_allocations.allocations.create_allocations.allocations);

        // Set new owner for builder unlock
        if (deployConfigs.teamUnlock.change_owner) {
            console.log('Propose owner for builder unlock. Ownership has to be claimed within %s days',
                Number(deployConfigs.teamUnlock.propose_new_owner.expires_in) / SECONDS_DIVIDER)
            await executeContract(terra, wallet, network.builderUnlockAddress, {
                "propose_new_owner": deployConfigs.teamUnlock.propose_new_owner
            })
        }
    }
}

function checkAllocationAmount(allocations: Allocations[]) {
    let sum = 0;

    for (let builder of allocations) {
        sum += parseInt(builder[1].amount);
    }

    if (sum != parseInt(deployConfigs.teamUnlock.initMsg.max_allocations_amount)) {
        throw new Error(`Sum of allocations is ${sum}, but should be ${deployConfigs.teamUnlock.initMsg.max_allocations_amount}`);
    }
}

async function create_allocations(terra: LocalTerra | LCDClient, wallet: Wallet, network: any, allocations: Allocations[]) {
    let from = 0;
    let till = allocations.length > 5 ? 5: allocations.length;

    do {
        if (!network[`allocations_created_${from}_${till}`]) {
            let astro_to_transfer = 0;
            let allocations_to_create = [];

            for (let i=from; i<till; i++) {
                astro_to_transfer += Number(allocations[i][1].amount);
                allocations_to_create.push(allocations[i]);
            }

            console.log(`from ${from} to ${till}:  ${astro_to_transfer / 1000000} ASTRO to transfer.`);

            // Create allocations : TX
            let tx = await executeContract(terra, wallet, deployConfigs.generalInfo.astro_token,
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
                },
                [],
                deployConfigs.teamUnlock.setup_allocations.memo
            );

            console.log(
                `Creating ASTRO Unlocking schedules ::: ${from} - ${till}, ASTRO sent : ${
                    astro_to_transfer / 1000000
                }, \n Tx hash --> ${tx.txhash} \n`
            );

            network[`allocations_created_${from}_${till}`] = true;
            writeArtifact(network, terra.config.chainID);
            await delay(1000);
        }

        from = till;
        till += 5;
    } while (from<allocations.length);
}

async function deployAssembly(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.assemblyAddress) {
        checkParams(network, ["builderUnlockAddress"])
        deployConfigs.assembly.initMsg.xastro_token_addr ||= deployConfigs.generalInfo.xastro_token
        deployConfigs.assembly.initMsg.builder_unlock_addr ||= network.builderUnlockAddress
        deployConfigs.assembly.admin ||= wallet.key.accAddress

        console.log('Deploying Assembly Contract...')
        network.assemblyAddress = await deployContract(
            terra,
            wallet,
            deployConfigs.assembly.admin,
            join(ARTIFACTS_PATH, 'astro_assembly.wasm'),
            deployConfigs.assembly.initMsg,
            deployConfigs.assembly.label
        )

        console.log("assemblyAddress", network.assemblyAddress)
        writeArtifact(network, terra.config.chainID)
    }
}

function checkParams(network:any, required_params: any) {
    for (const k in required_params) {
        if (!network[required_params[k]]) {
            throw "Set required param: " + required_params[k]
        }
    }
}

await main()

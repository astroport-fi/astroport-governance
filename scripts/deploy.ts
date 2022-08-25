import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract, executeContract, uploadContract, toEncodedBinary,
} from './helpers.js'
import { join } from 'path'
import {LCDClient} from '@terra-money/terra.js';
import {deployConfigs} from "./types.d/deploy_configs.js";

const ARTIFACTS_PATH = '../artifacts'

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
        let tx = await executeContract(terra, wallet, deployConfigs.generalInfo.astro_token,
            {
                send: {
                    contract: network.builderUnlockAddress,
                    amount: deployConfigs.teamUnlock.setup_allocations.total_allocation_amount,
                    msg: toEncodedBinary(deployConfigs.teamUnlock.setup_allocations.allocations)
                },
            },
            [],
            deployConfigs.teamUnlock.setup_allocations.memo
        );

        console.log(tx)
        writeArtifact(network, terra.config.chainID)
    }
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

import 'dotenv/config'
import {
    newClient,
    readArtifact,
    executeContract,
    updateContractAdmin,
} from './helpers.js'
import {LCDClient, Wallet} from "@terra-money/terra.js";

async function proposeNewOwner(terra: LCDClient, wallet: Wallet, newOwner: string, contractAddress: string) {
    try {
        await executeContract(terra, wallet, contractAddress, {
            propose_new_owner: {
                owner: newOwner,
                expires_in: 1234567
            }
        });
    } catch (e: any) {
        console.log(e.response.data.message)
    }
}

async function updateAdmin(terra: LCDClient, wallet: Wallet, newAdminAddress: string, contractAddress: string) {
    try {
        await updateContractAdmin(terra, wallet, newAdminAddress, contractAddress);
    } catch (e: any) {
        console.log(e.response.data.message)
    }
}

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    // create propose new owner for our contracts
    for (const key in network) {
        console.log(`Updating owner for ${key}: ${network[key]}`);
        await proposeNewOwner(terra, wallet, network.assemblyAddress, network[key]);
    }

    // update admin for our contracts
    for (const key in network) {
        console.log(`Updating ${key}: ${network[key]}`);
        await updateAdmin(terra, wallet, network.assemblyAddress, network[key]);
    }

    const stablePairs = readArtifact("stablepairs-pisco")
    // update admin for pools
    for (const obj of stablePairs) {
        console.log(`Updating: `, obj["contractAddr"]);
        await updateAdmin(terra, wallet, network.assemblyAddress, obj["contractAddr"]);
    }

    console.log('FINISH')
}

main().catch(console.log)

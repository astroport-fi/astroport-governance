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

async function claimOwnership(terra: LCDClient, wallet: Wallet, contractAddress: string) {
    try {
        await executeContract(terra, wallet, contractAddress, {
            claim_ownership: {}
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
        //await claimOwnership(terra, wallet, network[key]);
    }

    // update admin for our contracts
    for (const key in network) {
        console.log(`Updating ${key}: ${network[key]}`);
        await updateAdmin(terra, wallet, network.assemblyAddress, network[key]);
    }

    const pools = readArtifact("stablepairs-pisco")
    // update admin for pools
    for (const pool of pools) {
        console.log(`Updating: `, pool["contractAddr"]);
        await updateAdmin(terra, wallet, network.assemblyAddress, pool["contractAddr"]);
    }

    console.log('FINISH')
}

main().catch(console.log)

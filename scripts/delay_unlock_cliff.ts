import { LCDClient, LocalTerra, Wallet } from "@terra-money/terra.js";
import "dotenv/config";
import { executeContract, newClient, queryContract, readArtifact } from "./helpers.js";

const NEW_CLIFF = 32054400; // distribution starts on 14 Jun 2022 00:00

async function main() {
    const { terra, wallet } = newClient();
    console.log(
        `chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`
    );

    let network = readArtifact(terra.config.chainID);
    console.log("network:", network);

    let allocations = await fetch_all_allocations(terra, network);
    await set_new_cliff(terra, wallet, network, allocations, NEW_CLIFF);
    await check_new_cliffs_are_set(terra, network, NEW_CLIFF);

}

async function fetch_all_allocations(terra: LCDClient | LocalTerra, network: any) {
    console.log("Fetching allocations...");

    let allocations = [];
    let start_after = undefined;
    let last_received_count = undefined;
    do {
        let sub_result: any[] = await queryContract(terra, network.builderUnlockAddress, {
            allocations:
            {
                start_after,
                limit: 30
            }
        });

        for (let allocation of sub_result) {
            allocations.push([allocation[0], allocation[1].unlock_schedule.cliff]);
            console.log(`account: ${allocation[0]}, start_time: ${allocation[1].unlock_schedule.start_time}, cliff: ${allocation[1].unlock_schedule.cliff}`);
        }

        last_received_count = sub_result.length;
        if (last_received_count) { start_after = sub_result[sub_result.length - 1][0]; }
    } while (last_received_count);

    return allocations
}

async function set_new_cliff(terra: LCDClient | LocalTerra, wallet: Wallet, network: any, allocations: (string | number)[][], new_cliff: number) {
    console.log("Setting new cliff...");

    let new_cliffs = allocations.map(account => [account[0], new_cliff]);
    await executeContract(terra, wallet, network.builderUnlockAddress, {
        "increase_cliff": {
            new_cliffs
        }
    });
}

async function check_new_cliffs_are_set(terra: LCDClient | LocalTerra, network: any, new_cliff: number) {
    console.log("Checking new cliffs are set...");

    let allocations = await fetch_all_allocations(terra, network);
    allocations.forEach(allocation => { if (allocation[1] != new_cliff) { throw "New cliff wasn't set!" } })
    console.log("Completed successfully!");
}

main().catch(console.log);

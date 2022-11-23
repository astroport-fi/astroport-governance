import { LCDClient, LocalTerra, Wallet } from "@terra-money/terra.js";
import "dotenv/config";
import { executeContract, newClient, queryContract, readArtifact } from "./helpers.js";

const NEW_START_TIME = 1656633600; // July 1st 2022 12:00:00
const NEW_CLIFF = 31536000; // distribution starts on July 1st 2023 12:00:00
const NEW_DURATION = 94694400; // July 1st 2025 12:00:00

async function main() {
    const { terra, wallet } = newClient();
    console.log(
        `chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`
    );

    let network = readArtifact(terra.config.chainID);
    console.log("network:", network);

    let allocations = await fetch_all_allocations(terra, network);
    await set_new_schedule(terra, wallet, network, allocations, NEW_CLIFF, NEW_START_TIME, NEW_DURATION);
    await check_new_cliffs_are_set(terra, network, NEW_CLIFF, NEW_START_TIME, NEW_DURATION);

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
            allocations.push([allocation[0], allocation[1].unlock_schedule.start_time, allocation[1].unlock_schedule.cliff, allocation[1].unlock_schedule.duration]);
            console.log(`account: ${allocation[0]}, start_time: ${allocation[1].unlock_schedule.start_time}, cliff: ${allocation[1].unlock_schedule.cliff}, duration: ${allocation[1].unlock_schedule.duration}`);
        }

        last_received_count = sub_result.length;
        if (last_received_count) { start_after = sub_result[sub_result.length - 1][0]; }
    } while (last_received_count);

    return allocations
}

async function set_new_schedule(
    terra: LCDClient | LocalTerra,
    wallet: Wallet,
    network: any,
    allocations: (string | number)[][],
    new_cliff: number,
    new_start_time: number,
    new_duration: number
    )
{
    console.log("Setting new schedule...");

    let new_unlock_schedules = allocations.map(account => [account[0], { start_time: new_start_time, cliff: new_cliff, duration: new_duration }]);
    console.log("New allocation schedules", new_unlock_schedules);

    await executeContract(terra, wallet, network.builderUnlockAddress, {
        "update_unlock_schedules": {
            new_unlock_schedules
        }
    });
}

async function check_new_cliffs_are_set(
    terra: LCDClient | LocalTerra,
    network: any,
    new_cliff: number,
    new_start_time: number,
    new_duration: number
) {
    console.log("Checking new schedules are set...");

    let allocations = await fetch_all_allocations(terra, network);
    allocations.forEach(allocation => {
        if (allocation[1] != new_start_time) {throw "New start time wasn't set!"}
        if (allocation[2] != new_cliff) {throw "New cliff wasn't set!"}
        if (allocation[3] != new_duration) {throw "New duration wasn't set!"}
    })
    console.log("Completed successfully!");
}

main().catch(console.log);

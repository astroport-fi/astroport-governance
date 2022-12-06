import { LCDClient, LocalTerra, MsgExecuteContract, Wallet } from "@terra-money/terra.js";
import "dotenv/config";
import { executeContract, newClient, queryContract } from "./helpers.js";
import { BUILDER_UNLOCK_ADDRESS, fetch_all_allocations, multisig, NEW_CLIFF, NEW_DURATION, NEW_START_TIME } from "./delay_unlock_cliff"

async function main() {
    let isClassic = process.env.CHAIN_ID == "columbus-5";

    var { terra, wallet } = newClient(isClassic);
    console.log(
        `chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`
    );

    let allocations = await fetch_all_allocations(terra);

    await set_new_schedule(terra, wallet, allocations, NEW_CLIFF, NEW_START_TIME, NEW_DURATION);
    await check_new_cliffs_are_set(terra, NEW_CLIFF, NEW_START_TIME, NEW_DURATION);

    await return_ownership(terra, wallet, multisig)
}

async function set_new_schedule(
    terra: LCDClient | LocalTerra,
    wallet: Wallet,
    allocations: (string | number)[][],
    new_cliff: number,
    new_start_time: number,
    new_duration: number
) {
    console.log("Setting new schedule...");

    let new_unlock_schedules = allocations.map(account => [account[0], { start_time: new_start_time, cliff: new_cliff, duration: new_duration }]);
    console.log("New allocation schedules", new_unlock_schedules);

    let msg = {
        "update_unlock_schedules": {
            new_unlock_schedules
        }
    }

    await executeContract(terra, wallet, BUILDER_UNLOCK_ADDRESS, msg);

};


async function check_new_cliffs_are_set(
    terra: LCDClient | LocalTerra,
    new_cliff: number,
    new_start_time: number,
    new_duration: number
) {
    console.log("Checking new schedules are set...");

    let allocations = await fetch_all_allocations(terra);
    allocations.forEach(allocation => {
        if (allocation[1] != new_start_time) { throw "New start time wasn't set!" }
        if (allocation[2] != new_cliff) { throw "New cliff wasn't set!" }
        if (allocation[3] != new_duration) { throw "New duration wasn't set!" }
    })
    console.log("Completed successfully!");
}

async function return_ownership(terra: LCDClient | LocalTerra,
    wallet: Wallet,
    multisig: string) {
    await executeContract(terra, wallet, BUILDER_UNLOCK_ADDRESS, { propose_new_owner: { new_owner: multisig, expires_in: 604800 } })
}

main().catch(console.log);

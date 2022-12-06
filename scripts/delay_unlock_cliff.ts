import { LCDClient, LocalTerra, MsgExecuteContract, Wallet } from "@terra-money/terra.js";
import "dotenv/config";
import { executeContract, newClient, queryContract } from "./helpers.js";

export const NEW_START_TIME = 1656633600; // July 1st 2022 12:00:00
export const NEW_CLIFF = 31536000; // distribution starts on July 1st 2023 12:00:00
export const NEW_DURATION = 94694400; // July 1st 2025 12:00:00

// Mainnet
export const BUILDER_UNLOCK_ADDRESS = "terra1zdfquayrfx6jzxmxaltnhq8jfhas7xyrg4q9hrtwvcuyjhgd7qdsgmkgrz"
export const multisig = "terra174gu7kg8ekk5gsxdma5jlfcedm653tyg6ayppw"

// Mainnet classic
// export const BUILDER_UNLOCK_ADDRESS = "terra1fh27l8h4s0tfx9ykqxq5efq4xx88f06x6clwmr"
// export const multisig = "terra1c7m6j8ya58a2fkkptn8fgudx8sqjqvc8azq0ex"


async function main() {

    let isClassic = process.env.CHAIN_ID == "columbus-5";

    var { terra, wallet } = newClient(isClassic);
    console.log(
        `chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`
    );

    await claim_ownership(terra, wallet);

    let allocations = await fetch_all_allocations(terra);

    await simulate_setting_new_schedule(terra, wallet, allocations, NEW_CLIFF, NEW_START_TIME, NEW_DURATION);
}

async function claim_ownership(terra: LCDClient | LocalTerra,
    wallet: Wallet
) {
    await executeContract(terra, wallet, BUILDER_UNLOCK_ADDRESS, { claim_ownership: {} })
}

export async function fetch_all_allocations(terra: LCDClient | LocalTerra) {
    console.log("Fetching allocations...");

    let allocations = [];
    let start_after = undefined;
    let last_received_count = undefined;
    do {
        let sub_result: any[] = await queryContract(terra, BUILDER_UNLOCK_ADDRESS, {
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

async function simulate_setting_new_schedule(
    terra: LCDClient | LocalTerra,
    wallet: Wallet,
    allocations: (string | number)[][],
    new_cliff: number,
    new_start_time: number,
    new_duration: number
) {
    console.log("Simulate setting new schedule...");

    let new_unlock_schedules = allocations.map(account => [account[0], { start_time: new_start_time, cliff: new_cliff, duration: new_duration }]);
    console.log("New allocation schedules", new_unlock_schedules);

    let msg = {
        "update_unlock_schedules": {
            new_unlock_schedules
        }
    }

    const executeMsg = new MsgExecuteContract(
        wallet.key.accAddress,
        BUILDER_UNLOCK_ADDRESS,
        msg,
        undefined
    );
    let sequence = await wallet.sequence()
    let fee = await terra.tx.estimateFee([{ sequenceNumber: sequence }], { msgs: [executeMsg] })
    console.log(`Required fee: ${fee.amount}`)
};

main().catch(console.log);
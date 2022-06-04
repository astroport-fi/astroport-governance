import "dotenv/config";
import {LCDClient, LocalTerra, Wallet,} from "@terra-money/terra.js";
import {deployContract, executeContract, newClient, readArtifact, writeArtifact,} from "./helpers.js";
import {join} from "path";

const ARTIFACTS_PATH = "../artifacts";

const START_TIME = 1654646400; // 8 June 2022 00:00
const CLIFF = 16329600; // distribution starts on 14 Dec 2022 00:00
const UNLOCK_DURATION = 63072000; // 2 years + 1 day since 2024 is a leap year

async function main() {
    // terra, wallet
    const {terra, wallet} = newClient();
    console.log(
        `chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`
    );

    // Network : stores contract addresses
    let network = readArtifact(terra.config.chainID);
    console.log("network:", network);

    const TOKEN_ADDR = network.tokenAddress;

    // ASTRO token addresss should be set
    if (terra.config.chainID == "phoenix-1" && !TOKEN_ADDR) {
        console.log(
            `Please deploy the CW20-base ASTRO token, and then set this address in the deploy config before running this script...`
        );
        return;
    }

    /*************************************** VESTING ::: DEPOYMENT AND INITIALIZATION  *****************************************/

    if (terra.config.chainID == "phoenix-1") {
        const MAX_ALLOC_AMOUNT = 300_000_000_000100;

        // VESTING CONTRACT ::: DEPLOYMENT
        if (!network.builderUnlockAddress) {
            console.log(`${terra.config.chainID} :: Deploying Unlocking Contract`);

            network.builderUnlockAddress = await deployContract(
                terra,
                wallet,
                network.multisigAddress,
                join(ARTIFACTS_PATH, 'builder_unlock.wasm'),
                {
                    "owner": wallet.key.accAddress,
                    "astro_token": TOKEN_ADDR,
                    "max_allocations_amount": String(MAX_ALLOC_AMOUNT)
                },
                "Astroport Builder Unlocking Contract"
            )

            console.log("builderUnlockAddress", network.builderUnlockAddress)
            writeArtifact(network, terra.config.chainID);
            console.log(
                `${terra.config.chainID} :: Unlocking Contract Address : ${network.builderUnlockAddress} \n`
            );
        }

        // ALLOCATIONS
        let allocations: Array<[string, { amount: String; unlock_schedule: any; proposed_receiver: any }]>;

        // ALLOCATIONS DETAILS
        allocations = [
            [
                "terra1nj7umezl9xdqrsd5n0hzcct0kwadkuc726xpdt",
                {
                    amount: String(112_383_407_330000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1nupt9dl6sqhc6eve8dwqsrww2panvju4wxrulp",
                {
                    amount: String(10_456_400_835900),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1kyndl58gmxnz859j9wm5k85lwzqyhc9jqw3fk6",
                {
                    amount: String(10_343_900_835900),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1qyxz6nl2pqq8agnmtjdkp3xa90fdt53nndf4en",
                {
                    amount: String(10_343_900_835900),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra18yct96pxtanxkym52mnw3lcems909pdlvuedhd",
                {
                    amount: String(4_202_903_750100),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1ltryq9mrvk0esdhsrs7dgcehcv0uw5chd2smgn",
                {
                    amount: String(7_428_306_375800),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1svlx2775tg2dlfwkpcvu49q4y4xgefp3ftyk0z",
                {
                    amount: String(116_666_670000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1zaqeperrwghqlsa9yykzsjaets54mtq0u6kl60",
                {
                    amount: String(15_000_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra10yfjdgrj40yckeh5gju86fzyyrw46va48ajxg4",
                {
                    amount: String(547_445_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1frrme65c3rxngyry6j44ahwusha6mkkxefu0tr",
                {
                    amount: String(182_481_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra19xmydpl3zdnw2ef2mnresrsurn3g23e07a8xya",
                {
                    amount: String(200_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1jy3vlu9x2fc2slundxz0kvj7n5y9hjlj6h0hkw",
                {
                    amount: String(100_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1cgdmn0n2x4jj4awnwehstfsy42stcrfqvxcf66",
                {
                    amount: String(1_950_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1pq02fnrm68x6kcv2lhgvyetjelps550w3pq6m2",
                {
                    amount: String(6_000_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1kkwklh7kyr20ktq29uctagkxc7rc27ymp2gf3h",
                {
                    amount: String(1_500_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra14rg786jljcyt08mpfjqfe0tyqtkc5ku07u5cpl",
                {
                    amount: String(1_550_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1gn53cj0v8kvwxqg867e3mu9f3q9yzskmfgnvla",
                {
                    amount: String(2_000_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra17j6tjl2zxd0lugwz3vvsvjcl0z34kh9hqaa63l",
                {
                    amount: String(4_750_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1630fz3hu9np4fwdqt42eduu69hdzv8yfd3mcdp",
                {
                    amount: String(3_500_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1lv845g7szf9m3082qn3eehv9ewkjjr2kdyz0t6",
                {
                    amount: String(600_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1a7rqwyn3zgymqjwhde27d3208muhy8zvgyng6l",
                {
                    amount: String(300_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1jjq6vyq5am5q7tzchc9252y0aczvjtj5ju5hu2",
                {
                    amount: String(670_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1h5cankw4vjf2q5cuepxww4cmefww0ds0qqgem7",
                {
                    amount: String(15_672_765_538700),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1t2fj6czytujh22dwe8zx4sduqkrcpda758mn0q",
                {
                    amount: String(14_821_821_827800),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1l750ue570u3xwm8008ncs5cw22pwrsz0yawztp",
                {
                    amount: String(32_080_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
            [
                "terra1q4pxqn3ytlt4wqkdpkt76mx6v4v8h2zakye4jn",
                {
                    amount: String(43_300_000_000000),
                    unlock_schedule: {
                        start_time: START_TIME,
                        cliff: CLIFF,
                        duration: UNLOCK_DURATION,
                    },
                    proposed_receiver: undefined,
                },
            ],
        ];

        let sum = 0;
        for (let builder of allocations) {
            sum += parseInt(builder[1].amount.toString());
        }
        if (sum != MAX_ALLOC_AMOUNT) {
            throw new Error(`Sum of allocations is ${sum}, but should be ${MAX_ALLOC_AMOUNT}`);
        } else {
            console.log("Sum of allocations is correct");
        }

        // Create allocations tx : 0-5
        if (!network.allocations_created_0_5) {
            console.log("Creating allocations tx 0-5");
            await create_allocations(
                terra,
                wallet,
                TOKEN_ADDR,
                network.builderUnlockAddress,
                allocations,
                0,
                5
            );
            network.allocations_created_0_5 = true;
            writeArtifact(network, terra.config.chainID);
            await delay(1000);
        }

        // Create allocations tx : 6-10
        if (!network.allocations_created_6_10) {
            console.log("Creating allocations tx 6-10");
            await create_allocations(
                terra,
                wallet,
                TOKEN_ADDR,
                network.builderUnlockAddress,
                allocations,
                6,
                10
            );
            network.allocations_created_6_10 = true;
            writeArtifact(network, terra.config.chainID);
            await delay(1000);
        }

        // Create allocations tx : 11-15
        if (!network.allocations_created_11_15) {
            console.log("Creating allocations tx 11-15");
            await create_allocations(
                terra,
                wallet,
                TOKEN_ADDR,
                network.builderUnlockAddress,
                allocations,
                11,
                15
            );
            network.allocations_created_11_15 = true;
            writeArtifact(network, terra.config.chainID);
            await delay(1000);
        }

        // Create allocations tx : 16-20
        if (!network.allocations_created_16_20) {
            console.log("Creating allocations tx 16-20");
            await create_allocations(
                terra,
                wallet,
                TOKEN_ADDR,
                network.builderUnlockAddress,
                allocations,
                16,
                20
            );
            network.allocations_created_16_20 = true;
            writeArtifact(network, terra.config.chainID);
            await delay(1000);
        }

        // Create allocations tx : 21-25
        if (!network.allocations_created_21_25) {
            console.log("Creating allocations tx 21-25");
            await create_allocations(
                terra,
                wallet,
                TOKEN_ADDR,
                network.builderUnlockAddress,
                allocations,
                21,
                25
            );
            network.allocations_created_21_25 = true;
            writeArtifact(network, terra.config.chainID);
            await delay(1000);
        }

        // Update Owner to multiSig
        if (network.multisigAddress) {
            console.log("Updating Owner to multiSig");
            // TransferOwnership : TX
            let tx = await executeContract(
                terra,
                wallet,
                network.builderUnlockAddress,
                {
                    propose_new_owner: {
                        new_owner: network.multisigAddress,
                        expires_in: 86400 * 7,
                    },
                },
                [],
                `ASTRO Unlocking :: Propose new owner`
            );

            console.log(
                `Created proposal to change an owner of ASTRO Unlocking contract, \n Tx hash --> ${tx.txhash} \n`
            );
        }

        console.log("FINISH");
    }

    // Helper function to create allocations
    async function create_allocations(
        terra: LocalTerra | LCDClient,
        wallet: Wallet,
        astro_token_address: string,
        vesting_address: string,
        allocations: any,
        from: number,
        till: number
    ) {
        let astro_to_transfer = 0;
        let allocations_to_create = [];

        for (let i = from; i <= till; i++) {
            astro_to_transfer += Number(allocations[i][1]["amount"]);
            allocations_to_create.push(allocations[i]);
        }

        console.log(`from ${from} to ${till}:  ${astro_to_transfer / 1000000} ASTRO to transfer.`);

        // Create allocations : TX
        let tx = await executeContract(
            terra,
            wallet,
            astro_token_address,
            {
                send: {
                    contract: vesting_address,
                    amount: String(astro_to_transfer),
                    msg: Buffer.from(
                        JSON.stringify({
                            create_allocations: {
                                allocations: allocations_to_create,
                            },
                        })
                    ).toString("base64"),
                },
            },
            [],
            `Setting ASTRO unlock schedules`
        );

        console.log(
            `Creating ASTRO Unlocking schedules ::: ${from} - ${till}, ASTRO sent : ${
                astro_to_transfer / 1000000
            }, \n Tx hash --> ${tx.txhash} \n`
        );
    }
}

function delay(ms: number) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

main().catch(console.log);

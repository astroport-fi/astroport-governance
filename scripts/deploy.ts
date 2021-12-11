import "dotenv/config";
import {
  Key,
  LegacyAminoMultisigPublicKey,
  MsgExecuteContract,
  SimplePublicKey,
  LCDClient,
  MsgUpdateContractAdmin,
  LocalTerra,
  Wallet,
} from "@terra-money/terra.js";
import {
  deployContract,
  executeContract,
  newClient,
  readArtifact,
  writeArtifact,
  uploadContract,
  performTransaction,
  instantiateContract,
  Client,
} from "./helpers.js";
import { join } from "path";
import { writeFileSync } from "fs";

const ARTIFACTS_PATH = "../artifacts";

const START_TIME = 1639440000;
const CLIFF = 31536000;
const UNLOCK_DURATION = 94608000;

async function main() {
  // terra, wallet
  const { terra, wallet } = newClient();
  console.log(
    `chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`
  );

  // network : stores contract addresses
  let network = readArtifact(terra.config.chainID);
  console.log("network:", network);

  // ASTRO Token addresss should be set
  if (terra.config.chainID == "columbus-5" && !network.astro_token_address) {
    console.log(
      `Please deploy the CW20-base ASTRO token, and then set this address in the deploy config before running this script...`
    );
    return;
  }

  /*************************************** VESTING ::: DEPOYMENT AND INITIALIZATION  *****************************************/

  if (terra.config.chainID == "columbus-5") {
    // Multisig details:
    const MULTI_SIG_ADDRESS = "terra1c7m6j8ya58a2fkkptn8fgudx8sqjqvc8azq0ex";

    // Deploy dummy ASTRO token for testing on bombay-12
    // if (terra.config.chainID == "bombay-12" && !network.astro_token_address) {
    //   // CW20 TOKEN CODE ID
    //   if (!network.cw20_token_code_id) {
    //     network.cw20_token_code_id = await uploadContract(
    //       terra,
    //       wallet,
    //       join(ARTIFACTS_PATH, "cw20_token.wasm")
    //     );
    //     console.log(`Cw20 Code id = ${network.cw20_token_code_id}`);
    //     writeArtifact(network, terra.config.chainID);
    //   }
    //   // ASTRO Token for testing
    //   network.astro_token_address = await instantiateContract(
    //     terra,
    //     wallet,
    //     network.cw20_token_code_id,
    //     {
    //       name: "Astroport",
    //       symbol: "ASTRO",
    //       decimals: 6,
    //       initial_balances: [
    //         {
    //           address: wallet.key.accAddress,
    //           amount: String(1_000_000_000_000000),
    //         },
    //       ],
    //       mint: {
    //         minter: wallet.key.accAddress,
    //         cap: String(1_000_000_000_000000),
    //       },
    //     },
    //     "ASTRO Token for testing"
    //   );
    //   console.log(
    //     `ASTRO Token deployed successfully, address : ${network.astro_token_address}`
    //   );
    //   writeArtifact(network, terra.config.chainID);
    // }

    // VESTING CONTRACT ::: DEPLOYMENT
    if (!network.vesting_address) {
      console.log(`${terra.config.chainID} :: Deploying Unlocking Contract`);
      let instantiate_msg = {
        owner: wallet.key.accAddress,
        astro_token: network.astro_token_address,
      };
      console.log(instantiate_msg);
      // deploy vesting contract
      network.vesting_address = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, "astro_vesting.wasm"),
        instantiate_msg,
        "ASTROPORT -::- Unlocking Contract"
      );
      writeArtifact(network, terra.config.chainID);
      console.log(
        `${terra.config.chainID} :: Unlocking Contract Address : ${network.vesting_address} \n`
      );
    }

    // ALLOCATIONS
    let allocations: Array<
      [string, { amount: String; unlock_schedule: any; proposed_receiver: any }]
    >;

    // ALLOCATIONS DETAILS
    allocations = [
      [
        "terra1nj7umezl9xdqrsd5n0hzcct0kwadkuc726xpdt",
        {
          amount: String(91_230_000_000000),
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
        "terra1edl99rx5nfjq26mmydck4nr5w6enf3wwttrqn8",
        {
          amount: String(22_000_000_000000),
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
          amount: String(5_000_000_000000),
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
          amount: String(2_500_000_000000),
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
          amount: String(2_300_000_000000),
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
          amount: String(1_100_000_000000),
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
        "terra1svlx2775tg2dlfwkpcvu49q4y4xgefp3ftyk0z",
        {
          amount: String(350_000_000000),
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
        "terra1frrme65c3rxngyry6j44ahwusha6mkkxefu0tr",
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
    ];

    // Create allocations tx : 0-5
    if (!network.allocations_created_0_5) {
      await create_allocations(
        terra,
        wallet,
        network.astro_token_address,
        network.vesting_address,
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
      await create_allocations(
        terra,
        wallet,
        network.astro_token_address,
        network.vesting_address,
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
      await create_allocations(
        terra,
        wallet,
        network.astro_token_address,
        network.vesting_address,
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
      await create_allocations(
        terra,
        wallet,
        network.astro_token_address,
        network.vesting_address,
        allocations,
        16,
        20
      );
      network.allocations_created_16_20 = true;
      writeArtifact(network, terra.config.chainID);
      await delay(1000);
    }

    // Create allocations tx : 21-26
    if (!network.allocations_created_21_26) {
      await create_allocations(
        terra,
        wallet,
        network.astro_token_address,
        network.vesting_address,
        allocations,
        21,
        26
      );
      network.allocations_created_21_26 = true;
      writeArtifact(network, terra.config.chainID);
      await delay(1000);
    }

    // Update Owner to multiSig
    if (MULTI_SIG_ADDRESS) {
      // TransferOwnership : TX
      let tx = await executeContract(
        terra,
        wallet,
        network.vesting_address,
        {
          transfer_ownership: {
            new_owner: MULTI_SIG_ADDRESS,
          },
        },
        [],
        `ASTRO Unlocking :: Update Owner`
      );

      console.log(
        `Updated owner of ASTRO Unlocking contract, \n Tx hash --> ${tx.txhash} \n`
      );
    }

    // Update Contract admin to multiSig
    if (MULTI_SIG_ADDRESS) {
      let update_admin = new MsgUpdateContractAdmin(
        wallet.key.accAddress,
        MULTI_SIG_ADDRESS,
        network.vesting_address
      );
      // TransferOwnership : TX
      let tx = await performTransaction(terra, wallet, update_admin);

      console.log(
        `Updated ownership of ASTRO Unlocking contract, \n Tx hash --> ${tx.txhash} \n`
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
      `Setting ASTRO Unlocking schedules`
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

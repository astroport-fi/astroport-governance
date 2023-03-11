import {
  isTxError,
  LCDClient,
  LocalTerra,
  MnemonicKey,
  Msg,
  MsgExecuteContract,
  MsgInstantiateContract,
  MsgMigrateContract,
  MsgStoreCode,
  Wallet,
  PublicKey,
  Tx,
} from "@terra-money/terra.js";
import { readFileSync, writeFileSync } from "fs";
import path from "path";
import { CustomError } from 'ts-custom-error'

export const ARTIFACTS_PATH = "../artifacts";

export function readArtifact(name: string = 'artifact', from: string = ARTIFACTS_PATH,) {
  try {
    const data = readFileSync(path.join(from, `${name}.json`), 'utf8')
    return JSON.parse(data)
  } catch (e) {
    return {}
  }
}

export interface Client {
  wallet: Wallet;
  terra: LCDClient | LocalTerra;
  MULTI_SIG_TO_USE: String;
}

// Creates `Client` instance with `terra` and `wallet` to be used for interacting with terra
export function newClient(): Client {
  const client = <Client>{};
  if (process.env.WALLET) {
    client.terra = new LCDClient({
      URL: String(process.env.LCD_CLIENT_URL),
      chainID: String(process.env.CHAIN_ID),
    });

    client.wallet = recover(client.terra, process.env.WALLET);
  } else {
    client.terra = new LocalTerra();
    client.wallet = (client.terra as LocalTerra).wallets.test1;
  }
  return client;
}

export function writeArtifact(data: object, name: string = "artifact") {
  writeFileSync(
    path.join(ARTIFACTS_PATH, `${name}.json`),
    JSON.stringify(data, null, 2)
  );
}

// Tequila lcd is load balanced, so txs can't be sent too fast, otherwise account sequence queries
// may resolve an older state depending on which lcd you end up with. Generally 1000 ms is enough
// for all nodes to sync up.
let TIMEOUT = 1000;

export function setTimeoutDuration(t: number) {
  TIMEOUT = t;
}

export function getTimeoutDuration() {
  return TIMEOUT;
}

export class TransactionError extends CustomError {
  public constructor(
      public code: string | number,
      public codespace: string | undefined,
      public rawLog: string,
  ) {
    super("transaction failed")
  }
}

export async function sleep(timeout: number) {
  await new Promise(resolve => setTimeout(resolve, timeout))
}

export async function createTransaction(wallet: Wallet, msg: Msg) {
  return await wallet.createAndSignTx({ msgs: [msg]})
}

export async function broadcastTransaction(terra: LCDClient, signedTx: Tx) {
  const result = await terra.tx.broadcast(signedTx)
  await sleep(TIMEOUT)
  return result
}

export async function performTransaction(terra: LCDClient, wallet: Wallet, msg: Msg) {
  const signedTx = await createTransaction(wallet, msg)
  const result = await broadcastTransaction(terra, signedTx)
  if (isTxError(result)) {
    throw new TransactionError(result.code, result.codespace, result.raw_log)
  }
  return result
}

export async function uploadContract(
  terra: LocalTerra | LCDClient,
  wallet: Wallet,
  filepath: string
) {
  const contract = readFileSync(filepath, "base64");
  const uploadMsg = new MsgStoreCode(wallet.key.accAddress, contract);
  let result = await performTransaction(terra, wallet, uploadMsg);
  return Number(result.logs[0].eventsByType.store_code.code_id[0]); // code_id
}

export async function instantiateContract(terra: LCDClient, wallet: Wallet, admin_address: string | undefined, codeId: number, msg: object, label?: string) {
const instantiateMsg = new MsgInstantiateContract(wallet.key.accAddress, admin_address, codeId, msg, undefined, label);
let result = await performTransaction(terra, wallet, instantiateMsg)
return result.logs[0].events.filter(el => el.type == 'instantiate').map(x => x.attributes.filter(element => element.key == '_contract_address' ).map(x => x.value))[0][0];
}

export async function executeContract(
  terra: LocalTerra | LCDClient,
  wallet: Wallet,
  contractAddress: string,
  msg: object,
  coins?: any
) {
  const executeMsg = new MsgExecuteContract(
    wallet.key.accAddress,
    contractAddress,
    msg,
    coins
  );
  return await performTransaction(terra, wallet, executeMsg);
}

// Returns a TX object
export async function executeContractJsonForMultiSig(
  terra: LocalTerra | LCDClient,
  multisigAddress: string,
  sequence_number: number,
  pub_key: PublicKey | null,
  contract_address: string,
  execute_msg: any,
  memo?: string
) {
  const tx = await terra.tx.create(
    [
      {
        address: multisigAddress,
        sequenceNumber: sequence_number,
        publicKey: pub_key,
      },
    ],
    {
      msgs: [
        new MsgExecuteContract(multisigAddress, contract_address, execute_msg),
      ],
      memo: memo,
    }
  );
  return tx;
}

export async function queryContract(
  terra: LocalTerra | LCDClient,
  contractAddress: string,
  query: object
): Promise<any> {
  return await terra.wasm.contractQuery(contractAddress, query);
}

export async function deployContract(
  terra: LocalTerra | LCDClient,
  wallet: Wallet,
  admin_address: string | undefined,
  filepath: string,
  initMsg: object,
  label?: string
) {
  const codeId = await uploadContract(terra, wallet, filepath);
  return await instantiateContract(terra, wallet, admin_address, codeId, initMsg, label);
}

export async function migrate(
  terra: LocalTerra | LCDClient,
  wallet: Wallet,
  contractAddress: string,
  newCodeId: number
) {
  const migrateMsg = new MsgMigrateContract(
    wallet.key.accAddress,
    contractAddress,
    newCodeId,
    {}
  );
  return await performTransaction(terra, wallet, migrateMsg);
}

export function recover(terra: LocalTerra | LCDClient, mnemonic: string) {
  const mk = new MnemonicKey({ mnemonic: mnemonic });
  return terra.wallet(mk);
}

export function initialize(terra: LCDClient) {
  const mk = new MnemonicKey();

  console.log(`Account Address: ${mk.accAddress}`);
  console.log(`MnemonicKey: ${mk.mnemonic}`);

  return terra.wallet(mk);
}

export async function transferCW20Tokens(
  terra: LCDClient,
  wallet: Wallet,
  tokenAddress: string,
  recipient: string,
  amount: number
) {
  let transfer_msg = {
    transfer: { recipient: recipient, amount: amount.toString() },
  };
  let resp = await executeContract(terra, wallet, tokenAddress, transfer_msg);
}

export async function getCW20Balance(
  terra: LocalTerra | LCDClient,
  token_addr: string,
  user_address: string
) {
  let curBalance = await terra.wasm.contractQuery<{ balance: string }>(
    token_addr,
    { balance: { address: user_address } }
  );
  return curBalance.balance;
}

export function toEncodedBinary(object: any) {
  return Buffer.from(JSON.stringify(object)).toString("base64");
}

// Returns the `pool_address` and `lp_token_address` for a terraswap pool that's created
export function extract_terraswap_pool_info(response: any) {
  let pool_address = "";
  let lp_token_address = "";
  if (response.height > 0) {
    let events_array = JSON.parse(response["raw_log"])[0]["events"];
    let attributes = events_array[1]["attributes"];
    for (let i = 0; i < attributes.length; i++) {
      // console.log(attributes[i]);
      if (attributes[i]["key"] == "liquidity_token_addr") {
        lp_token_address = attributes[i]["value"];
      }
      if (attributes[i]["key"] == "pair_contract_addr") {
        pool_address = attributes[i]["value"];
      }
    }
  }

  return { pool_address: pool_address, lp_token_address: lp_token_address };
}

// Returns the `pool_address` and `lp_token_address` of the Astroport pool that's created
export function extract_astroport_pool_info(response: any) {
  let pool_address = "";
  let lp_token_address = "";
  if (response.height > 0) {
    let events_array = JSON.parse(response["raw_log"])[0]["events"];
    let attributes = events_array[1]["attributes"];
    for (let i = 0; i < attributes.length; i++) {
      // console.log(attributes[i]);
      if (attributes[i]["key"] == "liquidity_token_addr") {
        lp_token_address = attributes[i]["value"];
      }
      if (attributes[i]["key"] == "pair_contract_addr") {
        pool_address = attributes[i]["value"];
      }
    }
  }

  return { pool_address: pool_address, lp_token_address: lp_token_address };
}

export function delay(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

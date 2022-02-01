## Scripts to deploy the Astroport Builder Unlock contract

### Build env local

```shell
npm install
npm start
```

### Deploy on `testnet`

Create `.env`:

```shell
WALLET="mnemonic"
LCD_CLIENT_URL=https://bombay-lcd.terra.dev
CHAIN_ID=bombay-12
node --loader ts-node/esm deploy.ts
```

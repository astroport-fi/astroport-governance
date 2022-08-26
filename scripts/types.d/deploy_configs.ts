import {readArtifact} from "../helpers.js";

let chainConfigs = readArtifact(`${process.env.CHAIN_ID}`, 'deploy_configs');

export const deployConfigs: Config = {
    teamUnlock: chainConfigs.teamUnlock,
    assembly: chainConfigs.assembly,
    generalInfo: chainConfigs.generalInfo,
    votingEscrow: chainConfigs.votingEscrow,
    feeDistributor: chainConfigs.feeDistributor,
    generatorController: chainConfigs.generatorController,
    votingEscrowDelegation: chainConfigs.votingEscrowDelegation,
}
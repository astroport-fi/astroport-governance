interface GeneralInfo {
    multisig: string,
    astro_token: string,
    xastro_token: string,
    factory_addr: string,
    generator_addr: string,
}

type Marketing = {
    project: string,
    description: string,
    marketing: string,
    logo: {
        url: string
    }
}

type Allocation = {
    amount: string,
    unlock_schedule: {
        start_time: number,
        cliff: number,
        duration: number,
    },
    proposed_receiver: string,
}

type Allocations = [
    string,
    Allocation
]

interface TeamUnlock {
    admin: string,
    initMsg: {
        owner: string,
        astro_token: string,
        max_allocations_amount: string
    },
    label: string,
    change_owner: boolean,
    propose_new_owner: {
        owner: string,
        expires_in: number
    },
    setup_allocations: {
        allocations: {
            create_allocations: {
                allocations: Allocations[]
            }
        },
        memo: string
    }
}

interface Assembly {
    admin: string,
    initMsg: {
        xastro_token_addr: string,
        builder_unlock_addr: string,
        proposal_voting_period: number,
        proposal_effective_delay: number,
        proposal_expiration_period: number,
        proposal_required_deposit: string,
        proposal_required_quorum: string,
        proposal_required_threshold: string,
        whitelisted_links: string[]
    },
    label: string
}

interface VotingEscrow {
    admin: string,
    initMsg: {
        owner: string,
        guardian_addr?: string,
        deposit_token_addr: string,
        marketing: Marketing,
        logo_urls_whitelist: string[]
    },
    label: string,
}

interface FeeDistributor {
    admin: string,
    initMsg: {
        owner: string,
        astro_token: string,
        voting_escrow_addr: string,
        claim_many_limit?: number,
        is_claim_disabled?: boolean
    },
    label: string,
}

interface GeneratorController {
    admin: string,
    initMsg: {
        owner: string,
        escrow_addr: string,
        generator_addr: string,
        factory_addr: string,
        pools_limit: number,
    },
    label: string
}

interface VotingEscrowDelegation {
    admin: string,
    initMsg: {
        owner: string,
        voting_escrow_addr: string,
        nft_code_id: number
    },
    label: string
}

interface Config {
    teamUnlock: TeamUnlock,
    assembly: Assembly,
    votingEscrow: VotingEscrow,
    feeDistributor: FeeDistributor,
    generatorController: GeneratorController,
    votingEscrowDelegation: VotingEscrowDelegation,
    generalInfo: GeneralInfo
}
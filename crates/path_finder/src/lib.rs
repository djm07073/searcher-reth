use alloy_sol_types::SolValue;
use eyre::Error;
use reth_provider::{ BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider };
use reth_revm::{
    context::{ BlockEnv, CfgEnv, Evm, TxEnv },
    database::StateProviderDatabase,
    db::CacheDB,
    handler::{ instructions::EthInstructions, EthPrecompiles },
    interpreter::interpreter::EthInterpreter,
    primitives::{ address, Address },
    state::{ AccountInfo, Bytecode },
    Context,
    MainBuilder,
    MainContext,
    SystemCallEvm,
};

const DEPLOYED_ADDRESS: Address = address!("0000000000000000000000000000000000012345");

type PathFinderCtx<'a, DB> = Context<
    BlockEnv,
    TxEnv,
    CfgEnv,
    CacheDB<StateProviderDatabase<LatestStateProviderRef<'a, DB>>>
>;

pub struct PathFinder<'a, DB> where DB: DBProvider + BlockHashReader + StateCommitmentProvider {
    evm: Evm<
        PathFinderCtx<'a, DB>,
        (),
        EthInstructions<EthInterpreter, PathFinderCtx<'a, DB>>,
        EthPrecompiles
    >,
}

impl<'a, DB> PathFinder<'a, DB> where DB: DBProvider + BlockHashReader + StateCommitmentProvider {
    /// Creates a new instance of the PathFinder
    pub fn new(provider: LatestStateProviderRef<'a, DB>, contract: Bytecode) -> Self {
        let mut db = CacheDB::new(StateProviderDatabase::new(provider));
        db.insert_account_info(DEPLOYED_ADDRESS, AccountInfo {
            code_hash: contract.hash_slow(),
            code: Some(contract),
            ..Default::default()
        });

        let evm = Context::mainnet().with_db(db).build_mainnet();

        Self { evm }
    }

    // DFS-based search for arbitrage paths with dynamic pruning.
    // Keeps only the most profitable path for each start token.
    // Get the top 10 paths over min_profit and

    // Main logic:
    // 1. For each token, start a DFS to explore swap paths up to a given hop limit.
    // 2. Simulate swap outcomes and calculate cumulative profit ratio.
    // 3. If a newly found path has higher profit than the previously best one for the same start token, replace it.
    // 4. Discard any path with lower profit than the current best.

    // Path pruning rules:
    // - Avoid cycles (do not revisit same token in a path).
    // - Stop expanding a path if:
    //     - The cumulative profit is below a defined threshold.
    //     - The current profit is less than or equal to the best known for the same start token.
    // - Store only the most profitable path per start token.
    pub fn find_optimal_paths(
        &mut self,
        route_paths: Vec<RoutePath>,
        max_profit: u64,
        min_profit: u64
    ) -> Result<Vec<RoutePath>, Error> {
        let mut optimal_paths = Vec::<RoutePath>::new();
        // get native token price. ex. BERA/USDC
        while let Some(route_path) = route_paths.iter().next() {
            let result = self.evm
                .transact_system_call(route_path.abi_encode().into(), DEPLOYED_ADDRESS)
                .unwrap();
            // extract gas used and output amount
            let gas_used = result.result.gas_used();
            let net_profit = result.result.output().unwrap();
            // calculate net profit
            // net profit > max_profit
            todo!();
        }

        // find paths of min ~ max with using "Incremental Evaluation"
        // stateless transition in evm
        todo!();
        Ok(optimal_paths)
    }
}

// A -> B -> A
// A -> B -> C -> A
use itertools::{ Either, Itertools };
use searcher_reth_types::{ Hop, Priority, RoutePath }; // Add this to dependencies

pub fn get_cycle_route_paths(
    dexs: Vec<(Address, u8)>,
    tokens: Vec<(Address, Priority)>
) -> Vec<RoutePath> {
    let mut route_paths = Vec::new();

    let (beginning_tokens, other_tokens): (Vec<Address>, Vec<Address>) = tokens
        .iter()
        .partition_map(|(addr, p)| {
            if *p == Priority::Beginning { Either::Left(*addr) } else { Either::Right(*addr) }
        });

    // Case 1: A -> B -> A (2-hop paths)
    for start_token in &beginning_tokens {
        for inter_token in &other_tokens {
            for dex_hops in dexs.iter().permutations(2) {
                let route = vec![
                    // A -> B
                    Hop {
                        dexType: dex_hops[0].1,
                        dex: dex_hops[0].0,
                        srcToken: *start_token,
                        dstToken: *inter_token,
                    },
                    // B -> A
                    Hop {
                        dexType: dex_hops[1].1,
                        dex: dex_hops[1].0,
                        srcToken: *inter_token,
                        dstToken: *start_token,
                    }
                ];
                route_paths.push(route);
            }
        }
    }

    // Case 2: A -> B -> C -> A (3-hop paths)
    for start_token in &beginning_tokens {
        for inter_token_pair in other_tokens.iter().combinations(2) {
            for dex_hops in dexs.iter().permutations(3) {
                let route = vec![
                    // A -> B
                    Hop {
                        dexType: dex_hops[0].1,
                        dex: dex_hops[0].0,
                        srcToken: *start_token,
                        dstToken: *inter_token_pair[0],
                    },
                    // B -> C
                    Hop {
                        dexType: dex_hops[1].1,
                        dex: dex_hops[1].0,
                        srcToken: *inter_token_pair[0],
                        dstToken: *inter_token_pair[1],
                    },
                    // C -> A
                    Hop {
                        dexType: dex_hops[2].1,
                        dex: dex_hops[2].0,
                        srcToken: *inter_token_pair[1],
                        dstToken: *start_token,
                    }
                ];
                route_paths.push(route);
            }
        }
    }

    route_paths
}

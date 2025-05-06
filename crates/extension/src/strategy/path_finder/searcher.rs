use alloy_sol_types::SolValue;
use eyre::Error;

use reth_provider::{ BlockHashReader, DBProvider, StateCommitmentProvider };
use reth_revm::SystemCallEvm;

use crate::strategy::path_finder::types::{ Profit, DEPLOYED_ADDRESS };

use super::{ types::RoutePath, PathFinder };

pub trait Strategy {
    fn filter_candidates(
        &mut self,
        candidates: Vec<RoutePath>,
        max_profit: u64,
        min_profit: u64
    ) -> Result<Vec<RoutePath>, Error>;
}

impl<'a, DB> Strategy
    for PathFinder<'a, DB>
    where DB: DBProvider + BlockHashReader + StateCommitmentProvider
{
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
    fn filter_candidates(
        &mut self,
        route_paths: Vec<RoutePath>,
        max_profit: u64,
        min_profit: u64
    ) -> Result<Vec<RoutePath>, Error> {
        let mut optimal_paths = Vec::<RoutePath>::new();
        // TODO: use parallel core
        // get native token price. ex. BERA/USDC
        while let Some(route_path) = route_paths.iter().next() {
            let result = self.evm
                .transact_system_call(route_path.abi_encode().into(), DEPLOYED_ADDRESS)
                .unwrap();
            // amount
            let Profit { amount } = Profit::abi_decode(result.result.output().unwrap())?;

            let net_profit = amount;
            // if max profit / 1e6 < net_profit,  push optimal_paths.push(route_path) and break
            // else if min profit / 1e6 < net_profit, push optimal_paths.push(route_path);
            // else continue
            todo!();
        }

        // find paths of min ~ max with using "Incremental Evaluation"
        // stateless transition in evm
        Ok(optimal_paths)
    }
}

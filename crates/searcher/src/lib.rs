type DexType = uint64;
pub(crate) type RoutePath = (DexType, Address, Address, Address); // dex type, swap_router, src token ,dst token

#[derive(Debug)]
pub(crate) struct Searcher<'a, DB: StateProvider> {
    evm: Evm<'a, (), State<DB>>,
}

impl Searcher {
    /// Creates a new instance of the Searcher
    pub(crate) fn new(
        config: &'a EthEvmConfig,
        db: ShadowDatabase<DB>,
        chain: Arc<ChainSpec>,
        simualte_contract: Bytecode,
        header: &Header
    ) -> Self {
        let evm = configure_evm(config, db, chain, simualte_contract, header);
        Self { evm }
    }

    // DFS-based search for arbitrage paths with dynamic pruning.
    // Keeps only the most profitable path for each start token.

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
    pub(crate) fn find_optimal_execution(
        &mut self,
        route_paths: Vec<RoutePath>,
        max_profit: u64,
        min_profit: u64
    ) -> Result<Vec<RoutePath>> {
        // find paths of min ~ max with using "Incremental Evaluation"
        // stateless transition in evm
        todo!()
    }
}

/// Configure EVM with the given database, header, precompile contract.
fn configure_evm<'a, DB: StateProvider>(
    config: &'a EthEvmConfig,
    db: ShadowDatabase<DB>,
    chain: Arc<ChainSpec>,
    simualte_contract: Bytecode,
    header: &Header
) -> Evm<'a, (), State<ShadowDatabase<DB>>> {
    let mut evm = config.evm(StateBuilder::new_with_database(db).with_bundle_update().build());
    let mut cfg = CfgEnvWithHandlerCfg::new_with_spec_id(evm.cfg().clone(), evm.spec_id());
    EthEvmConfig::fill_cfg_and_block_env(&mut cfg, evm.block_mut(), &chain, header, U256::ZERO);
    // TODO: set simulate contract in precompile contracts 
    *evm.cfg_mut() = cfg.cfg_env;

    evm
}

pub mod candidate;
pub mod strategy;
pub mod types;

use reth_provider::{BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider};
use reth_revm::{
    Context, MainBuilder, MainContext,
    context::{BlockEnv, CfgEnv, Evm, TxEnv},
    database::StateProviderDatabase,
    db::CacheDB,
    handler::{EthPrecompiles, instructions::EthInstructions},
    interpreter::interpreter::EthInterpreter,
    state::{AccountInfo, Bytecode},
};
use types::STRATEGY_CONTRACT_ADDRESS;

type PathFinderCtx<'a, DB> = Context<
    BlockEnv,
    TxEnv,
    CfgEnv,
    CacheDB<StateProviderDatabase<LatestStateProviderRef<'a, DB>>>,
>;

pub struct PathFinder<'a, DB>
where
    DB: DBProvider + BlockHashReader + StateCommitmentProvider,
{
    evm: Evm<
        PathFinderCtx<'a, DB>,
        (),
        EthInstructions<EthInterpreter, PathFinderCtx<'a, DB>>,
        EthPrecompiles,
    >,
}

impl<'a, DB> PathFinder<'a, DB>
where
    DB: DBProvider + BlockHashReader + StateCommitmentProvider,
{
    /// Creates a new instance of the PathFinder
    pub fn new(provider: LatestStateProviderRef<'a, DB>, contract: Bytecode) -> Self {
        let mut db = CacheDB::new(StateProviderDatabase::new(provider));
        db.insert_account_info(
            STRATEGY_CONTRACT_ADDRESS,
            AccountInfo {
                code_hash: contract.hash_slow(),
                code: Some(contract),
                ..Default::default()
            },
        );
        let evm = Context::mainnet().with_db(db).build_mainnet();
        Self { evm }
    }
}

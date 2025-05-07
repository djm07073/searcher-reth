pub mod strategy;
pub mod types;
pub mod candidate;

use reth_provider::{ BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider };
use reth_revm::{
    context::{ BlockEnv, CfgEnv, Evm, TxEnv },
    database::StateProviderDatabase,
    db::CacheDB,
    handler::{ instructions::EthInstructions, EthPrecompiles },
    interpreter::interpreter::EthInterpreter,
    state::{ AccountInfo, Bytecode },
    Context,
    MainBuilder,
    MainContext,
};
use types::DEPLOYED_ADDRESS;

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
}

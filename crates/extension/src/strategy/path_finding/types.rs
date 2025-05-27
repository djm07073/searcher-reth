use alloy_primitives::{ Address, address };
use alloy_sol_types::sol;
use searcher_reth_repository::entity::hop::Model as HopModel;
pub(super) const STRATEGY_CONTRACT_ADDRESS: Address = address!(
    "0000000000000000000000000000000000012345"
);

sol! {
    #[derive(Debug)]
    struct Hop {
        uint8 dexType;
        address dex;
        address srcToken;
        address dstToken;
        bytes metadata; // Additional metadata for the hop ex. Balancer
    }

    function getProfit(uint256 initialAmt, Hop[] memory route) view external returns (uint256 profit);

    function execute(
        Hop[][] memory routes
    ) external returns (uint256 profit);
}

pub type RoutePath = Vec<Hop>;

impl From<HopModel> for Hop {
    fn from(hop: HopModel) -> Self {
        Self {
            dexType: hop.dex_type as u8,
            dex: hop.address.parse::<Address>().unwrap(),
            srcToken: hop.src_token.parse::<Address>().unwrap(),
            dstToken: hop.dst_token.parse::<Address>().unwrap(),
            metadata: hop.metadata.into_bytes().into(),
        }
    }
}

use alloy_primitives::{Address, address};
use alloy_sol_types::sol;

pub(super) const STRATEGY_CONTRACT_ADDRESS: Address =
    address!("0000000000000000000000000000000000012345");

sol! {
    #[derive(Debug)]
    struct Hop {
        uint8 dexType;
        address dex;
        address srcToken;
        address dstToken;
    }

    function getProfit(uint256 initialAmt, Hop[] memory route) view external returns (uint256 profit);

    function execute(
        Hop[][] memory routes,
    ) external returns (uint256 profit);
}

pub(crate) type RoutePath = Vec<Hop>;

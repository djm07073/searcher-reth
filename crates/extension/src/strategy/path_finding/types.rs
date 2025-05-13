use alloy_primitives::{ address, Address };
use alloy_sol_types::sol;

pub(super) const DEPLOYED_ADDRESS: Address = address!("0000000000000000000000000000000000012345");

sol! {
    #[derive(Debug)]
    struct Hop {
        uint8 dexType;
        address dex;
        address srcToken;
        address dstToken;
    }

    function getProfit(uint256 initialAmt, Hop[] memory route) external returns (uint256 profit); 

}

pub(crate) type RoutePath = Vec<Hop>;

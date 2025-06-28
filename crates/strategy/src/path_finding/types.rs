use alloy_sol_types::sol;

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

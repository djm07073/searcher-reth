use alloy_sol_types::sol;

sol! {
    #[derive(Debug)]
    struct Hop {
        uint8 dexType;
        address router;
        address quoter;
        address srcToken;
        address dstToken;
        bytes metadata; // Additional metadata for the hop ex. Balancer pool id or Uniswap v3 fee
    }

    // Calculate the profit for a given route
    // quoterCalldata: []QuoterHop
    #[derive(Debug)]
    function getProfit(uint256 amount, bytes memory quoterCalldata) view external returns (uint256 profit);
    
    // Execute a route and return the profit
    #[derive(Debug)]
    function execute(
        uint256[] memory amounts,
        bytes[] memory executorCalldata // [][]RouterHop
    ) external returns (uint256 profit);
}

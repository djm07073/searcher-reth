use alloy_sol_types::sol;

sol! {
    #[derive(Debug)]
    struct LiquidationPayload {
    uint8 lendingType;          // Protocol type: 1 = AAVE V3, 2+ = future protocols
    uint256 liquidateAmount;     // Amount of debt to repay (e.g., 1000 ETH)
    address liquidateToken;      // Debt token address to repay (e.g., WETH, DAI)
    address collateralToken;     // Collateral token to receive (e.g., WBTC, USDC)
    address lendingProtocol;     // Protocol contract address (e.g., AAVE Pool)
    address user;                // User address to liquidate (must have HF < 1)
    }

    function getProfit(
        uint256 initialAmt,
        address stableToken,  // USDC or other stable token address
        LiquidationPayload memory payload
    ) external view returns (uint256 finalAmount);

     #[derive(Debug)]
    struct LiquidationPayloads {
        uint256 amount;
        LiquidationPayload payload;
        address swapRouterBefore;
        address swapRouterAfter;
        bytes swapDataBefore;
        bytes swapDataAfter;
    }

    function execute(
        address stableToken,
        LiquidationPayloads[] memory liquidations
    ) external returns (uint256 totalProfit);
}

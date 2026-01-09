// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {IERC20} from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts/token/ERC20/utils/SafeERC20.sol";

import {IForwarder} from "../interfaces/IForwarder.sol";

/// @notice Interface for Uniswap V3 SwapRouter02
interface ISwapRouter02 {
    struct ExactInputSingleParams {
        address tokenIn;
        address tokenOut;
        uint24 fee;
        address recipient;
        uint256 amountIn;
        uint256 amountOutMinimum;
        uint160 sqrtPriceLimitX96;
    }

    struct ExactOutputSingleParams {
        address tokenIn;
        address tokenOut;
        uint24 fee;
        address recipient;
        uint256 amountOut;
        uint256 amountInMaximum;
        uint160 sqrtPriceLimitX96;
    }

    function exactInputSingle(ExactInputSingleParams calldata params) external payable returns (uint256 amountOut);

    function exactOutputSingle(ExactOutputSingleParams calldata params) external payable returns (uint256 amountIn);
}

/// @title UniswapV3Forwarder
/// @notice Forwarder contract for executing Uniswap V3 swaps via the Protocol Adapter
/// @dev Enables shielded swaps by allowing the Protocol Adapter to execute trades
contract UniswapV3Forwarder is IForwarder {
    using SafeERC20 for IERC20;

    /// @notice The Uniswap V3 SwapRouter02 address
    /// @dev Sepolia: 0x3bFA4769FB09eefC5a80d6E87c3B9C650f7Ae48E
    /// @dev Mainnet: 0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45
    ISwapRouter02 public immutable swapRouter;

    /// @notice The Protocol Adapter address
    address public immutable protocolAdapter;

    /// @notice Emitted when a swap is executed
    event SwapExecuted(
        address indexed tokenIn,
        address indexed tokenOut,
        uint256 amountIn,
        uint256 amountOut,
        address recipient
    );

    /// @notice Error thrown when caller is not the Protocol Adapter
    error OnlyProtocolAdapter();

    /// @notice Error thrown when an unsupported function selector is called
    error UnsupportedSelector(bytes4 selector);

    /// @notice Error thrown when output amount is less than minimum
    error InsufficientOutputAmount(uint256 expected, uint256 actual);

    constructor(address _protocolAdapter, address _swapRouter) {
        protocolAdapter = _protocolAdapter;
        swapRouter = ISwapRouter02(_swapRouter);
    }

    /// @inheritdoc IForwarder
    /// @notice Execute a swap via Uniswap V3
    /// @param input Encoded swap parameters
    /// @return output The amount of tokens received (encoded)
    function forwardCall(
        bytes32, /* logicRef */
        bytes calldata input
    ) external override returns (bytes memory output) {
        if (msg.sender != protocolAdapter) {
            revert OnlyProtocolAdapter();
        }

        bytes4 selector = bytes4(input[:4]);

        if (selector == this.exactInputSingle.selector) {
            // Decode ExactInputSingleParams
            ISwapRouter02.ExactInputSingleParams memory params =
                abi.decode(input[4:], (ISwapRouter02.ExactInputSingleParams));

            // Approve router to spend input tokens
            IERC20(params.tokenIn).forceApprove(address(swapRouter), params.amountIn);

            // Execute swap
            uint256 amountOut = swapRouter.exactInputSingle(params);

            emit SwapExecuted(params.tokenIn, params.tokenOut, params.amountIn, amountOut, params.recipient);

            output = abi.encode(amountOut);
        } else if (selector == this.exactOutputSingle.selector) {
            // Decode ExactOutputSingleParams
            ISwapRouter02.ExactOutputSingleParams memory params =
                abi.decode(input[4:], (ISwapRouter02.ExactOutputSingleParams));

            // Approve router to spend max input tokens
            IERC20(params.tokenIn).forceApprove(address(swapRouter), params.amountInMaximum);

            // Execute swap
            uint256 amountIn = swapRouter.exactOutputSingle(params);

            // Clear any remaining approval
            IERC20(params.tokenIn).forceApprove(address(swapRouter), 0);

            emit SwapExecuted(params.tokenIn, params.tokenOut, amountIn, params.amountOut, params.recipient);

            output = abi.encode(amountIn);
        } else {
            revert UnsupportedSelector(selector);
        }
    }

    /// @notice Function selector for exactInputSingle
    /// @dev Used for encoding calls
    function exactInputSingle(ISwapRouter02.ExactInputSingleParams calldata) external pure returns (uint256) {
        // This function is only used for its selector
        revert("Use forwardCall");
    }

    /// @notice Function selector for exactOutputSingle
    /// @dev Used for encoding calls
    function exactOutputSingle(ISwapRouter02.ExactOutputSingleParams calldata) external pure returns (uint256) {
        // This function is only used for its selector
        revert("Use forwardCall");
    }

    /// @notice Rescue tokens accidentally sent to this contract
    /// @dev Only callable by Protocol Adapter owner in emergency
    function rescueTokens(address token, address to, uint256 amount) external {
        if (msg.sender != protocolAdapter) {
            revert OnlyProtocolAdapter();
        }
        IERC20(token).safeTransfer(to, amount);
    }
}

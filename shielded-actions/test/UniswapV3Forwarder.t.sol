// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Test, console} from "forge-std/Test.sol";
import {IERC20} from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import {ERC20} from "@openzeppelin-contracts/token/ERC20/ERC20.sol";

import {UniswapV3Forwarder, ISwapRouter02} from "../src/forwarders/UniswapV3Forwarder.sol";

/// @notice Mock ERC20 token for testing
contract MockERC20 is ERC20 {
    constructor(string memory name, string memory symbol) ERC20(name, symbol) {}

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

/// @notice Mock Uniswap V3 SwapRouter for testing
contract MockSwapRouter {
    // Simple mock: returns amountIn for exactInputSingle, amountOut for exactOutputSingle
    uint256 public mockAmountOut = 950 ether; // 95% for simulating slippage
    uint256 public mockAmountIn = 1050 ether; // 105% for simulating slippage

    function exactInputSingle(ISwapRouter02.ExactInputSingleParams calldata params)
        external
        returns (uint256 amountOut)
    {
        // Transfer tokens from caller
        IERC20(params.tokenIn).transferFrom(msg.sender, address(this), params.amountIn);

        // Mock: mint output tokens to recipient
        MockERC20(params.tokenOut).mint(params.recipient, mockAmountOut);

        return mockAmountOut;
    }

    function exactOutputSingle(ISwapRouter02.ExactOutputSingleParams calldata params)
        external
        returns (uint256 amountIn)
    {
        // Transfer tokens from caller (use a lower amount than max for testing)
        amountIn = params.amountInMaximum > mockAmountIn ? mockAmountIn : params.amountInMaximum;
        IERC20(params.tokenIn).transferFrom(msg.sender, address(this), amountIn);

        // Mock: mint output tokens to recipient
        MockERC20(params.tokenOut).mint(params.recipient, params.amountOut);

        return amountIn;
    }

    function setMockAmounts(uint256 _amountOut, uint256 _amountIn) external {
        mockAmountOut = _amountOut;
        mockAmountIn = _amountIn;
    }
}

contract UniswapV3ForwarderTest is Test {
    UniswapV3Forwarder public forwarder;
    MockSwapRouter public router;
    MockERC20 public tokenIn;
    MockERC20 public tokenOut;

    address public protocolAdapter = address(0x1234);
    address public recipient = address(0x5678);

    uint256 public constant INITIAL_BALANCE = 10000 ether;
    uint256 public constant SWAP_AMOUNT = 1000 ether;

    function setUp() public {
        // Deploy mock tokens
        tokenIn = new MockERC20("Token In", "TIN");
        tokenOut = new MockERC20("Token Out", "TOUT");

        // Deploy mock router
        router = new MockSwapRouter();

        // Deploy forwarder
        forwarder = new UniswapV3Forwarder(protocolAdapter, address(router));

        // Mint tokens to forwarder (simulating shielded tokens being available)
        tokenIn.mint(address(forwarder), INITIAL_BALANCE);
    }

    function test_constructor() public view {
        assertEq(forwarder.protocolAdapter(), protocolAdapter);
        assertEq(address(forwarder.swapRouter()), address(router));
    }

    function test_exactInputSingle() public {
        ISwapRouter02.ExactInputSingleParams memory params = ISwapRouter02.ExactInputSingleParams({
            tokenIn: address(tokenIn),
            tokenOut: address(tokenOut),
            fee: 3000, // 0.3%
            recipient: recipient,
            amountIn: SWAP_AMOUNT,
            amountOutMinimum: 900 ether,
            sqrtPriceLimitX96: 0
        });

        bytes memory input = abi.encodeWithSelector(forwarder.exactInputSingle.selector, params);

        vm.prank(protocolAdapter);
        bytes memory output = forwarder.forwardCall(bytes32(0), input);

        uint256 amountOut = abi.decode(output, (uint256));

        // Verify output amount
        assertEq(amountOut, router.mockAmountOut());

        // Verify recipient received tokens
        assertEq(tokenOut.balanceOf(recipient), router.mockAmountOut());

        // Verify forwarder spent tokens
        assertEq(tokenIn.balanceOf(address(forwarder)), INITIAL_BALANCE - SWAP_AMOUNT);
    }

    function test_exactOutputSingle() public {
        uint256 desiredOutput = 500 ether;
        uint256 maxInput = 600 ether;

        ISwapRouter02.ExactOutputSingleParams memory params = ISwapRouter02.ExactOutputSingleParams({
            tokenIn: address(tokenIn),
            tokenOut: address(tokenOut),
            fee: 3000,
            recipient: recipient,
            amountOut: desiredOutput,
            amountInMaximum: maxInput,
            sqrtPriceLimitX96: 0
        });

        // Set mock to use less than max
        router.setMockAmounts(desiredOutput, 550 ether);

        bytes memory input = abi.encodeWithSelector(forwarder.exactOutputSingle.selector, params);

        vm.prank(protocolAdapter);
        bytes memory output = forwarder.forwardCall(bytes32(0), input);

        uint256 amountIn = abi.decode(output, (uint256));

        // Verify amount spent
        assertEq(amountIn, 550 ether);

        // Verify recipient received exact output
        assertEq(tokenOut.balanceOf(recipient), desiredOutput);
    }

    function test_revert_notProtocolAdapter() public {
        ISwapRouter02.ExactInputSingleParams memory params = ISwapRouter02.ExactInputSingleParams({
            tokenIn: address(tokenIn),
            tokenOut: address(tokenOut),
            fee: 3000,
            recipient: recipient,
            amountIn: SWAP_AMOUNT,
            amountOutMinimum: 0,
            sqrtPriceLimitX96: 0
        });

        bytes memory input = abi.encodeWithSelector(forwarder.exactInputSingle.selector, params);

        // Call as random user should fail
        vm.prank(address(0xDEAD));
        vm.expectRevert(UniswapV3Forwarder.OnlyProtocolAdapter.selector);
        forwarder.forwardCall(bytes32(0), input);
    }

    function test_revert_unsupportedSelector() public {
        // Use random selector
        bytes memory input = abi.encodeWithSelector(bytes4(0x12345678), uint256(100));

        vm.prank(protocolAdapter);
        vm.expectRevert(
            abi.encodeWithSelector(UniswapV3Forwarder.UnsupportedSelector.selector, bytes4(0x12345678))
        );
        forwarder.forwardCall(bytes32(0), input);
    }

    function test_rescueTokens() public {
        // Rescue tokens from forwarder
        uint256 rescueAmount = 100 ether;

        vm.prank(protocolAdapter);
        forwarder.rescueTokens(address(tokenIn), recipient, rescueAmount);

        assertEq(tokenIn.balanceOf(recipient), rescueAmount);
        assertEq(tokenIn.balanceOf(address(forwarder)), INITIAL_BALANCE - rescueAmount);
    }

    function test_revert_rescueTokens_notPA() public {
        vm.prank(address(0xDEAD));
        vm.expectRevert(UniswapV3Forwarder.OnlyProtocolAdapter.selector);
        forwarder.rescueTokens(address(tokenIn), recipient, 100 ether);
    }
}

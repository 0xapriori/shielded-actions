// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Test, console} from "forge-std/Test.sol";
import {IERC20} from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import {ERC20} from "@openzeppelin-contracts/token/ERC20/ERC20.sol";

import {ERC20Forwarder} from "../src/forwarders/ERC20Forwarder.sol";

/// @notice Mock ERC20 token for testing
contract MockERC20 is ERC20 {
    constructor(string memory name, string memory symbol) ERC20(name, symbol) {}

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

contract ERC20ForwarderTest is Test {
    ERC20Forwarder public forwarder;
    MockERC20 public token;

    address public protocolAdapter = address(0x1234);
    address public user = address(0x5678);
    address public recipient = address(0x9ABC);

    uint256 public constant INITIAL_BALANCE = 1000 ether;
    uint256 public constant DEPOSIT_AMOUNT = 100 ether;

    function setUp() public {
        // Deploy mock token
        token = new MockERC20("Test Token", "TEST");

        // Deploy forwarder with protocolAdapter as owner
        forwarder = new ERC20Forwarder(protocolAdapter, address(token));

        // Mint tokens to user
        token.mint(user, INITIAL_BALANCE);

        // User approves forwarder to spend tokens
        vm.prank(user);
        token.approve(address(forwarder), type(uint256).max);
    }

    function test_constructor() public view {
        assertEq(address(forwarder.token()), address(token));
        assertEq(forwarder.protocolAdapter(), protocolAdapter);
        assertEq(forwarder.owner(), protocolAdapter);
    }

    function test_deposit_shielding() public {
        // Encode transferFrom call
        bytes memory input = abi.encodeWithSelector(
            IERC20.transferFrom.selector, user, address(forwarder), DEPOSIT_AMOUNT
        );

        // Call as protocol adapter
        vm.prank(protocolAdapter);
        bytes memory output = forwarder.forwardCall(bytes32(0), input);

        // Verify output
        assertEq(abi.decode(output, (bool)), true);

        // Verify balances
        assertEq(token.balanceOf(user), INITIAL_BALANCE - DEPOSIT_AMOUNT);
        assertEq(token.balanceOf(address(forwarder)), DEPOSIT_AMOUNT);
    }

    function test_withdraw_unshielding() public {
        // First deposit some tokens
        bytes memory depositInput = abi.encodeWithSelector(
            IERC20.transferFrom.selector, user, address(forwarder), DEPOSIT_AMOUNT
        );
        vm.prank(protocolAdapter);
        forwarder.forwardCall(bytes32(0), depositInput);

        // Now withdraw to recipient
        bytes memory withdrawInput =
            abi.encodeWithSelector(IERC20.transfer.selector, recipient, DEPOSIT_AMOUNT);

        vm.prank(protocolAdapter);
        bytes memory output = forwarder.forwardCall(bytes32(0), withdrawInput);

        // Verify output
        assertEq(abi.decode(output, (bool)), true);

        // Verify balances
        assertEq(token.balanceOf(address(forwarder)), 0);
        assertEq(token.balanceOf(recipient), DEPOSIT_AMOUNT);
    }

    function test_revert_notProtocolAdapter() public {
        bytes memory input = abi.encodeWithSelector(
            IERC20.transferFrom.selector, user, address(forwarder), DEPOSIT_AMOUNT
        );

        // Call as random user should fail
        vm.prank(user);
        vm.expectRevert(ERC20Forwarder.OnlyProtocolAdapter.selector);
        forwarder.forwardCall(bytes32(0), input);
    }

    function test_revert_invalidRecipient() public {
        // Try to deposit to wrong recipient
        bytes memory input = abi.encodeWithSelector(
            IERC20.transferFrom.selector,
            user,
            recipient, // Wrong! Should be forwarder
            DEPOSIT_AMOUNT
        );

        vm.prank(protocolAdapter);
        vm.expectRevert(
            abi.encodeWithSelector(
                ERC20Forwarder.InvalidRecipient.selector, address(forwarder), recipient
            )
        );
        forwarder.forwardCall(bytes32(0), input);
    }

    function test_revert_unsupportedSelector() public {
        // Try unsupported selector (e.g., approve)
        bytes memory input = abi.encodeWithSelector(IERC20.approve.selector, user, DEPOSIT_AMOUNT);

        vm.prank(protocolAdapter);
        vm.expectRevert(
            abi.encodeWithSelector(
                ERC20Forwarder.UnsupportedSelector.selector, IERC20.approve.selector
            )
        );
        forwarder.forwardCall(bytes32(0), input);
    }

    function test_getBalance() public {
        // Initially zero
        assertEq(forwarder.getBalance(), 0);

        // Deposit
        bytes memory input = abi.encodeWithSelector(
            IERC20.transferFrom.selector, user, address(forwarder), DEPOSIT_AMOUNT
        );
        vm.prank(protocolAdapter);
        forwarder.forwardCall(bytes32(0), input);

        // Check balance
        assertEq(forwarder.getBalance(), DEPOSIT_AMOUNT);
    }

    function testFuzz_deposit(uint256 amount) public {
        // Bound amount to reasonable values
        amount = bound(amount, 1, INITIAL_BALANCE);

        bytes memory input = abi.encodeWithSelector(
            IERC20.transferFrom.selector, user, address(forwarder), amount
        );

        vm.prank(protocolAdapter);
        bytes memory output = forwarder.forwardCall(bytes32(0), input);

        assertEq(abi.decode(output, (bool)), true);
        assertEq(token.balanceOf(address(forwarder)), amount);
    }
}

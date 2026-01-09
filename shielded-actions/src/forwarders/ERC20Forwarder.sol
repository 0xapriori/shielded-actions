// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {IERC20} from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts/token/ERC20/utils/SafeERC20.sol";
import {Ownable} from "@openzeppelin-contracts/access/Ownable.sol";

import {IForwarder} from "../interfaces/IForwarder.sol";

/// @title ERC20Forwarder
/// @notice Forwarder contract for wrapping ERC20 tokens into Anoma resources (shielding/unshielding)
/// @dev Only callable by the Protocol Adapter. Handles deposits (shield) and withdrawals (unshield).
contract ERC20Forwarder is IForwarder, Ownable {
    using SafeERC20 for IERC20;

    /// @notice The ERC20 token this forwarder handles
    IERC20 public immutable token;

    /// @notice The Protocol Adapter address (set as owner)
    address public immutable protocolAdapter;

    /// @notice Emitted when tokens are deposited (shielded)
    event Deposited(address indexed from, uint256 amount);

    /// @notice Emitted when tokens are withdrawn (unshielded)
    event Withdrawn(address indexed to, uint256 amount);

    /// @notice Error thrown when caller is not the Protocol Adapter
    error OnlyProtocolAdapter();

    /// @notice Error thrown when an unsupported function selector is called
    error UnsupportedSelector(bytes4 selector);

    /// @notice Error thrown when recipient validation fails
    error InvalidRecipient(address expected, address actual);

    constructor(address _protocolAdapter, address _token) Ownable(_protocolAdapter) {
        protocolAdapter = _protocolAdapter;
        token = IERC20(_token);
    }

    /// @inheritdoc IForwarder
    /// @notice Forward a call - only callable by Protocol Adapter
    /// @param input The encoded function call (transferFrom for deposit, transfer for withdraw)
    /// @return output The call result (encoded bool)
    function forwardCall(
        bytes32, /* logicRef */
        bytes calldata input
    ) external override returns (bytes memory output) {
        if (msg.sender != protocolAdapter) {
            revert OnlyProtocolAdapter();
        }

        // Decode the function selector
        bytes4 selector = bytes4(input[:4]);

        if (selector == IERC20.transferFrom.selector) {
            // Deposit (Shield): transferFrom(from, to, amount)
            (address from, address to, uint256 amount) = abi.decode(input[4:], (address, address, uint256));

            // Ensure tokens are being transferred TO this forwarder (escrow)
            if (to != address(this)) {
                revert InvalidRecipient(address(this), to);
            }

            token.safeTransferFrom(from, address(this), amount);
            emit Deposited(from, amount);

            output = abi.encode(true);
        } else if (selector == IERC20.transfer.selector) {
            // Withdraw (Unshield): transfer(to, amount)
            (address to, uint256 amount) = abi.decode(input[4:], (address, uint256));

            token.safeTransfer(to, amount);
            emit Withdrawn(to, amount);

            output = abi.encode(true);
        } else {
            revert UnsupportedSelector(selector);
        }
    }

    /// @notice Get the token address this forwarder handles
    function getToken() external view returns (address) {
        return address(token);
    }

    /// @notice Get the balance of tokens held by this forwarder
    function getBalance() external view returns (uint256) {
        return token.balanceOf(address(this));
    }
}

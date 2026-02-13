// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

/// @title Counter
/// @notice Example contract used as the default benchmark target.
/// @dev Includes intentional unused variables and locals to produce
///      diagnostics warnings from LSP servers.
contract Counter {
    /// @notice The current count.
    uint256 public number;

    /// @dev Unused state variable — should trigger a warning.
    uint256 private unused;

    /// @dev Unused state variable — should trigger a warning.
    address private owner;

    /// @notice Set the counter to a specific value.
    /// @param newNumber The new value.
    function setNumber(uint256 newNumber) public {
        uint256 old = number; // unused local — should trigger a warning
        number = newNumber;
    }

    /// @notice Increment the counter by one.
    function increment() public {
        number++;
    }

    /// @notice Reset the counter to zero.
    function reset() public {
        uint256 temp; // unused local — should trigger a warning
        number = 0;
    }
}

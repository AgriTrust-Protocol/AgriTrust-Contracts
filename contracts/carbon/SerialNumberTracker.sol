// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Tracks retired carbon-credit serial numbers using compact bitmaps per token ID.
contract SerialNumberTracker {
    mapping(bytes32 => mapping(uint256 => uint256)) private retiredBitMaps;

    event SerialRangeRetired(bytes32 indexed tokenId, uint256 indexed startSerial, uint256 amount);

    error SerialAlreadyRetired(bytes32 tokenId, uint256 serialNumber);

    function _retireSerialRange(bytes32 tokenId, uint256 startSerial, uint256 amount) internal {
        for (uint256 serial = startSerial; serial < startSerial + amount; serial++) {
            uint256 bucket = serial >> 8;
            uint256 mask = 1 << (serial & 255);
            if (retiredBitMaps[tokenId][bucket] & mask != 0) revert SerialAlreadyRetired(tokenId, serial);
            retiredBitMaps[tokenId][bucket] |= mask;
        }
        emit SerialRangeRetired(tokenId, startSerial, amount);
    }

    function isSerialRetired(bytes32 tokenId, uint256 serialNumber) public view returns (bool) {
        uint256 bucket = serialNumber >> 8;
        uint256 mask = 1 << (serialNumber & 255);
        return retiredBitMaps[tokenId][bucket] & mask != 0;
    }
}

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Minimal ERC-721-style retirement certificate NFT minted by the credit registry.
contract RetirementCertificate {
    struct Certificate {
        bytes32 creditTokenId;
        uint256 amount;
        address retiree;
        string retireeInfo;
        uint256 issuedAt;
        string metadataURI;
    }

    string public constant name = "AgriTrust Carbon Retirement Certificate";
    string public constant symbol = "ATCRC";
    address public immutable registry;
    uint256 public nextCertificateId = 1;

    mapping(uint256 => address) public ownerOf;
    mapping(uint256 => Certificate) public certificates;

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    event CertificateMinted(uint256 indexed certificateId, address indexed retiree, bytes32 indexed creditTokenId, uint256 amount, string retireeInfo, string metadataURI);

    error OnlyRegistry();
    error NonTransferable();

    constructor(address registry_) {
        registry = registry_;
    }

    function mintCertificate(address retiree, bytes32 creditTokenId, uint256 amount, string calldata retireeInfo, string calldata metadataURI) external returns (uint256 certificateId) {
        if (msg.sender != registry) revert OnlyRegistry();
        certificateId = nextCertificateId++;
        ownerOf[certificateId] = retiree;
        certificates[certificateId] = Certificate(creditTokenId, amount, retiree, retireeInfo, block.timestamp, metadataURI);
        emit Transfer(address(0), retiree, certificateId);
        emit CertificateMinted(certificateId, retiree, creditTokenId, amount, retireeInfo, metadataURI);
    }

    function tokenURI(uint256 certificateId) external view returns (string memory) {
        return certificates[certificateId].metadataURI;
    }

    function transferFrom(address, address, uint256) external pure {
        revert NonTransferable();
    }
}

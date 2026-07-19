// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "./CarbonOracle.sol";
import "./RetirementCertificate.sol";
import "./SerialNumberTracker.sol";

/// @notice ERC-1155-like carbon credit registry with retirement tracking.
contract CreditRegistry is SerialNumberTracker {
    struct CreditMetadata {
        string methodology;
        uint256 vintage;
        string region;
        bytes32 projectId;
        string verificationReportCID;
        bytes32 oracleAttestationId;
        uint256 expiresAt;
        uint256 nextMintSerial;
        uint256 nextRetireSerial;
        bool exists;
    }

    address public admin;
    CarbonOracle public oracle;
    RetirementCertificate public retirementCertificate;

    mapping(address => bool) public carbonVerifier;
    mapping(bytes32 => CreditMetadata) public creditMetadata;
    mapping(address => mapping(uint256 => uint256)) public balanceOf;
    mapping(address => mapping(address => bool)) public isApprovedForAll;

    event TransferSingle(address indexed operator, address indexed from, address indexed to, uint256 id, uint256 value);
    event TransferBatch(address indexed operator, address indexed from, address indexed to, uint256[] ids, uint256[] values);
    event ApprovalForAll(address indexed account, address indexed operator, bool approved);
    event CreditMinted(bytes32 indexed tokenId, address indexed to, bytes32 indexed projectId, uint256 amount, string reportCID, bytes32 attestationId, uint256 startSerial);
    event CreditRetired(bytes32 indexed tokenId, address indexed retiree, uint256 amount, uint256 certificateId, uint256 startSerial, string retireeInfo);

    error OnlyAdmin();
    error OnlyCarbonVerifier();
    error InvalidOracleAttestation();
    error LengthMismatch();
    error InsufficientBalance();
    error ExpiredCredit();
    error NotApproved();

    constructor(CarbonOracle oracle_) {
        admin = msg.sender;
        oracle = oracle_;
        carbonVerifier[msg.sender] = true;
        retirementCertificate = new RetirementCertificate(address(this));
    }

    modifier onlyAdmin() { if (msg.sender != admin) revert OnlyAdmin(); _; }
    modifier onlyVerifier() { if (!carbonVerifier[msg.sender]) revert OnlyCarbonVerifier(); _; }

    function setCarbonVerifier(address verifier, bool authorized) external onlyAdmin {
        carbonVerifier[verifier] = authorized;
    }

    function tokenId(string memory methodology, uint256 vintage, string memory region, bytes32 projectId) public pure returns (bytes32) {
        return keccak256(abi.encode(methodology, vintage, region, projectId));
    }

    function mint(address to, bytes32 projectId, string memory methodology, uint256 vintage, string memory region, uint256 amount, string memory reportCID, bytes32 attestationId, uint256 expiresAt) public onlyVerifier returns (uint256 id) {
        if (!oracle.isAttested(attestationId)) revert InvalidOracleAttestation();
        bytes32 computed = tokenId(methodology, vintage, region, projectId);
        id = uint256(computed);
        CreditMetadata storage meta = creditMetadata[computed];
        if (!meta.exists) {
            meta.methodology = methodology;
            meta.vintage = vintage;
            meta.region = region;
            meta.projectId = projectId;
            meta.verificationReportCID = reportCID;
            meta.oracleAttestationId = attestationId;
            meta.expiresAt = expiresAt;
            meta.exists = true;
        }
        uint256 startSerial = meta.nextMintSerial;
        meta.nextMintSerial += amount;
        balanceOf[to][id] += amount;
        emit TransferSingle(msg.sender, address(0), to, id, amount);
        emit CreditMinted(computed, to, projectId, amount, reportCID, attestationId, startSerial);
    }

    function mintBatch(address to, bytes32 projectId, string[] calldata methodologies, uint256[] calldata vintages, string[] calldata regions, uint256[] calldata amounts, string[] calldata reportCIDs, bytes32[] calldata attestationIds, uint256[] calldata expiries) external onlyVerifier returns (uint256[] memory ids) {
        uint256 length = methodologies.length;
        if (length != amounts.length || length != vintages.length || length != regions.length || length != reportCIDs.length || length != attestationIds.length || length != expiries.length) revert LengthMismatch();
        ids = new uint256[](length);
        for (uint256 i; i < length; i++) ids[i] = mint(to, projectId, methodologies[i], vintages[i], regions[i], amounts[i], reportCIDs[i], attestationIds[i], expiries[i]);
    }

    function setApprovalForAll(address operator, bool approved) external {
        isApprovedForAll[msg.sender][operator] = approved;
        emit ApprovalForAll(msg.sender, operator, approved);
    }

    function safeTransferFrom(address from, address to, uint256 id, uint256 amount, bytes calldata) external {
        if (from != msg.sender && !isApprovedForAll[from][msg.sender]) revert NotApproved();
        if (balanceOf[from][id] < amount) revert InsufficientBalance();
        balanceOf[from][id] -= amount;
        balanceOf[to][id] += amount;
        emit TransferSingle(msg.sender, from, to, id, amount);
    }

    function retire(uint256 id, uint256 amount, string calldata retireeInfo) public returns (uint256 certificateId) {
        if (balanceOf[msg.sender][id] < amount) revert InsufficientBalance();
        bytes32 creditId = bytes32(id);
        CreditMetadata storage meta = creditMetadata[creditId];
        if (meta.expiresAt != 0 && block.timestamp > meta.expiresAt) revert ExpiredCredit();
        balanceOf[msg.sender][id] -= amount;
        uint256 startSerial = meta.nextRetireSerial;
        meta.nextRetireSerial += amount;
        _retireSerialRange(creditId, startSerial, amount);
        string memory uri = string(abi.encodePacked("ipfs://retirement/", retireeInfo));
        certificateId = retirementCertificate.mintCertificate(msg.sender, creditId, amount, retireeInfo, uri);
        emit TransferSingle(msg.sender, msg.sender, address(0), id, amount);
        emit CreditRetired(creditId, msg.sender, amount, certificateId, startSerial, retireeInfo);
    }

    function retireBatch(uint256[] calldata ids, uint256[] calldata amounts, string calldata retireeInfo) external returns (uint256[] memory certificateIds) {
        if (ids.length != amounts.length) revert LengthMismatch();
        certificateIds = new uint256[](ids.length);
        for (uint256 i; i < ids.length; i++) certificateIds[i] = retire(ids[i], amounts[i], retireeInfo);
    }
}

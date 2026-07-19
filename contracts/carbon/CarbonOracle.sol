// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Stores immutable carbon-registry attestations from authorized oracle signers.
contract CarbonOracle {
    enum Registry { Verra, GoldStandard }

    struct Attestation {
        Registry registry;
        bytes32 projectId;
        bytes32 reportHash;
        string reportCID;
        address verifier;
        uint256 attestedAt;
        bool exists;
    }

    address public admin;
    mapping(address => bool) public authorizedVerifier;
    mapping(bytes32 => Attestation) public attestations;

    event VerifierAuthorizationUpdated(address indexed verifier, bool authorized);
    event OracleAttestation(bytes32 indexed attestationId, Registry indexed registry, bytes32 indexed projectId, bytes32 reportHash, string reportCID, address verifier);

    error OnlyAdmin();
    error UnauthorizedVerifier();
    error AttestationExists();

    constructor(address admin_) {
        admin = admin_;
        authorizedVerifier[admin_] = true;
    }

    modifier onlyAdmin() {
        if (msg.sender != admin) revert OnlyAdmin();
        _;
    }

    function setVerifier(address verifier, bool authorized) external onlyAdmin {
        authorizedVerifier[verifier] = authorized;
        emit VerifierAuthorizationUpdated(verifier, authorized);
    }

    function attest(Registry registry, bytes32 projectId, bytes32 reportHash, string calldata reportCID) external returns (bytes32 attestationId) {
        if (!authorizedVerifier[msg.sender]) revert UnauthorizedVerifier();
        attestationId = keccak256(abi.encode(registry, projectId, reportHash, reportCID));
        if (attestations[attestationId].exists) revert AttestationExists();
        attestations[attestationId] = Attestation(registry, projectId, reportHash, reportCID, msg.sender, block.timestamp, true);
        emit OracleAttestation(attestationId, registry, projectId, reportHash, reportCID, msg.sender);
    }

    function isAttested(bytes32 attestationId) external view returns (bool) {
        return attestations[attestationId].exists;
    }
}

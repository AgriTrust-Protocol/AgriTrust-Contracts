const { DidResolver, sha256Hex } = require('../src/services/didResolver');

test('assembles DID document and verifies two final keys after rotation and revoke', async () => {
  const did = 'did:agritrust:GABC123';
  const stored = { id: did };
  const ipfsCid = 'ipfs://bafy-final';
  const events = [
    { name: 'DIDRegistered', documentHash: sha256Hex(stored), ipfsCid, alsoKnownAs: ['acct:farm-1@agritrust'] },
    { name: 'KeyAdded', keyId: 'key-1', keyType: 'Ed25519VerificationKey2020', publicKeyMultibase: 'z6Mk1' },
    { name: 'KeyAdded', keyId: 'key-2', keyType: 'Ed25519VerificationKey2020', publicKeyMultibase: 'z6Mk2' },
    { name: 'KeyAdded', keyId: 'key-3', keyType: 'Ed25519VerificationKey2020', publicKeyMultibase: 'z6Mk3' },
    { name: 'KeyAdded', keyId: 'key-1', keyType: 'Ed25519VerificationKey2020', publicKeyMultibase: 'z6Mk1-rotated' },
    { name: 'KeyRevoked', keyId: 'key-2' },
  ];
  const resolver = new DidResolver({
    registry: { getDidEvents: jest.fn().mockResolvedValue(events) },
    ipfs: { cat: jest.fn().mockResolvedValue(stored) },
  });

  const document = await resolver.resolveDID(did);

  expect(document.verificationMethod).toHaveLength(2);
  expect(document.verificationMethod.map((key) => key.id)).toEqual([`${did}#key-1`, `${did}#key-3`]);
  expect(document.verificationMethod[0].publicKeyMultibase).toBe('z6Mk1-rotated');
});

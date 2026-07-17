const crypto = require('crypto');

const DID_PREFIX = 'did:agritrust:';

function sha256Hex(document) {
  const payload = typeof document === 'string' ? document : JSON.stringify(document);
  return crypto.createHash('sha256').update(payload).digest('hex');
}

function normalizeHash(hash) {
  return String(hash || '').replace(/^0x/i, '').toLowerCase();
}

function eventName(event) {
  return event.type || event.name || event.topic || event.topic0;
}

function eventValue(event, key) {
  return event[key] || (event.args && event.args[key]);
}

function parseDid(did) {
  if (!did || !did.startsWith(DID_PREFIX)) {
    throw new Error('invalid AgriTrust DID');
  }
  const identity = did.slice(DID_PREFIX.length);
  if (!identity) {
    throw new Error('DID identity is required');
  }
  return identity;
}

class DidResolver {
  constructor({ registry, ipfs } = {}) {
    this.registry = registry;
    this.ipfs = ipfs;
  }

  async resolveDID(did) {
    const identity = parseDid(did);
    const events = await this.registry.getDidEvents(identity);
    const document = this.assembleDocument(did, identity, events);

    if (document.ipfsCid && this.ipfs && this.ipfs.cat) {
      const stored = await this.ipfs.cat(document.ipfsCid);
      const actualHash = sha256Hex(stored);
      if (normalizeHash(document.documentHash) !== actualHash) {
        throw new Error('DID document hash mismatch');
      }
    }

    return document;
  }

  assembleDocument(did, identity, events = []) {
    const verificationMethods = new Map();
    const services = new Map();
    const alsoKnownAs = [];
    let documentHash;
    let ipfsCid;

    for (const event of events) {
      const name = eventName(event);
      if (name === 'did_reg' || name === 'DIDRegistered') {
        documentHash = eventValue(event, 'documentHash');
        ipfsCid = eventValue(event, 'ipfsCid');
        for (const alias of eventValue(event, 'alsoKnownAs') || []) alsoKnownAs.push(alias);
      }
      if (name === 'did_doc' || name === 'DIDDocumentUpdated') {
        documentHash = eventValue(event, 'documentHash');
        ipfsCid = eventValue(event, 'ipfsCid');
      }
      if (name === 'key_add' || name === 'KeyAdded') {
        const id = eventValue(event, 'id') || eventValue(event, 'keyId');
        verificationMethods.set(id, {
          id: `${did}#${id}`,
          type: eventValue(event, 'keyType') || eventValue(event, 'type'),
          controller: did,
          publicKeyMultibase: eventValue(event, 'publicKeyMultibase'),
        });
      }
      if (name === 'key_rev' || name === 'KeyRevoked') {
        const id = eventValue(event, 'id') || eventValue(event, 'keyId');
        verificationMethods.delete(id);
      }
      if (name === 'svc_add' || name === 'ServiceAdded' || name === 'svc_upd' || name === 'ServiceUpdated') {
        const id = eventValue(event, 'id') || eventValue(event, 'serviceId');
        const existing = services.get(id) || {};
        services.set(id, {
          id: `${did}#${id}`,
          type: eventValue(event, 'serviceType') || existing.type,
          serviceEndpoint: eventValue(event, 'serviceEndpoint') || eventValue(event, 'endpoint'),
        });
      }
    }

    return {
      id: did,
      verificationMethod: [...verificationMethods.values()],
      service: [...services.values()],
      alsoKnownAs,
      documentHash,
      ipfsCid,
      controllerAddress: identity,
    };
  }
}

module.exports = { DidResolver, DID_PREFIX, parseDid, sha256Hex };

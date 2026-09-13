"use strict";

class Server {
  constructor(url) {
    this.url = url;
  }
  async simulateInvoke() {
    return {};
  }
}

class Contract {
  constructor(id) {
    this.id = id;
  }
  address() {
    return this.id;
  }
}

const xdr = {
  ScVal: {
    scvSymbol: (val) => val,
  },
};

module.exports = {
  SorobanRpc: { Server },
  Contract,
  xdr,
};

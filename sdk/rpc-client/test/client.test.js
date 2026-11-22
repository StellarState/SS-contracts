import assert from "node:assert/strict";
import test from "node:test";
import { Account, Keypair, rpc } from "@stellar/stellar-sdk";
import { InvoiceEscrowRpcClient } from "../dist/index.js";

const signer = Keypair.random();

function clientWithServer(server, options = {}) {
  const client = new InvoiceEscrowRpcClient({
    contractId: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M",
    rpcUrl: "http://localhost:8000/rpc",
    pollIntervalMs: 0,
    retryDelayMs: 0,
    ...options,
  });
  client.server = server;
  return client;
}

function fakeServer(sendResponses, transactionResponses) {
  let sendIndex = 0;
  let transactionIndex = 0;
  let accountCalls = 0;
  let sendCalls = 0;
  return {
    get accountCalls() {
      return accountCalls;
    },
    get sendCalls() {
      return sendCalls;
    },
    getAccount: async () => {
      accountCalls += 1;
      return new Account(signer.publicKey(), String(accountCalls));
    },
    prepareTransaction: async (transaction) => transaction,
    sendTransaction: async () => {
      sendCalls += 1;
      return sendResponses[Math.min(sendIndex++, sendResponses.length - 1)];
    },
    getTransaction: async () => transactionResponses[Math.min(transactionIndex++, transactionResponses.length - 1)],
  };
}

test("constructs a client with testnet as the default passphrase", () => {
  const client = new InvoiceEscrowRpcClient({
    contractId: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M",
    rpcUrl: "http://localhost:8000/rpc",
  });
  assert.equal(client.networkPassphrase, "Test SDF Network ; September 2015");
  assert.equal(client.contractId, "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M");
});

test("allows an explicit network passphrase", () => {
  const client = new InvoiceEscrowRpcClient({
    contractId: "contract",
    rpcUrl: "https://rpc.example.test",
    networkPassphrase: "Standalone Network ; February 2017",
  });
  assert.equal(client.networkPassphrase, "Standalone Network ; February 2017");
});

test("retries temporary rejection only after checking the original hash", async () => {
  const server = fakeServer(
    [
      { status: "TRY_AGAIN_LATER", hash: "first-hash" },
      { status: "PENDING", hash: "second-hash" },
    ],
    [
      { status: rpc.Api.GetTransactionStatus.NOT_FOUND, txHash: "first-hash" },
      { status: rpc.Api.GetTransactionStatus.NOT_FOUND, txHash: "first-hash" },
      { status: rpc.Api.GetTransactionStatus.SUCCESS, txHash: "second-hash", ledger: 12 },
    ],
  );
  const result = await clientWithServer(server, { pollAttempts: 1, submissionRetries: 1 }).submit("lock", [], signer);
  assert.equal(result.status, "confirmed");
  assert.equal(result.hash, "second-hash");
  assert.equal(server.sendCalls, 2);
  assert.equal(server.accountCalls, 2);
});

test("returns confirmed metadata after delayed confirmation", async () => {
  const server = fakeServer(
    [{ status: "PENDING", hash: "delayed-hash" }],
    [
      { status: rpc.Api.GetTransactionStatus.NOT_FOUND, txHash: "delayed-hash" },
      { status: rpc.Api.GetTransactionStatus.SUCCESS, txHash: "delayed-hash", ledger: 42 },
    ],
  );
  const result = await clientWithServer(server, { pollAttempts: 2 }).submit("lock", [], signer);
  assert.equal(result.status, "confirmed");
  assert.equal(result.ledger, 42);
  assert.equal(result.result.txHash, "delayed-hash");
});

test("surfaces terminal transaction failure", async () => {
  const server = fakeServer(
    [{ status: "PENDING", hash: "failed-hash" }],
    [{ status: rpc.Api.GetTransactionStatus.FAILED, txHash: "failed-hash" }],
  );
  const result = await clientWithServer(server, { pollAttempts: 1 }).submit("lock", [], signer);
  assert.equal(result.status, "failed");
  assert.equal(result.hash, "failed-hash");
});

test("returns diagnostics for an RPC terminal rejection", async () => {
  const server = fakeServer(
    [{ status: "ERROR", hash: "rejected-hash", errorResult: "result", diagnosticEvents: ["event"] }],
    [],
  );
  const result = await clientWithServer(server, { pollAttempts: 1 }).submit("lock", [], signer);
  assert.equal(result.status, "failed");
  assert.equal(result.hash, "rejected-hash");
  assert.equal(result.errorResult, "result");
  assert.deepEqual(result.diagnosticEvents, ["event"]);
});

test("returns unresolved with the original hash when polling expires", async () => {
  const server = fakeServer(
    [{ status: "PENDING", hash: "unresolved-hash" }],
    [{ status: rpc.Api.GetTransactionStatus.NOT_FOUND, txHash: "unresolved-hash" }],
  );
  const result = await clientWithServer(server, { pollAttempts: 2 }).submit("lock", [], signer);
  assert.equal(result.status, "unresolved");
  assert.equal(result.hash, "unresolved-hash");
  assert.equal(result.attempts, 2);
});

test("recovers an accepted temporary submission without consuming another sequence", async () => {
  const server = fakeServer(
    [{ status: "TRY_AGAIN_LATER", hash: "accepted-hash" }],
    [
      { status: rpc.Api.GetTransactionStatus.NOT_FOUND, txHash: "accepted-hash" },
      { status: rpc.Api.GetTransactionStatus.SUCCESS, txHash: "accepted-hash", ledger: 99 },
    ],
  );
  const result = await clientWithServer(server, { pollAttempts: 1, submissionRetries: 1 }).submit("lock", [], signer);
  assert.equal(result.status, "confirmed");
  assert.equal(result.hash, "accepted-hash");
  assert.equal(server.sendCalls, 1);
  assert.equal(server.accountCalls, 1);
});

test("can return accepted without polling", async () => {
  const server = fakeServer([{ status: "PENDING", hash: "accepted-only" }], []);
  const result = await clientWithServer(server, { pollAttempts: 0 }).submit("lock", [], signer);
  assert.deepEqual(result, { status: "accepted", hash: "accepted-only", sendStatus: "PENDING" });
});
